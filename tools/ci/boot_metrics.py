"""Best-effort, atomic timing artifacts for ISO VM phases."""
import json
import os
from pathlib import Path
import tempfile
import time
from contextlib import contextmanager


def _write_metrics(artifacts_dir, phase, acceleration, status, elapsed, error):
    result = {
        "phase": phase,
        "status": status,
        "acceleration": acceleration,
        "elapsed_seconds": max(0.0, elapsed),
    }
    if error is not None:
        result["error"] = error
    target = Path(artifacts_dir) / f"boot-metrics-{phase}.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=f".{target.name}.", dir=target.parent)
    try:
        # The VM may run in a root container on a host-mounted artifacts dir.
        # Keep the atomically published diagnostic readable by the host runner.
        os.fchmod(fd, 0o644)
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(result, stream, sort_keys=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(tmp, target)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


@contextmanager
def timed_phase(artifacts_dir, phase, acceleration):
    """Record duration and outcome, without masking an exception from the phase."""
    started = time.monotonic()
    status, error = "success", None
    try:
        yield
    except BaseException as exc:
        status, error = "failure", f"{type(exc).__name__}: {exc}"
        raise
    finally:
        try:
            _write_metrics(artifacts_dir, phase, acceleration, status,
                           time.monotonic() - started, error)
        except Exception as write_error:
            # Metrics are diagnostic; a write failure never replaces VM outcome.
            print(f"WARNING: could not write boot metrics for {phase}: {write_error}")
