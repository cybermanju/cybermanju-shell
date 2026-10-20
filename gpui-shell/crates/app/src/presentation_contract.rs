//! Executable upstream-integration contract, NOT a live display observer.
//! Compiled only by `cargo test`. Input fixtures exercise protocol parsing and
//! bounded lifecycle logic; none are measurements or physical-display evidence.
//!
//! A future upstream adapter must supply these events from GPUI's ORIGINAL
//! connection/queue, and allocate keys only at the actual WSI buffer-commit
//! boundary. See developer/presentation-feedback.md for the remaining blockers.
use std::{collections::BTreeMap, collections::VecDeque, time::Duration};

use wayland_protocols::wp::presentation_time::client::{wp_presentation, wp_presentation_feedback};

const MAX_OUTSTANDING: usize = 64;
const MAX_RECORDS: usize = 4096;
const MAX_OUTPUT_ALIASES: usize = 16;
const DEADLINE: Duration = Duration::from_secs(2);

/// Immutable user data on the real feedback proxy, not a reusable wire object ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    connection_epoch: u64,
    surface_generation: u64,
    buffer_commit: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Output {
    /// Connection-local proxy ID, NOT a connector/DRM device identifier.
    proxy_id: u32,
    /// Registry global, if mapped by GPUI's original output registry.
    global: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Timestamp {
    seconds: u64,
    nanoseconds: u32,
}

#[derive(Debug, Eq, PartialEq)]
struct Presented {
    timestamp: Timestamp,
    refresh_ns: u32,
    /// Raw MSC; zero is retained, never interpreted as a known refresh count.
    sequence: u64,
    /// Preserve unknown bits rather than silently dropping future flags.
    flags: u32,
}

impl Presented {
    fn hardware_vsync(&self) -> bool {
        self.flags & 1 != 0
    }
    fn hardware_clock(&self) -> bool {
        self.flags & 2 != 0
    }
    fn hardware_completion(&self) -> bool {
        self.flags & 4 != 0
    }
    fn zero_copy(&self) -> bool {
        self.flags & 8 != 0
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome {
    Presented(Presented),
    Discarded,
    TimedOut,
    InvalidTimestamp,
    ConnectionLost,
}

#[derive(Debug)]
struct Record {
    key: Key,
    clock_id: Option<u32>,
    outputs: Vec<Output>,
    omitted_output_events: usize,
    outcome: Outcome,
}

struct Pending {
    requested_at: Duration,
    outputs: Vec<Output>,
    omitted_output_events: usize,
    timed_out: bool,
}

struct Contract {
    connection_epoch: u64,
    clock_id: Option<u32>,
    disabled: bool,
    pending: BTreeMap<Key, Pending>,
    records: VecDeque<Record>,
    dropped_records: usize,
    late_terminal_events: usize,
    untracked_events: usize,
}

impl Contract {
    fn new(connection_epoch: u64) -> Self {
        Self {
            connection_epoch,
            clock_id: None,
            disabled: false,
            pending: BTreeMap::new(),
            records: VecDeque::new(),
            dropped_records: 0,
            late_terminal_events: 0,
            untracked_events: 0,
        }
    }

    fn clock(&mut self, event: wp_presentation::Event) -> Result<(), &'static str> {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            if self.clock_id.is_some_and(|old| old != clk_id) {
                self.disabled = true;
                return Err("presentation clock changed on one connection");
            }
            self.clock_id = Some(clk_id);
        }
        Ok(())
    }

    /// Reservation contract only: this does NOT send a Wayland request.
    /// Original-queue adapter must reserve BEFORE constructing a feedback proxy.
    fn reserve(&mut self, key: Key, now: Duration) -> Result<(), &'static str> {
        if self.disabled || key.connection_epoch != self.connection_epoch {
            return Err("disabled or wrong connection");
        }
        if self.pending.contains_key(&key) {
            return Err("duplicate buffer-commit key");
        }
        if self.pending.len() >= MAX_OUTSTANDING {
            return Err("feedback budget exhausted; do not create another proxy");
        }
        self.pending.insert(
            key,
            Pending {
                requested_at: now,
                outputs: Vec::new(),
                omitted_output_events: 0,
                timed_out: false,
            },
        );
        Ok(())
    }

    /// Output registry lookup occurs in the original GPUI dispatcher. No second
    /// connection and no binding to another surface can supply this identity.
    fn sync_output(&mut self, key: Key, output: Output) {
        let Some(pending) = self.pending.get_mut(&key) else {
            self.untracked_events += 1;
            return;
        };
        if pending.timed_out || pending.outputs.contains(&output) {
            return;
        }
        if pending.outputs.len() == MAX_OUTPUT_ALIASES {
            pending.omitted_output_events += 1;
        } else {
            pending.outputs.push(output);
        }
    }

    /// Parses real generated Rust event TYPES, not a fabricated wire format.
    /// The future Dispatch adapter resolves SyncOutput via GPUI's registry and
    /// calls sync_output; terminal events enter here. No FPS is calculated.
    fn terminal(&mut self, key: Key, event: wp_presentation_feedback::Event) {
        let outcome = match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                seq_hi,
                seq_lo,
                flags,
            } => {
                if tv_nsec >= 1_000_000_000 {
                    Outcome::InvalidTimestamp
                } else {
                    Outcome::Presented(Presented {
                        timestamp: Timestamp {
                            seconds: (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo),
                            nanoseconds: tv_nsec,
                        },
                        refresh_ns: refresh,
                        sequence: (u64::from(seq_hi) << 32) | u64::from(seq_lo),
                        flags: flags.into(),
                    })
                }
            }
            wp_presentation_feedback::Event::Discarded => Outcome::Discarded,
            _ => return,
        };
        let Some(pending) = self.pending.remove(&key) else {
            self.untracked_events += 1;
            return;
        };
        if pending.timed_out {
            self.late_terminal_events += 1;
            return;
        }
        self.push(Record {
            key,
            clock_id: self.clock_id,
            outputs: pending.outputs,
            omitted_output_events: pending.omitted_output_events,
            outcome,
        });
    }

    /// Called by a monotonic event-loop timer EVEN WHEN NO RENDERS OCCUR.
    /// Feedback has no client destroy request! Expiry releases payloads, NOT
    /// the outstanding slot. Reuse only after the actual terminal destructor.
    fn expire(&mut self, now: Duration) {
        let mut expired = Vec::new();
        for (&key, pending) in &mut self.pending {
            if !pending.timed_out
                && now
                    .checked_sub(pending.requested_at)
                    .is_some_and(|age| age >= DEADLINE)
            {
                pending.timed_out = true;
                expired.push(Record {
                    key,
                    clock_id: self.clock_id,
                    outputs: std::mem::take(&mut pending.outputs),
                    omitted_output_events: pending.omitted_output_events,
                    outcome: Outcome::TimedOut,
                });
            }
        }
        for record in expired {
            self.push(record);
        }
    }

    fn connection_lost(&mut self) {
        self.disabled = true;
        for (key, pending) in std::mem::take(&mut self.pending) {
            if !pending.timed_out {
                self.push(Record {
                    key,
                    clock_id: self.clock_id,
                    outputs: pending.outputs,
                    omitted_output_events: pending.omitted_output_events,
                    outcome: Outcome::ConnectionLost,
                });
            }
        }
    }

    fn push(&mut self, record: Record) {
        if self.records.len() == MAX_RECORDS {
            self.records.pop_front();
            self.dropped_records += 1;
        }
        self.records.push_back(record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wayland_client::WEnum;

    fn key(commit: u64) -> Key {
        Key {
            connection_epoch: 1,
            surface_generation: 2,
            buffer_commit: commit,
        }
    }
    fn presented(nsec: u32, flags: u32) -> wp_presentation_feedback::Event {
        wp_presentation_feedback::Event::Presented {
            tv_sec_hi: u32::MAX,
            tv_sec_lo: u32::MAX,
            tv_nsec: nsec,
            refresh: 0,
            seq_hi: 3,
            seq_lo: 4,
            flags: WEnum::Unknown(flags),
        }
    }
    fn setup() -> Contract {
        let mut contract = Contract::new(1);
        contract
            .clock(wp_presentation::Event::ClockId { clk_id: 1 })
            .unwrap();
        contract.reserve(key(1), Duration::ZERO).unwrap();
        contract
    }

    #[test]
    fn combines_full_width_fields_without_nanosecond_overflow() {
        let mut c = setup();
        c.terminal(key(1), presented(999_999_999, 15));
        let r = c.records.back().unwrap();
        assert_eq!(r.key, key(1));
        assert_eq!(r.clock_id, Some(1));
        let Outcome::Presented(p) = &r.outcome else {
            panic!("expected parsed fixture")
        };
        assert_eq!(p.timestamp.seconds, u64::MAX);
        assert_eq!(p.timestamp.nanoseconds, 999_999_999);
        assert_eq!(p.sequence, (3_u64 << 32) | 4);
        assert_eq!(p.refresh_ns, 0);
        assert!(
            p.hardware_vsync() && p.hardware_clock() && p.hardware_completion() && p.zero_copy()
        );
    }

    #[test]
    fn preserves_unknown_flags_and_does_not_invent_async_evidence() {
        let mut c = setup();
        c.terminal(key(1), presented(0, 0x8000_0000));
        let Outcome::Presented(p) = &c.records[0].outcome else {
            panic!()
        };
        assert_eq!(p.flags, 0x8000_0000);
        assert!(!p.hardware_vsync()); // NOT proof of async scanout or tearing.
        assert!(!p.hardware_clock() && !p.hardware_completion());
    }

    #[test]
    fn known_bitflags_and_zero_msc_are_retained() {
        let mut c = setup();
        c.terminal(
            key(1),
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi: 0,
                tv_sec_lo: 1,
                tv_nsec: 0,
                refresh: 16_666_667,
                seq_hi: 0,
                seq_lo: 0,
                flags: WEnum::Value(wp_presentation_feedback::Kind::Vsync),
            },
        );
        let Outcome::Presented(p) = &c.records[0].outcome else {
            panic!()
        };
        assert_eq!(p.flags, 1);
        assert_eq!(p.sequence, 0);
        assert_eq!(p.refresh_ns, 16_666_667);
    }

    #[test]
    fn invalid_nanoseconds_are_not_measurements() {
        let mut c = setup();
        c.terminal(key(1), presented(1_000_000_000, 7));
        assert_eq!(c.records[0].outcome, Outcome::InvalidTimestamp);
    }

    #[test]
    fn discard_has_no_timestamp_and_releases_slot() {
        let mut c = setup();
        c.terminal(key(1), wp_presentation_feedback::Event::Discarded);
        assert_eq!(c.records[0].outcome, Outcome::Discarded);
        assert!(c.pending.is_empty());
    }

    #[test]
    fn output_aliases_are_deduplicated_bounded_and_connection_local() {
        let mut c = setup();
        for proxy_id in 1..=20 {
            let output = Output {
                proxy_id,
                global: Some(9),
            };
            c.sync_output(key(1), output);
            if proxy_id <= 16 {
                c.sync_output(key(1), output);
            }
        }
        c.sync_output(
            key(1),
            Output {
                proxy_id: 20,
                global: Some(9),
            },
        );
        c.terminal(key(1), presented(0, 0));
        assert_eq!(c.records[0].outputs.len(), MAX_OUTPUT_ALIASES);
        assert_eq!(c.records[0].omitted_output_events, 5);
        assert!(c.records[0].outputs.iter().all(|o| o.global == Some(9)));
    }

    #[test]
    fn no_output_or_clock_is_preserved_as_missing_not_guessed() {
        let mut c = Contract::new(1);
        c.reserve(key(1), Duration::ZERO).unwrap();
        c.terminal(key(1), presented(0, 0));
        assert!(c.records[0].clock_id.is_none());
        assert!(c.records[0].outputs.is_empty());
    }

    #[test]
    fn clock_changes_disable_further_requests() {
        let mut c = setup();
        assert!(
            c.clock(wp_presentation::Event::ClockId { clk_id: 2 })
                .is_err()
        );
        assert_eq!(c.clock_id, Some(1));
        assert!(c.reserve(key(2), Duration::ZERO).is_err());
    }

    #[test]
    fn timeout_is_not_discard_and_late_destructor_is_not_a_second_sample() {
        let mut c = setup();
        c.expire(Duration::from_millis(1999));
        assert!(c.records.is_empty());
        c.expire(DEADLINE);
        c.expire(DEADLINE * 2);
        assert_eq!(c.records.len(), 1);
        assert_eq!(c.records[0].outcome, Outcome::TimedOut);
        assert_eq!(c.pending.len(), 1);
        c.terminal(key(1), presented(0, 7));
        assert_eq!(c.records.len(), 1);
        assert_eq!(c.late_terminal_events, 1);
        assert!(c.pending.is_empty());
    }

    #[test]
    fn unresolved_timeouts_cannot_leak_unbounded_protocol_objects() {
        let mut c = Contract::new(1);
        for n in 0..MAX_OUTSTANDING as u64 {
            c.reserve(key(n), Duration::ZERO).unwrap();
        }
        c.expire(DEADLINE);
        assert!(c.reserve(key(1000), DEADLINE).is_err());
        assert_eq!(c.pending.len(), MAX_OUTSTANDING);
        c.terminal(key(0), wp_presentation_feedback::Event::Discarded);
        c.reserve(key(1000), DEADLINE).unwrap();
        assert_eq!(c.pending.len(), MAX_OUTSTANDING);
    }

    #[test]
    fn duplicate_foreign_and_out_of_order_keys_do_not_mix_surfaces() {
        let mut c = setup();
        assert!(c.reserve(key(1), Duration::ZERO).is_err());
        let foreign = Key {
            connection_epoch: 99,
            ..key(1)
        };
        assert!(c.reserve(foreign, Duration::ZERO).is_err());
        let other_surface = Key {
            surface_generation: 3,
            ..key(1)
        };
        c.reserve(other_surface, Duration::ZERO).unwrap();
        c.terminal(other_surface, wp_presentation_feedback::Event::Discarded);
        c.terminal(key(1), presented(0, 1));
        c.terminal(key(1), presented(0, 1));
        c.terminal(foreign, presented(0, 1));
        assert_eq!(c.records.len(), 2);
        assert_eq!(c.records[0].key, other_surface);
        assert_eq!(c.records[1].key, key(1));
        assert_eq!(c.untracked_events, 2);
    }

    #[test]
    fn record_ring_is_bounded_and_reports_loss() {
        let mut c = Contract::new(1);
        for n in 0..(MAX_RECORDS + 3) as u64 {
            c.reserve(key(n), Duration::ZERO).unwrap();
            c.terminal(key(n), wp_presentation_feedback::Event::Discarded);
        }
        assert_eq!(c.records.len(), MAX_RECORDS);
        assert_eq!(c.dropped_records, 3);
    }

    #[test]
    fn monotonic_deadlines_ignore_backwards_fixture_time() {
        let mut c = Contract::new(1);
        c.reserve(key(1), Duration::from_secs(10)).unwrap();
        c.expire(Duration::ZERO);
        assert!(c.records.is_empty());
    }

    #[test]
    fn disconnect_retires_pending_without_claiming_protocol_discard() {
        let mut c = setup();
        c.connection_lost();
        assert!(c.pending.is_empty());
        assert_eq!(c.records[0].outcome, Outcome::ConnectionLost);
        assert!(c.reserve(key(2), Duration::ZERO).is_err());
    }
}
