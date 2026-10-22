import os
import pathlib
import shlex
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
        self.sleep_log = self.root / "sleep.log"
        self.update_audit = self.root / "update-audit.log"
        self._stub("dpkg", '#!/bin/sh\necho "${TEST_ARCH:-amd64}"\n')
        apt_stub = r'''#!/bin/sh
printf '%s\n' "$*" >> "$APT_LOG"
listdir=
source=
for arg in "$@"; do
  case "$arg" in
    Dir::State::lists=*) listdir=${arg#*=} ;;
    Dir::Etc::sourcelist=*) source=${arg#*=}; cat "$source" > "$APT_SOURCES" ;;
  esac
done
case " $* " in
  *" update "*)
    count=0; [ ! -f "$APT_UPDATE_COUNT" ] || count=$(cat "$APT_UPDATE_COUNT")
    count=$((count + 1)); printf '%s' "$count" > "$APT_UPDATE_COUNT"
    [ -z "$listdir" ] || { [ -d "$listdir" ] && printf '%s %s\n' "$listdir" "$(find "$listdir" -type f | wc -l)" >> "$UPDATE_AUDIT"; }
    [ "${FAKE_APT_WRITE_MARKER:-0}" != 1 ] || { mkdir -p "$listdir"; : > "$listdir/previous-attempt-marker"; }
    failures=${UPDATE_FAILURES:-0}
    if [ "${PARTIAL_UPDATE_ERROR:-0}" = 1 ]; then
      case " $* " in *" APT::Update::Error-Mode=any "*) exit 100 ;; esac
      exit 0
    fi
    if [ "$count" -le "$failures" ]; then echo "Hash Sum mismatch / mirror failure" >&2; exit "${UPDATE_FAILURE_STATUS:-100}"; fi
    exit "${UPDATE_STATUS:-0}" ;;
  *" install "*) exit "${INSTALL_STATUS:-0}" ;;
esac
exit 90
'''
        self._stub("apt-get", apt_stub)
        self._stub("sleep", '#!/bin/sh\nprintf \'%s\\n\' "$*" >> "$SLEEP_LOG"\n')

    def _stub(self, name, body):
        path = self.bin / name
        path.write_text(body)
        path.chmod(0o755)

    def run_script(self, profile, **extra):
        env = os.environ.copy()
        env.update(
            PATH=f"{self.bin}:{env['PATH']}", CI_RUNTIME_TOOLS_ROOT=str(self.root),
            APT_LOG=str(self.log), APT_SOURCES=str(self.root / "captured.sources"),
            APT_UPDATE_COUNT=str(self.root / "apt-update-count"), SLEEP_LOG=str(self.sleep_log),
            UPDATE_AUDIT=str(self.update_audit),
        )
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
            for path in (self.root / "apt-update-count", self.sleep_log, self.update_audit):
                path.unlink(missing_ok=True)

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
        result = self.run_script(["framebuffer"], UPDATE_FAILURES="3", UPDATE_FAILURE_STATUS="19")
        self.assertEqual(result.returncode, 19)
        self.assertEqual(len(self.log.read_text().splitlines()), 3)
        self.assert_temp_removed()
        self.log.write_text("")
        result = self.run_script(["framebuffer"], INSTALL_STATUS="23")
        self.assertEqual(result.returncode, 23)
        self.assertEqual(len(self.log.read_text().splitlines()), 2)
        self.assert_temp_removed()

    def test_update_recovers_after_one_and_two_failures(self):
        for failures in (1, 2):
            with self.subTest(failures=failures):
                self.log.write_text("")
                self.sleep_log.unlink(missing_ok=True)
                self.update_audit.unlink(missing_ok=True)
                (self.root / "apt-update-count").unlink(missing_ok=True)
                result = self.run_script(["framebuffer"], UPDATE_FAILURES=str(failures), FAKE_APT_WRITE_MARKER="1")
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = self.log.read_text().splitlines()
                self.assertEqual(len(calls), failures + 2)
                updates = calls[:-1]
                self.assertTrue(all("Error-Mode=any" in call for call in updates))
                self.assertEqual(self.sleep_log.read_text().splitlines(), ["5", "10"][:failures])
                self.assertIn("Hash Sum mismatch / mirror failure", result.stderr)

    def test_permanent_update_failure_exhausts_retries_and_returns_last_status(self):
        result = self.run_script(["framebuffer"], UPDATE_FAILURES="9", UPDATE_FAILURE_STATUS="37")
        self.assertEqual(result.returncode, 37)
        self.assertEqual(len(self.log.read_text().splitlines()), 3)
        self.assertEqual(self.sleep_log.read_text().splitlines(), ["5", "10"])
        self.assert_temp_removed()

    def test_each_update_gets_fresh_empty_lists_and_install_uses_last_success(self):
        result = self.run_script(["kbuild"], UPDATE_FAILURES="2", FAKE_APT_WRITE_MARKER="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.log.read_text().splitlines()
        self.assertEqual(len(calls), 4)
        updates = [shlex.split(call) for call in calls[:3]]
        install = shlex.split(calls[3])
        def listdir(args):
            return next(arg.split("=", 1)[1] for arg in args if arg.startswith("Dir::State::lists="))
        dirs = [listdir(args) for args in updates]
        self.assertEqual(len(set(dirs)), 3)
        self.assertEqual([pathlib.Path(entry.split()[0]) for entry in self.update_audit.read_text().splitlines()], list(map(pathlib.Path, dirs)))
        self.assertEqual([int(entry.split()[1]) for entry in self.update_audit.read_text().splitlines()], [0, 0, 0])
        self.assertEqual(listdir(install), dirs[-1])
        self.assertEqual(updates[-1][:-3], install[:install.index("install")])
        self.assertFalse((self.root / "etc/apt/sources.list").exists())

    def test_retry_simulation_preserves_install_options_and_official_signed_sources(self):
        result = self.run_script(["kbuild"], UPDATE_FAILURES="1", CI_RUNTIME_TOOLS_SIMULATE="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.log.read_text().splitlines()
        self.assertEqual(len(calls), 3)
        install = calls[-1]
        self.assertIn("--simulate install --no-install-recommends", install)
        self.assertIn("Dir::Etc::sourcelist=", install)
        self.assertIn("Dir::Etc::sourceparts=-", install)
        self.assertIn("Signed-By", (self.root / "captured.sources").read_text())
        self.assertNotIn("--allow-unauthenticated", install)
        self.assertNotIn("trusted=yes", install)

    def test_install_failure_is_not_retried(self):
        result = self.run_script(["kbuild"], INSTALL_STATUS="23")
        self.assertEqual(result.returncode, 23)
        self.assertEqual(len(self.log.read_text().splitlines()), 2)
        self.assertFalse(self.sleep_log.exists())

    def test_partial_update_failure_never_reaches_install(self):
        result = self.run_script(["framebuffer"], PARTIAL_UPDATE_ERROR="1")
        self.assertEqual(result.returncode, 100)
        calls = self.log.read_text().splitlines()
        self.assertEqual(len(calls), 3)
        self.assertTrue(all(" update" in call and " install " not in call for call in calls))
        self.assertEqual(self.sleep_log.read_text().splitlines(), ["5", "10"])
        self.assert_temp_removed()

    def test_update_error_mode_precedes_override_on_every_attempt(self):
        result = self.run_script(["framebuffer"], UPDATE_FAILURES="2")
        self.assertEqual(result.returncode, 0, result.stderr)
        for call in self.log.read_text().splitlines()[:3]:
            self.assertLess(call.index("APT::Update::Error-Mode=any"), call.index("update"))
            self.assertNotIn("AllowInsecure", call)

    def test_cleanup_removes_all_attempt_lists_and_preserves_host_source_cache(self):
        source = self.root / "etc/apt/sources.list.d/host.list"
        source.parent.mkdir(parents=True)
        source.write_text("deb https://third-party.invalid noble main\n")
        cache = self.root / "var/cache/apt/archives/sentinel.deb"
        cache.parent.mkdir(parents=True)
        cache.write_bytes(b"keep")
        host_lists = self.root / "var/lib/apt/lists/sentinel"
        host_lists.parent.mkdir(parents=True)
        host_lists.write_bytes(b"keep host lists")
        result = self.run_script(["framebuffer"], UPDATE_FAILURES="2")
        self.assertEqual(result.returncode, 0, result.stderr)
        dirs = [next(arg.split("=", 1)[1] for arg in shlex.split(call) if arg.startswith("Dir::State::lists=")) for call in self.log.read_text().splitlines()]
        self.assertEqual(len(set(dirs)), 3)
        self.assertTrue(all(not pathlib.Path(path).exists() for path in dirs))
        self.assertEqual(source.read_text(), "deb https://third-party.invalid noble main\n")
        self.assertEqual(cache.read_bytes(), b"keep")
        self.assertEqual(host_lists.read_bytes(), b"keep host lists")

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
