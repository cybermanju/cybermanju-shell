// CyberManju OS — per-provider concurrency caps (AGENT-1)
//
// The sync pipeline fans out with rayon (`max_concurrent_uploads`), which
// today means N simultaneous requests against one provider — a guaranteed
// 429 on GitHub/Google. Every backend method
// takes a `Permit` for its provider first; the gate blocks (bounded) until a
// slot frees up, so one provider never sees more in-flight operations than
// its limit, no matter how wide the pool is.
//
// <<< AGENT-1 RATE LIMIT >>>

use cybermanju_types::sync::SyncBackendType;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long a caller may wait for a slot before the operation is refused
/// with `rate_limited:` instead of hanging the sync forever.
const MAX_WAIT: Duration = Duration::from_secs(120);
/// Wake-up cadence while waiting, so a lost notification can never stall us.
const WAIT_SLICE: Duration = Duration::from_secs(5);

struct Gate {
    free: Mutex<usize>,
    limit: usize,
    cv: Condvar,
}

impl Gate {
    const fn new(limit: usize) -> Self {
        Self {
            free: Mutex::new(limit),
            limit,
            cv: Condvar::new(),
        }
    }
}

// Limits: Git/Drive APIs are generous but shared; the gates keep one
// provider from ever seeing more in-flight operations than its limit,
// no matter how wide the rayon pool is.
static LOCAL_GATE: Gate = Gate::new(64);
static GITHUB_GATE: Gate = Gate::new(4);
static GITLAB_GATE: Gate = Gate::new(4);
static DRIVE_GATE: Gate = Gate::new(4);

fn gate_for(backend_type: &SyncBackendType) -> &'static Gate {
    match backend_type {
        SyncBackendType::Local => &LOCAL_GATE,
        SyncBackendType::GitHub => &GITHUB_GATE,
        SyncBackendType::GitLab => &GITLAB_GATE,
        SyncBackendType::GoogleDrive => &DRIVE_GATE,
    }
}

/// RAII slot in a provider's gate. Released on drop (including panics), so a
/// failed transfer can never wedge the semaphore.
pub struct Permit {
    gate: &'static Gate,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut free = self.gate.free.lock().unwrap_or_else(|p| p.into_inner());
        *free += 1;
        self.gate.cv.notify_one();
    }
}

/// Acquire one in-flight slot for `backend_type`, blocking up to
/// [`MAX_WAIT`]. Returns `rate_limited: ...` if the provider stays saturated
/// — honest failure beats an unbounded queue.
pub fn acquire(backend_type: &SyncBackendType) -> Result<Permit, String> {
    let gate = gate_for(backend_type);
    let started = Instant::now();
    let mut free = gate.free.lock().unwrap_or_else(|p| p.into_inner());

    while *free == 0 {
        if started.elapsed() >= MAX_WAIT {
            return Err(format!(
                "rate_limited: {} is at its concurrency limit of {} for over {}s",
                backend_type,
                gate.limit,
                MAX_WAIT.as_secs()
            ));
        }
        let (guard, _timed_out) = gate
            .cv
            .wait_timeout(free, WAIT_SLICE)
            .unwrap_or_else(|p| p.into_inner());
        free = guard;
    }

    *free -= 1;
    drop(free);
    Ok(Permit { gate })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permits_are_returned_on_drop() {
        let backend = SyncBackendType::GoogleDrive; // limit 4
        let a = acquire(&backend).expect("first permit");
        let b = acquire(&backend).expect("second permit");
        drop(a);
        let c = acquire(&backend).expect("slot released again");
        drop(b);
        drop(c);
    }

    #[test]
    fn the_gate_is_not_a_mutex_around_everything() {
        let backend = SyncBackendType::GoogleDrive; // limit 4
        let permits: Vec<Permit> = (0..4).map(|_| acquire(&backend).expect("permit")).collect();
        assert_eq!(permits.len(), 4);
        // Dropping frees everything for the next caller.
        drop(permits);
        assert!(acquire(&backend).is_ok());
    }
}
