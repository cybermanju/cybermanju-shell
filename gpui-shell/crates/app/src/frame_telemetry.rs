//! Opt-in, per-bar observations, deliberately not a presentation benchmark.
use std::time::{Duration, Instant};

use gpui::Window;
use serde_json::{Value, json};

use crate::frame_metrics::{
    Distribution, Samples, parse_probe_seconds, probe_active, renderer_class,
};

pub struct FrameTelemetry {
    start: Instant,
    last_report: Instant,
    probe_seconds: Option<u64>,
    probe_finished: bool,
    samples: Samples,
}

impl FrameTelemetry {
    pub fn from_env() -> Option<Self> {
        let probe_seconds = match std::env::var("GPUI_SHELL_FRAME_PROBE_SECONDS") {
            Ok(value) => match parse_probe_seconds(&value) {
                Some(seconds) => Some(seconds),
                None => {
                    tracing::warn!(
                        "Ignoring GPUI_SHELL_FRAME_PROBE_SECONDS: expected integer 1..=60"
                    );
                    None
                }
            },
            Err(_) => None,
        };
        if probe_seconds.is_none()
            && std::env::var("GPUI_SHELL_FRAME_TELEMETRY").as_deref() != Ok("1")
        {
            return None;
        }
        let now = Instant::now();
        Some(Self {
            start: now,
            last_report: now,
            probe_seconds,
            probe_finished: false,
            samples: Samples::default(),
        })
    }

    /// Called once after building this bar's element tree (CPU-side wall time).
    /// This is not thread CPU time and may include descheduling. It excludes
    /// layout, paint, GPU submission/completion, compositor work or scanout.
    pub fn record(&mut self, entry: Instant, window: &Window) {
        let view_build = entry.elapsed();
        let since_start = entry.duration_since(self.start);
        self.samples.record(since_start, view_build);
        let probing = probe_active(since_start, self.probe_seconds);
        if probing {
            // Real GPUI invalidation; no synthetic samples or timer-based loop.
            // Explicit diagnostic only: passive telemetry never requests redraws.
            window.request_animation_frame();
        }
        let probe_just_finished = self.probe_seconds.is_some() && !probing && !self.probe_finished;
        if probe_just_finished {
            self.probe_finished = true;
        }
        if entry.duration_since(self.last_report) < Duration::from_secs(1) && !probe_just_finished {
            return;
        }
        self.last_report = entry;
        let summary = self.samples.take_summary();
        let report = json!({
            "schema": 1,
            "window": format!("{:?}", window.window_handle()),
            "surface": "bar",
            "metric": "bar_render_entry",
            "mode": if self.probe_seconds.is_some() { "bounded_redraw_probe" } else { "passive" },
            "probe_seconds": self.probe_seconds,
            "probe_finished": self.probe_finished,
            "elapsed_seconds": since_start.as_secs_f64(),
            "sample_entries": summary.entries,
            "sample_intervals": summary.intervals,
            "dropped_samples": summary.dropped,
            "invalid_intervals": summary.invalid_intervals,
            "render_entry_rate_hz": summary.render_entry_rate_hz,
            "render_entry_interval_ms": distribution_json(summary.entry_interval),
            "view_build_wall_ms": distribution_json(summary.view_build),
            "physical_presentation_fps": null,
            "hardware_acceleration_verified": false,
            "includes_idle_gaps": true,
        });
        tracing::info!("GPUI_SHELL_FRAME_TELEMETRY {report}");
    }
}

fn distribution_json(value: Option<Distribution>) -> Value {
    value.map_or(Value::Null, |value| {
        json!({
            "p50": value.p50_ms, "p95": value.p95_ms,
            "p99": value.p99_ms, "max": value.max_ms,
        })
    })
}

/// Reports the device GPUI actually chose for this window, not a separate probe.
/// The locked GpuSpecs has no backend or PCI identity: never guess those fields.
pub fn report_renderer(window: &Window) {
    let specs = window.gpu_specs();
    let classification = specs
        .as_ref()
        .map(|specs| {
            renderer_class(
                specs.is_software_emulated,
                &specs.device_name,
                &specs.driver_name,
            )
        })
        .unwrap_or("unknown");
    let report = json!({
        "schema": 1,
        "window": format!("{:?}", window.window_handle()),
        "api": "wgpu",
        "backend": null,
        "backend_source": "upstream Selected GPU adapter log (GpuSpecs does not expose backend)",
        "device": specs,
        "classification": classification,
        "hardware_acceleration_verified": false,
        "physical_presentation_timestamps_available": false,
        "presentation_feedback_status": "unavailable_locked_gpui",
        "presentation_feedback_reason": "no original-queue per-buffer-commit subscription; raw surface handles are insufficient",
        "host_monitor_scanout_verified": false,
    });
    tracing::info!("GPUI_SHELL_RENDERER {report}");
    if classification != "unverified_non_cpu" {
        tracing::warn!(
            "GPUI renderer is {classification}; this session is not physical-GPU performance evidence"
        );
    }
}
