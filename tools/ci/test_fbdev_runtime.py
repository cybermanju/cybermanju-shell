import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from verify_fbdev_runtime import FILEOPS, SCOPE, SURFACE, verify

SUMMARY = "test result: ok. 1 passed; 0 failed; 0 filtered out."
LOG = "\n".join((SURFACE, FILEOPS, SCOPE, SUMMARY))
RUNNER = Path(__file__).with_name("test_framebuffer_runtime.sh").read_text()


class FbdevRuntimeTests(unittest.TestCase):
    def test_full_json_claims_are_narrow_and_exact(self):
        expected = {
            "status": "passed", "scope": "fbdev_direct_fileops_only",
            "direct_fileops_verified": True, "userspace_syscall_verified": False,
            "mmap_tail_isolated": False, "fbdev_init_called": False,
            "linux_modules_executed": False,
            "surface": {"bytes": 24, "pitch": 12, "smem_len": 24, "mapping_larger": True},
            "read_cases": 4, "write_cases": 4, "guard_bytes": 8,
        }
        self.assertEqual(verify(LOG), expected)
        with tempfile.TemporaryDirectory() as td:
            p = Path(td) / "runtime.log"
            out = Path(td) / "result.json"
            p.write_bytes(LOG.replace("\n", "\r\n").encode())
            subprocess.run(["python3", str(Path(__file__).with_name("verify_fbdev_runtime.py")), str(p), "fbdev", str(out)], check=True, capture_output=True)
            self.assertEqual(json.loads(out.read_text()), expected)

    def test_mixed_extra_corrupt_duplicate_failure_and_cr_rejected(self):
        invalid = [
            LOG + "\n" + SURFACE,
            LOG.replace(SURFACE, SURFACE + "\n" + "CYBERMANJU_FBDEV:extra:PASS"),
            LOG.replace("bytes=24", "bytes=25"),
            LOG.replace(SURFACE, "$ echo " + SURFACE),
            LOG.replace(SCOPE, "prefix " + SCOPE),
            LOG + "\ntest result: ok. 1 passed; 0 failed;",
            LOG.replace(SUMMARY, "test result: ok. 0 passed; 0 failed;"),
            LOG + "\nthread panicked at runtime.rs",
            LOG.replace("\n", "\r"),
            LOG + "\nCYBERMANJU_FBDEV:unknown:PASS",
        ]
        for text in invalid:
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify(text)

    def test_fbdev_boot_enables_early_console_with_the_osdk_schema(self):
        import tomllib
        config = Path(__file__).with_name("fbdev-runtime.OSDK.toml")
        boot = tomllib.loads(config.read_text())["boot"]
        self.assertEqual(boot["kcmd_args"], ["earlycon"])
        self.assertNotIn("kcmdline", boot)
        self.assertIn('CFG="$ROOT/tools/ci/fbdev-runtime.OSDK.toml"', RUNNER)
        self.assertIn("multiboot2 /boot/aster-core-osdk-bin earlycon --$", RUNNER)

    def test_runner_preserves_config_before_vdso_preflight_and_rejects_before_build(self):
        preserve = RUNNER.index("trap 'if cmp -s")
        vdso = RUNNER.index('[[ -n "${VDSO_LIBRARY_DIR:-}"')
        build = RUNNER.index('"$OSDK" osdk test')
        self.assertLess(preserve, vdso)
        self.assertLess(vdso, build)
        self.assertLess(RUNNER.index('bash "$ROOT/tools/ci/prepare_kernel_vdso.sh" "$VDSO_LIBRARY_DIR"'), build)
        self.assertIn('[[ -n "${VDSO_LIBRARY_DIR:-}" && -s "$VDSO_LIBRARY_DIR/vdso_x86_64.so" ]]', RUNNER)
        self.assertIn('fbdev) CRATE=aster-core; PATHCRATE=kernel/core; FILTER=cybermanju_fbdev_fileops_runtime;', RUNNER)
        self.assertIn('VERIFY="$ROOT/tools/ci/verify_fbdev_runtime.py"', RUNNER)
        self.assertIn('sha256sum "$CFG" "$ROOT/tools/ci/test_framebuffer_runtime.sh" "$VERIFY"', RUNNER)
        self.assertIn('sha256sum "$ROOT/tools/ci/prepare_kernel_vdso.sh" "$VDSO_LIBRARY_DIR/vdso_x86_64.so"', RUNNER)


if __name__ == "__main__":
    unittest.main()
