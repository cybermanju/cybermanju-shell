//! Pure statistics for observations of Bar::render, NOT GPU/presentation timing.
//! No timers or CPU loops generate samples. Idle gaps are deliberately retained.

use std::{collections::VecDeque, time::Duration};

const MAX_SAMPLES: usize = 4096;

pub fn parse_probe_seconds(value: &str) -> Option<u64> {
    value
        .parse::<u64>()
        .ok()
        .filter(|seconds| (1..=60).contains(seconds))
}

pub fn probe_active(elapsed: Duration, seconds: Option<u64>) -> bool {
    seconds.is_some_and(|seconds| elapsed < Duration::from_secs(seconds))
}

#[derive(Default)]
pub struct Samples {
    last_entry: Option<Duration>,
    samples: VecDeque<(Option<Duration>, Duration)>,
    dropped: usize,
    invalid_intervals: usize,
}

#[derive(Debug)]
pub struct Distribution {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug)]
pub struct Summary {
    pub entries: usize,
    pub intervals: usize,
    pub dropped: usize,
    pub invalid_intervals: usize,
    pub render_entry_rate_hz: Option<f64>,
    pub entry_interval: Option<Distribution>,
    pub view_build: Option<Distribution>,
}

fn distribution(values: impl Iterator<Item = Duration>) -> Option<Distribution> {
    let mut values: Vec<_> = values.collect();
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    // Nearest-rank percentiles, including the tail for very small samples.
    let percentile = |p: usize| values[(values.len() * p).div_ceil(100) - 1].as_secs_f64() * 1000.;
    Some(Distribution {
        p50_ms: percentile(50),
        p95_ms: percentile(95),
        p99_ms: percentile(99),
        max_ms: values.last().unwrap().as_secs_f64() * 1000.,
    })
}

impl Samples {
    pub fn record(&mut self, entry: Duration, view_build: Duration) {
        let gap = self.last_entry.and_then(|previous| {
            let gap = entry.checked_sub(previous).filter(|gap| !gap.is_zero());
            if gap.is_none() {
                self.invalid_intervals += 1;
            }
            gap
        });
        self.last_entry = Some(entry);
        if self.samples.len() == MAX_SAMPLES {
            self.samples.pop_front();
            self.dropped += 1;
        }
        self.samples.push_back((gap, view_build));
    }

    pub fn take_summary(&mut self) -> Summary {
        let intervals: Vec<_> = self.samples.iter().filter_map(|sample| sample.0).collect();
        let seconds: f64 = intervals.iter().map(Duration::as_secs_f64).sum();
        let result = Summary {
            entries: self.samples.len(),
            intervals: intervals.len(),
            dropped: self.dropped,
            invalid_intervals: self.invalid_intervals,
            render_entry_rate_hz: (!intervals.is_empty() && seconds > 0.)
                .then(|| intervals.len() as f64 / seconds),
            entry_interval: distribution(intervals.into_iter()),
            view_build: distribution(self.samples.iter().map(|sample| sample.1)),
        };
        self.samples.clear();
        self.dropped = 0;
        self.invalid_intervals = 0;
        result
    }
}

/// GpuSpecs exposes CPU emulation, but not adapter type, backend or PCI identity.
/// Unknown/non-CPU is therefore NOT evidence of a physical accelerated device.
pub fn renderer_class(software_emulated: bool, device: &str, driver: &str) -> &'static str {
    let description = format!("{device} {driver}").to_ascii_lowercase();
    if software_emulated
        || [
            "llvmpipe",
            "lavapipe",
            "softpipe",
            "swiftshader",
            "software rasterizer",
        ]
        .iter()
        .any(|name| description.contains(name))
    {
        "software_cpu"
    } else if [
        "virgl",
        "virtio",
        "virtual",
        "venus",
        "svga",
        "vmware",
        "gfxstream",
    ]
    .iter()
    .any(|name| description.contains(name))
    {
        "virtual_or_remote"
    } else {
        "unverified_non_cpu"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn probe_is_opt_in_bounded_and_expires_at_deadline() {
        assert_eq!(parse_probe_seconds("1"), Some(1));
        assert_eq!(parse_probe_seconds("60"), Some(60));
        for value in ["", "0", "61", "-1", "1.5", "true", "999999999999999999999"] {
            assert_eq!(parse_probe_seconds(value), None);
        }
        assert!(!probe_active(ms(0), None));
        assert!(probe_active(ms(999), Some(1)));
        assert!(!probe_active(ms(1000), Some(1)));
        assert!(!probe_active(ms(2000), Some(1)));
    }

    #[test]
    fn no_frames_or_one_entry_cannot_establish_a_rate() {
        let mut samples = Samples::default();
        assert!(samples.take_summary().render_entry_rate_hz.is_none());
        samples.record(ms(0), ms(2));
        let summary = samples.take_summary();
        assert_eq!(summary.entries, 1);
        assert!(summary.render_entry_rate_hz.is_none());
        assert_eq!(summary.view_build.unwrap().p95_ms, 2.);
    }

    #[test]
    fn rate_uses_interval_count_not_frame_count() {
        let mut samples = Samples::default();
        for n in 0..=10 {
            samples.record(ms(n * 10), ms(1));
        }
        let summary = samples.take_summary();
        assert_eq!(summary.intervals, 10);
        assert!((summary.render_entry_rate_hz.unwrap() - 100.).abs() < 0.00001);
        assert_eq!(summary.entry_interval.unwrap().p99_ms, 10.);
    }

    #[test]
    fn includes_idle_gaps_and_tail_instead_of_hiding_them() {
        let mut samples = Samples::default();
        for t in [0, 10, 20, 1020] {
            samples.record(ms(t), ms(1));
        }
        let summary = samples.take_summary();
        assert!((summary.render_entry_rate_hz.unwrap() - 3. / 1.02).abs() < 0.00001);
        let intervals = summary.entry_interval.unwrap();
        assert_eq!(intervals.p50_ms, 10.);
        assert_eq!(intervals.p95_ms, 1000.);
        assert_eq!(intervals.max_ms, 1000.);
    }

    #[test]
    fn interval_crossing_report_boundary_is_preserved() {
        let mut samples = Samples::default();
        samples.record(ms(0), ms(1));
        samples.record(ms(10), ms(1));
        samples.take_summary();
        samples.record(ms(30), ms(2));
        let summary = samples.take_summary();
        assert_eq!(summary.intervals, 1);
        assert_eq!(summary.render_entry_rate_hz, Some(50.));
    }

    #[test]
    fn equal_or_backwards_time_never_creates_infinite_rates() {
        let mut samples = Samples::default();
        for t in [10, 10, 5] {
            samples.record(ms(t), ms(0));
        }
        let summary = samples.take_summary();
        assert_eq!(summary.invalid_intervals, 2);
        assert_eq!(summary.intervals, 0);
        assert!(summary.render_entry_rate_hz.is_none());
    }

    #[test]
    fn memory_is_bounded_and_truncation_is_reported() {
        let mut samples = Samples::default();
        for n in 0..MAX_SAMPLES + 20 {
            samples.record(ms(n as u64), ms(1));
        }
        let summary = samples.take_summary();
        assert_eq!(summary.entries, MAX_SAMPLES);
        assert_eq!(summary.dropped, 20);
        assert_eq!(samples.take_summary().entries, 0);
    }

    #[test]
    fn software_and_virtual_devices_cannot_be_classed_as_physical() {
        for name in ["llvmpipe", "LAVAPIPE", "SwiftShader", "softpipe"] {
            assert_eq!(renderer_class(false, name, ""), "software_cpu");
        }
        assert_eq!(renderer_class(true, "unknown", ""), "software_cpu");
        for name in ["virtio GPU", "Venus", "VMware SVGA", "virgl", "Virtual GPU"] {
            assert_eq!(renderer_class(false, name, ""), "virtual_or_remote");
        }
        assert_eq!(
            renderer_class(false, "NVIDIA RTX", "NVIDIA"),
            "unverified_non_cpu"
        );
        assert_eq!(renderer_class(false, "", ""), "unverified_non_cpu");
    }
}
