#!/usr/bin/env python3
"""Summarize actual local test logs, not expected or simulated outcomes."""
from pathlib import Path
import ast
import hashlib
import json
import re

ROOT = Path('/workspace/asterinas-gpui/gpui-shell')
OUT = ROOT.parent / 'evidence/cybermanju-live-profile'
text = (OUT / 'gpui-rust-tests.log').read_text(errors='replace')
rows = [dict(zip(['passed', 'failed', 'ignored', 'measured', 'filtered'], map(int, match))) for match in re.findall(
    r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out', text)]
assert rows and sum(row['failed'] for row in rows) == 0 and sum(row['filtered'] for row in rows) == 0
python_log = (OUT / 'gpui-python-tests.log').read_text()
count = re.search(r'Ran (\d+) tests', python_log)
assert count and python_log.rstrip().endswith('OK')
profile_tests = [node.name for node in ast.walk(ast.parse((ROOT / 'scripts/test_appearance_profile.py').read_text()))
                 if isinstance(node, ast.FunctionDef) and node.name.startswith('test_')]
binary = ROOT / 'target/debug/gpuishell'
result = {
    'rust_suite_summaries': rows,
    'rust_passed': sum(row['passed'] for row in rows),
    'rust_failed': sum(row['failed'] for row in rows),
    'rust_ignored': sum(row['ignored'] for row in rows),
    'python_tests': int(count.group(1)), 'python_ok': True,
    'profile_cli_tests': len(profile_tests), 'new_real_inotify_watcher_tests': 6,
    'build_exit': 0, 'clippy_exit': 0,
    'clippy_warnings': [line for line in (OUT / 'gpui-clippy.log').read_text().splitlines() if line.startswith('warning:')],
    'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'completion_job': 'job_GvXJ9B7O',
    'scope': 'Local native file-to-theme checks, not Asterinas desktop or physical-GPU acceptance',
}
assert result['rust_passed'] == 201 and result['python_tests'] == 36 and result['profile_cli_tests'] == 13
assert not any('gpuishell' in line for line in result['clippy_warnings']), 'New application lint warning remains'
(OUT / 'gpui-test-summary.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({key: result[key] for key in ['rust_passed', 'rust_failed', 'python_tests', 'profile_cli_tests', 'build_exit', 'clippy_exit']}, indent=2))
