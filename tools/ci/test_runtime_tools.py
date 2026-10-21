import os
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).with_name("install_runtime_tools.sh")


class RuntimeToolsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        (self.root / "etc").mkdir()
        (self.root / "usr/share/keyrings").mkdir(parents=True)
        (self.root / "etc/os-release").write_text('ID="ubuntu"\nVERSION_CODENAME=noble\n')
        (self.root / "usr/share/keyrings/ubuntu-archive-keyring.gpg").write_bytes(b"fixture")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "apt.log"
        self._stub("dpkg", '#!/bin/sh\necho "${TEST_ARCH:-amd64}"\n')
        self._stub("apt-get", '#!/bin/sh\nprintf "%s\\n" "$*" >> "$APT_LOG"\nfor arg in "$@"; do case "$arg" in Dir::Etc::sourcelist=*) cat "${arg#*=}" > "$APT_SOURCES";; esac; done\ncase " ${*} " in *" update "*) exit "${UPDATE_STATUS:-0}";; *" install "*) exit "${INSTALL_STATUS:-0}";; esac\n')

    def _stub(self, name, body):
        path = self.bin / name
        path.write_text(body)
        path.chmod(0o755)

    def run_script(self, profile, **extra):
        env = os.environ.copy()
        env.update(PATH=f"{self.bin}:{env['PATH']}", CI_RUNTIME_TOOLS_ROOT=str(self.root), APT_LOG=str(self.log), APT_SOURCES=str(self.root / "captured.sources"))
        env.update(extra)
        return subprocess.run(["bash", str(SCRIPT), *profile], env=env, text=True, capture_output=True)

    def test_profiles_sources_and_packages(self):
        expected = {
            "framebuffer": "qemu-system-x86 grub-common grub-pc-bin xorriso mtools",
            "module-staging": "qemu-system-x86 grub-common grub-pc-bin xorriso mtools linux-headers-6.8.0-147-generic=6.8.0-147.147 gcc-13 kmod",
            "kbuild": "linux-headers-6.8.0-147-generic=6.8.0-147.147 gcc-13 kmod",
        }
        for profile, packages in expected.items():
            result = self.run_script([profile])
            self.assertEqual(result.returncode, 0, result.stderr)
            lines = self.log.read_text().splitlines()
            self.assertEqual(len(lines), 2)
            self.assertIn("Dir::Etc::sourcelist=", lines[0])
            self.assertIn("Dir::Etc::sourceparts=-", lines[0])
            self.assertIn("Dir::State::lists=", lines[0])
            self.assertIn("APT::Update::Error-Mode=any", lines[0])
            self.assertIn(packages, lines[1])
            self.assertEqual(lines[0].split(" -o APT::Update::Error-Mode=any update")[0], lines[1].split(" install ")[0])
            self.assertIn("install -y --no-install-recommends", lines[1])
            for forbidden in ("--allow-unauthenticated", "AllowInsecure", "Verify-Peer=false", "trusted=yes"):
                self.assertNotIn(forbidden, self.log.read_text())
            source = (self.root / "captured.sources").read_text()
            self.assertIn("URIs: https://archive.ubuntu.com/ubuntu\nSuites: noble noble-updates noble-backports\nComponents: main universe\nSigned-By: /", source)
            self.assertIn("URIs: https://security.ubuntu.com/ubuntu\nSuites: noble-security\nComponents: main universe\nSigned-By: /", source)
            self.assertNotIn("packages.microsoft.com", source)
            self.assertFalse((self.root / "etc/apt/sources.list").exists())
            temp_path = lines[0].split("Dir::Etc::sourcelist=", 1)[1].split()[0]
            self.assertFalse(pathlib.Path(temp_path).exists())
            self.log.write_text("")

    def test_simulation_is_only_install(self):
        result = self.run_script(["kbuild"], CI_RUNTIME_TOOLS_SIMULATE="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.log.read_text().splitlines()
        self.assertEqual(len(calls), 2)
        self.assertIn("--simulate install", calls[1])

    def test_rejects_bad_inputs_and_fixtures(self):
        for profile in ([], ["unknown"], ["framebuffer", "extra"]):
            self.assertNotEqual(self.run_script(profile).returncode, 0)
        release = self.root / "etc/os-release"
        release.write_text('ID=debian\nVERSION_CODENAME=noble\n')
        self.assertNotEqual(self.run_script(["framebuffer"]).returncode, 0)
        release.write_text('ID=ubuntu\nVERSION_CODENAME=jammy\n')
        self.assertNotEqual(self.run_script(["framebuffer"]).returncode, 0)
        release.write_text('ID=ubuntu\nVERSION_CODENAME=noble\n')
        self.assertNotEqual(self.run_script(["framebuffer"], TEST_ARCH="arm64").returncode, 0)
        (self.root / "usr/share/keyrings/ubuntu-archive-keyring.gpg").unlink()
        self.assertNotEqual(self.run_script(["framebuffer"]).returncode, 0)
        self.assertEqual(self.log.read_text() if self.log.exists() else "", "")

    def test_failures_propagate_and_install_requires_update(self):
        result = self.run_script(["framebuffer"], UPDATE_STATUS="19")
        self.assertEqual(result.returncode, 19)
        self.assertEqual(len(self.log.read_text().splitlines()), 1)
        self.assert_temp_removed()
        self.log.write_text("")
        result = self.run_script(["framebuffer"], INSTALL_STATUS="23")
        self.assertEqual(result.returncode, 23)
        self.assertEqual(len(self.log.read_text().splitlines()), 2)
        self.assert_temp_removed()

    def assert_temp_removed(self):
        line = self.log.read_text().splitlines()[0]
        source = pathlib.Path(line.split("Dir::Etc::sourcelist=", 1)[1].split()[0])
        self.assertFalse(source.parent.exists())

    def test_existing_third_party_source_is_not_changed_or_selected(self):
        source = self.root / "etc/apt/sources.list.d/microsoft.list"
        source.parent.mkdir(parents=True)
        original = "deb https://packages.microsoft.com/ubuntu/24.04/prod noble main\n"
        source.write_text(original)
        result = self.run_script(["framebuffer"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(source.read_text(), original)
        self.assertNotIn("packages.microsoft.com", (self.root / "captured.sources").read_text())
        self.assertNotIn("--simulate", self.log.read_text())
        self.assertIn("Dir::Cache::archives=", self.log.read_text())
        self.assert_temp_removed()

    def test_rejects_invalid_simulation_and_empty_keyring_before_apt(self):
        self.assertEqual(self.run_script(["framebuffer"], CI_RUNTIME_TOOLS_SIMULATE="yes").returncode, 2)
        (self.root / "usr/share/keyrings/ubuntu-archive-keyring.gpg").write_bytes(b"")
        self.assertNotEqual(self.run_script(["framebuffer"]).returncode, 0)
        self.assertFalse(self.log.exists())

    def test_os_release_is_data_not_shell(self):
        marker = self.root / "must-not-exist"
        (self.root / "etc/os-release").write_text(
            f'ID=ubuntu\nVERSION_CODENAME="noble"\nUNTRUSTED=$(touch {marker})\n'
        )
        result = self.run_script(["kbuild"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
