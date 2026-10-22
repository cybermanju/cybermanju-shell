import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools/ci/prepare_kernel_vdso.sh"


class KernelVdsoTests(unittest.TestCase):
    def run_helper(self, out, curl=None):
        env = dict(os.environ)
        if curl is not None:
            bindir = out.parent / "bin"
            bindir.mkdir(exist_ok=True)
            command = bindir / "curl"
            command.write_text("#!/bin/bash\n" + curl + "\n")
            command.chmod(0o755)
            env["PATH"] = str(bindir) + os.pathsep + env["PATH"]
        return subprocess.run(["bash", str(SCRIPT), str(out)], env=env, capture_output=True, text=True)

    def test_source_matches_official_pin_and_requires_https_hash_and_elf(self):
        source = SCRIPT.read_text()
        docker = (ROOT / "asterinas/tools/dev_env/docker/kernel-dev/Dockerfile").read_text()
        self.assertIn("git checkout 8af5717", docker)
        self.assertIn("8af571775a3d3a1df3e84fc9709b9d6b160c771a", source)
        self.assertIn("27cd7bf576a92fa48a891c05988f52fac35b7374d5b9650d6b035e477c5d93a3", source)
        self.assertIn("sha256sum --check --status", source)
        self.assertIn("--proto '=https' --proto-redir '=https'", source)
        self.assertIn("== 62", source)
        self.assertIn("== 3", source)
        self.assertNotIn("--insecure", source)

    def test_existing_bad_file_is_preserved_without_download(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "vdso"
            out.mkdir()
            target = out / "vdso_x86_64.so"
            target.write_bytes(b"my-existing-file")
            result = self.run_helper(out, "exit 99")
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertEqual(target.read_bytes(), b"my-existing-file")
            self.assertFalse(list(out.glob(".vdso.*")))

    def test_download_failure_and_hash_mismatch_leave_no_published_file(self):
        cases = (
            "exit 19",
            'while (( $# )); do if [[ $1 == -o ]]; then printf fake > "$2"; exit 0; fi; shift; done; exit 91',
        )
        for curl in cases:
            with self.subTest(curl=curl), tempfile.TemporaryDirectory() as directory:
                out = Path(directory) / "vdso"
                result = self.run_helper(out, curl)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((out / "vdso_x86_64.so").exists())
                self.assertFalse(list(out.glob(".vdso.*")))

    def test_symlinked_directory_and_file_are_refused(self):
        for link_directory in (True, False):
            with self.subTest(directory=link_directory), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                target = base / "untouched"
                target.mkdir()
                (target / "data").write_bytes(b"untouched")
                out = base / "vdso"
                if link_directory:
                    out.symlink_to(target, target_is_directory=True)
                else:
                    out.mkdir()
                    (out / "vdso_x86_64.so").symlink_to(target / "data")
                result = self.run_helper(out, "exit 99")
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual((target / "data").read_bytes(), b"untouched")


if __name__ == "__main__":
    unittest.main()
