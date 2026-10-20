"""Unit tests for analyze_frame_telemetry.py (stdlib only)."""

from __future__ import annotations

import contextlib
import io
import json
import math
import tempfile
import unittest
from pathlib import Path

import analyze_frame_telemetry as analyzer


class FrameTelemetryAnalyzerTests(unittest.TestCase):
    device_name = 'GPU "quoted" \\ adapter'

    def renderer_record(self, classification: str = "unverified_non_cpu") -> dict:
        return {
            "schema": 1,
            "window": "WindowHandle(7)",
            "api": "wgpu",
            "backend": None,
            "backend_source": "Selected GPU adapter upstream log",
            "device": {
                "device_name": self.device_name,
                "driver_name": "test driver",
                "is_software_emulated": classification == "software_cpu",
            },
            "classification": classification,
            "hardware_acceleration_verified": False,
            "physical_presentation_timestamps_available": False,
        }

    def telemetry_record(self, **overrides: object) -> dict:
        record = {
            "schema": 1,
            "window": "WindowHandle(7)",
            "surface": "bar",
            "metric": "bar_render_entry",
            "mode": "passive",
            "probe_seconds": None,
            "probe_finished": False,
            "elapsed_seconds": 4.5,
            "sample_entries": 3,
            "sample_intervals": 2,
            "dropped_samples": 0,
            "invalid_intervals": 0,
            "render_entry_rate_hz": 20.0,
            "render_entry_interval_ms": {"p50": 50.0, "p95": 100.0, "p99": 100.0, "max": 100.0},
            "view_build_wall_ms": {"p50": 1.0, "p95": 2.0, "p99": 2.0, "max": 2.0},
            "physical_presentation_fps": None,
            "hardware_acceleration_verified": False,
            "includes_idle_gaps": True,
        }
        record.update(overrides)
        return record

    def log_text(
        self,
        *,
        classification: str = "unverified_non_cpu",
        telemetry: dict | None = None,
        renderer: dict | None = None,
        adapter_name: str | None = None,
        backend: str = "Vulkan",
    ) -> str:
        selected_name = self.device_name if adapter_name is None else adapter_name
        rows = [
            f"INFO GPUI_SHELL_RENDERER {json.dumps(renderer or self.renderer_record(classification))}",
            f"INFO Selected GPU adapter: {json.dumps(selected_name)} ({backend})",
            f"INFO GPUI_SHELL_FRAME_TELEMETRY {json.dumps(telemetry or self.telemetry_record())}",
        ]
        return "\n".join(rows) + "\n"

    def appended_log(self, *records: tuple[str, dict], adapters: tuple[tuple[str, str], ...] = ()) -> str:
        rows = [self.log_text().rstrip("\n")]
        for marker, record in records:
            rows.append(f"INFO {marker} {json.dumps(record)}")
        for name, backend in adapters:
            rows.append(f"INFO Selected GPU adapter: {json.dumps(name)} ({backend})")
        return "\n".join(rows) + "\n"

    def test_parses_escaped_device_name_and_matches_upstream_backend(self) -> None:
        result = analyzer.analyze_text(self.log_text(backend="Gl"))
        self.assertEqual(result["renderer"]["device_name"], self.device_name)
        self.assertEqual(result["renderer"]["upstream_selected_adapter"], {
            "device": self.device_name,
            "backend": "Gl",
        })
        self.assertTrue(result["renderer"]["backend_device_match"])
        self.assertEqual(result["renderer"]["specs"]["driver_name"], "test driver")

    def test_presentation_blocker_is_preserved_without_certifying_scanout(self) -> None:
        renderer = self.renderer_record()
        renderer.update(
            presentation_feedback_status="unavailable_locked_gpui",
            presentation_feedback_reason="no original-queue per-buffer-commit subscription",
            host_monitor_scanout_verified=False,
        )
        result = analyzer.analyze_text(self.log_text(renderer=renderer))
        self.assertEqual(result["renderer"]["presentation_feedback_status"], "unavailable_locked_gpui")
        self.assertEqual(result["renderer"]["presentation_feedback_reason"], renderer["presentation_feedback_reason"])
        self.assertFalse(result["renderer"]["host_monitor_scanout_verified"])
        self.assertIsNone(result["physical_presentation_fps"])

    def test_legacy_logs_do_not_invent_a_recorded_presentation_status(self) -> None:
        result = analyzer.analyze_text(self.log_text())
        self.assertEqual(result["renderer"]["presentation_feedback_status"], "not_recorded")
        self.assertIsNone(result["renderer"]["presentation_feedback_reason"])

    def test_partial_or_physical_presentation_claims_fail_closed(self) -> None:
        base = self.renderer_record()
        base.update(
            presentation_feedback_status="unavailable_locked_gpui",
            presentation_feedback_reason="no safe hook",
            host_monitor_scanout_verified=False,
        )
        for field, value in (
            ("presentation_feedback_status", "physical"),
            ("host_monitor_scanout_verified", True),
            ("presentation_feedback_reason", ""),
        ):
            with self.subTest(field=field), self.assertRaises(analyzer.AnalysisError):
                analyzer.analyze_text(self.log_text(renderer={**base, field: value}))
        with self.assertRaises(analyzer.AnalysisError):
            analyzer.analyze_text(self.log_text(renderer={**self.renderer_record(), "host_monitor_scanout_verified": False}))

    def test_malformed_json_after_each_required_marker_fails(self) -> None:
        base = self.log_text()
        for marker in (analyzer.RENDERER_MARKER, analyzer.TELEMETRY_MARKER):
            with self.subTest(marker=marker):
                corrupted = base.replace(marker + "{", marker + "{broken", 1)
                with self.assertRaisesRegex(analyzer.AnalysisError, "malformed JSON"):
                    analyzer.analyze_text(corrupted)

    def test_missing_renderer_telemetry_or_selected_adapter_fails(self) -> None:
        valid = self.log_text()
        cases = [
            (valid.replace("INFO GPUI_SHELL_RENDERER", "INFO OMITTED", 1), "missing GPUI_SHELL_RENDERER"),
            (valid.replace("INFO GPUI_SHELL_FRAME_TELEMETRY", "INFO OMITTED", 1), "missing GPUI_SHELL_FRAME_TELEMETRY"),
            (valid.replace("INFO Selected GPU adapter:", "INFO adapter:", 1), "missing upstream Selected GPU adapter"),
        ]
        for text, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(analyzer.AnalysisError, message):
                analyzer.analyze_text(text)

    def test_malformed_upstream_adapter_fails(self) -> None:
        text = self.log_text().replace(
            f'Selected GPU adapter: {json.dumps(self.device_name)} (Vulkan)',
            'Selected GPU adapter: "unterminated (Vulkan)',
        )
        with self.assertRaisesRegex(analyzer.AnalysisError, "malformed Selected GPU adapter"):
            analyzer.analyze_text(text)

    def test_device_backend_must_match_renderer_specs(self) -> None:
        with self.assertRaisesRegex(analyzer.AnalysisError, "exactly matches renderer device_name"):
            analyzer.analyze_text(self.log_text(adapter_name="different adapter"))

    def test_every_telemetry_window_requires_its_own_renderer_record(self) -> None:
        second_window = self.telemetry_record(window="WindowHandle(8)")
        text = self.appended_log((analyzer.TELEMETRY_MARKER, second_window))
        with self.assertRaisesRegex(analyzer.AnalysisError, "telemetry window.*no matching.*RENDERER"):
            analyzer.analyze_text(text)

    def test_each_renderer_device_requires_a_matching_adapter(self) -> None:
        second_renderer = self.renderer_record()
        second_renderer.update(window="WindowHandle(8)", device={"device_name": "other GPU"})
        second_telemetry = self.telemetry_record(window="WindowHandle(8)")
        text = self.appended_log(
            (analyzer.RENDERER_MARKER, second_renderer),
            (analyzer.TELEMETRY_MARKER, second_telemetry),
        )
        with self.assertRaisesRegex(analyzer.AnalysisError, "no upstream.*matches renderer device_name"):
            analyzer.analyze_text(text)

    def test_same_adapter_name_on_two_backends_is_rejected_as_ambiguous(self) -> None:
        text = self.appended_log(adapters=((self.device_name, "Gl"),))
        with self.assertRaisesRegex(analyzer.AnalysisError, "ambiguous upstream adapter"):
            analyzer.analyze_text(text)

    def test_mixed_window_identities_are_reported_per_window_without_singular_summary(self) -> None:
        renderer = self.renderer_record("software_cpu")
        renderer.update(window="WindowHandle(8)", device={"device_name": "second GPU"})
        telemetry = self.telemetry_record(window="WindowHandle(8)")
        text = self.appended_log(
            (analyzer.RENDERER_MARKER, renderer),
            (analyzer.TELEMETRY_MARKER, telemetry),
            adapters=(("second GPU", "Gl"),),
        )
        result = analyzer.analyze_text(text)
        self.assertNotIn("renderer", result)
        self.assertEqual([item["window"] for item in result["renderer_by_window"]], [
            "WindowHandle(7)", "WindowHandle(8)"
        ])
        self.assertEqual(result["physical_gpu_certification"]["classification"], "mixed")

    def test_renderer_metadata_and_telemetry_mode_fields_are_validated(self) -> None:
        invalid_renderer = self.renderer_record()
        invalid_renderer["window"] = ""
        with self.assertRaisesRegex(analyzer.AnalysisError, "missing window identity"):
            analyzer.analyze_text(self.log_text(renderer=invalid_renderer))
        for overrides, expected in (
            ({"mode": "invalid"}, "mode must be"),
            ({"probe_seconds": 2}, "passive mode requires"),
            ({"includes_idle_gaps": False}, "includes_idle_gaps must be true"),
        ):
            with self.subTest(overrides=overrides), self.assertRaisesRegex(analyzer.AnalysisError, expected):
                analyzer.analyze_text(self.log_text(telemetry=self.telemetry_record(**overrides)))

    def test_non_strict_software_observation_is_not_hardware_evidence(self) -> None:
        result = analyzer.analyze_text(self.log_text(classification="software_cpu"))
        self.assertTrue(result["not_hardware_performance_evidence"])
        self.assertFalse(result["hardware_acceleration_verified"])
        self.assertIsNone(result["physical_presentation_fps"])
        self.assertIn("not physical-GPU evidence", result["physical_gpu_certification"]["reason"])
        self.assertEqual(result["telemetry"]["observations"][0]["render_entry_rate_hz"], 20.0)
        self.assertIn("not presentation FPS", " ".join(result["limitations"]))

    def test_strict_physical_gpu_requirement_fails_for_every_classification(self) -> None:
        for classification in ("software_cpu", "virtual_or_remote", "unknown", "unverified_non_cpu"):
            with self.subTest(classification=classification):
                # Unknown classifications can still have a named adapter; the
                # checker must not mistake matching a name for GPU certification.
                with tempfile.TemporaryDirectory() as temporary:
                    log_path = Path(temporary) / "gpui.log"
                    log_path.write_text(self.log_text(classification=classification), encoding="utf-8")
                    stdout, stderr = io.StringIO(), io.StringIO()
                    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                        status = analyzer.main([str(log_path), "--require-physical-gpu"])
                    self.assertEqual(status, 3)
                    summary = json.loads(stdout.getvalue())
                    self.assertFalse(summary["hardware_acceleration_verified"])
                    self.assertFalse(summary["physical_gpu_certification"]["strict_requirement_met"])
                    self.assertIn("no override", summary["physical_gpu_certification"]["strict_requirement_reason"])
                    self.assertIn("not verified", stderr.getvalue())

    def test_empty_sample_count_fails(self) -> None:
        record = self.telemetry_record(
            sample_entries=0,
            sample_intervals=0,
            render_entry_rate_hz=None,
            render_entry_interval_ms=None,
            view_build_wall_ms=None,
        )
        with self.assertRaisesRegex(analyzer.AnalysisError, "no samples"):
            analyzer.analyze_text(self.log_text(telemetry=record))

    def test_invalid_and_nonfinite_timing_values_fail(self) -> None:
        cases = [
            ({"elapsed_seconds": -0.1}, "elapsed_seconds"),
            ({"elapsed_seconds": math.nan}, "finite"),
            ({"render_entry_rate_hz": math.inf}, "finite"),
            ({"render_entry_interval_ms": {"p50": 0, "p95": 0, "p99": math.inf, "max": math.inf}}, "finite"),
            ({"view_build_wall_ms": {"p50": 0, "p95": 2, "p99": 1, "max": 2}}, "non-decreasing"),
        ]
        for overrides, expected in cases:
            with self.subTest(overrides=overrides), self.assertRaisesRegex(analyzer.AnalysisError, expected):
                analyzer.analyze_text(self.log_text(telemetry=self.telemetry_record(**overrides)))

    def test_one_sample_allows_null_rate_but_requires_build_timing(self) -> None:
        telemetry = self.telemetry_record(
            sample_entries=1,
            sample_intervals=0,
            render_entry_rate_hz=None,
            render_entry_interval_ms=None,
        )
        result = analyzer.analyze_text(self.log_text(telemetry=telemetry))
        self.assertIsNone(result["telemetry"]["observations"][0]["render_entry_rate_hz"])
        with self.assertRaisesRegex(analyzer.AnalysisError, "view-build percentiles are required"):
            analyzer.analyze_text(self.log_text(telemetry={**telemetry, "view_build_wall_ms": None}))


if __name__ == "__main__":
    unittest.main()
