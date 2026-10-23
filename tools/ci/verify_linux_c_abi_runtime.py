#!/usr/bin/env python3
"""Verify the explicit Linux C ABI memcmp/memcpy probes evidence only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_LINUX_C_ABI:"
MARKERS = [
    PREFIX + "begin:PASS cpus=1 exports=1",
    PREFIX + "memcmp:PASS cases=16 c_calls=16 rust_calls=16 buffers_preserved=true",
    PREFIX + "memcpy:PASS cases=16 c_calls=16 rust_calls=16 source_preserved=true guards_preserved=true returns_destination=true destination_verified=true elf_admitted=false",
    PREFIX + "policy:PASS admitted=1 rust_denied=1 unknown_denied=3 signatures=1",
    PREFIX + "complete:PASS c_to_rust=true memcpy_elf_admitted=false elf_imports_verified=false modules_executed=0 production_mapping=false wx=false smp_mapping=false",
]
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(
    r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|"
    r"test result:\s*FAILED|\b[1-9]\d* failed;|"
    r"\bmutation(?:s|_flag)?\s*[:=]\s*true\b",
    re.I,
)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure or mutation")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if markers != MARKERS:
        raise ValueError("Expected exactly five unique ordered Linux C ABI markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if (len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0])
            or lines.index(summaries[0]) <= lines.index(MARKERS[-1])):
        raise ValueError("Expected one standalone successful native summary after evidence")
    return {
        "status": "passed",
        "scope": "explicit_memcmp_memcpy_c_abi_probes_only",
        "runtime_tests": 1,
        "failed": 0,
        "cpus": 1,
        "c_cases": 32,
        "c_calls": 32,
        "rust_calls": 32,
        "memcmp_buffers_preserved": True,
        "memcmp_cases": 16,
        "memcpy_cases": 16,
        "memcpy_source_preserved": True,
        "memcpy_guards_preserved": True,
        "memcpy_returns_destination": True,
        "memcpy_destination_verified": True,
        "memcpy_elf_admitted": False,
        "c_primitives_tested": 2,
        "explicit_c_shim_names": 1,
        "rust_internal_exports_denied": 1,
        "unknown_names_denied": 3,
        "c_abi_subset_verified": True,
        "elf_imports_verified": False,
        "linux_abi_verified": False,
        "production_linux_import_admission": False,
        "production_mapping_available": False,
        "module_execution_verified": False,
        "linux_modules_executed": False,
        "init_called": False,
        "wx_verified": False,
        "smp_mapping_verified": False,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","linux_abi_verified":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified explicit Linux C ABI memcmp/memcpy probes only.")


if __name__ == "__main__":
    main()
