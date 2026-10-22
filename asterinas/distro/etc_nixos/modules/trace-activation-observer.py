#!/usr/bin/env python3
"""Safely instrument exactly one setup-etc command in a temporary activation script."""

from __future__ import annotations

import os
import re
import shlex
import stat
import sys
from pathlib import Path

_OBSERVER_NAME = "__cybermanju_activation_observe"
_SETUP_MARKER = "-setup-etc.pl"
_PATH_TOKEN = re.compile(r"^[A-Za-z0-9_./+% -]+$")


class UnsupportedSetupEtc(ValueError):
    """The activation source does not have the supported command form."""


def _validate_file(path: str, description: str, *, readable: bool = False,
                   executable: bool = False, reject_symlink: bool = False) -> None:
    if not Path(path).is_absolute():
        raise UnsupportedSetupEtc(f"{description} must be an absolute path")
    try:
        info = os.stat(path, follow_symlinks=not reject_symlink)
        if reject_symlink and os.path.islink(path):
            raise UnsupportedSetupEtc(f"{description} must not be a symlink")
        if not stat.S_ISREG(info.st_mode):
            raise UnsupportedSetupEtc(f"{description} must be a regular file")
        if readable and not os.access(path, os.R_OK):
            raise UnsupportedSetupEtc(f"{description} is not readable")
        if executable and not os.access(path, os.X_OK):
            raise UnsupportedSetupEtc(f"{description} is not executable")
    except OSError as exc:
        raise UnsupportedSetupEtc(f"cannot validate {description}: {exc}") from exc


def _validate_literal_path(token: str) -> bool:
    return token.startswith("/") and _PATH_TOKEN.fullmatch(token) is not None


def instrument(path: str, observer: str, shell: str, sleep: str) -> None:
    """Validate inputs and rewrite the temporary activation source in place."""
    _validate_file(path, "temporary activation script", reject_symlink=True)
    _validate_file(observer, "observer", readable=True)
    _validate_file(shell, "shell", executable=True)
    _validate_file(sleep, "sleep executable", executable=True)

    try:
        with open(path, "r", encoding="utf-8", errors="strict", newline="") as stream:
            source = stream.read()
    except (OSError, UnicodeError, ValueError) as exc:
        raise UnsupportedSetupEtc(f"cannot read activation source as UTF-8: {exc}") from exc

    if _OBSERVER_NAME in source:
        raise UnsupportedSetupEtc(f"source already contains {_OBSERVER_NAME}")

    lines = source.splitlines(keepends=True)
    candidates: list[int] = []
    for index, line in enumerate(lines):
        body = line.rstrip("\r\n")
        if not body.strip() or body.lstrip().startswith("#"):
            continue
        if _SETUP_MARKER not in body:
            continue
        try:
            tokens = shlex.split(body, comments=True, posix=True)
        except ValueError as exc:
            raise UnsupportedSetupEtc(f"malformed command quoting on line {index + 1}: {exc}") from exc
        if not tokens or _SETUP_MARKER not in " ".join(tokens):
            continue
        if (len(tokens) != 3 or not _validate_literal_path(tokens[0])
                or not tokens[0].endswith("/bin/perl")
                or not _validate_literal_path(tokens[1])
                or not re.search(r"/[^/]+-setup-etc\.pl$", tokens[1])
                or not _validate_literal_path(tokens[2])
                or not re.search(r"/[^/]+-etc/etc$", tokens[2])):
            raise UnsupportedSetupEtc(f"unsupported setup-etc command on line {index + 1}")
        candidates.append(index)

    if len(candidates) != 1:
        raise UnsupportedSetupEtc(f"expected exactly one setup-etc command; found {len(candidates)}")

    command_index = candidates[0]
    original = lines[command_index]
    indentation = original[:len(original) - len(original.lstrip())]
    newline = "\r\n" if original.endswith("\r\n") else "\n" if original.endswith("\n") else ""
    command = original[len(indentation):len(original) - len(newline) if newline else len(original)]
    lines[command_index] = indentation + _OBSERVER_NAME + " " + command + newline

    shebang_end = 1 if lines and lines[0].startswith("#!") else 0
    function = (
        f"{_OBSERVER_NAME}() {{\n"
        f"  {shlex.quote(shell)} {shlex.quote(observer)} {shlex.quote(sleep)} -- \"$@\"\n"
        "}\n"
    )
    lines.insert(shebang_end, function)
    output = "".join(lines)
    try:
        with open(path, "w", encoding="utf-8", errors="strict", newline="") as stream:
            stream.write(output)
    except (OSError, UnicodeError, ValueError) as exc:
        raise UnsupportedSetupEtc(f"cannot write temporary activation script: {exc}") from exc


def main(argv: list[str]) -> int:
    if len(argv) != 5:
        print(f"Usage: {argv[0]} TEMP_ACTIVATE OBSERVER_ABSPATH SHELL_ABSPATH SLEEP_ABSPATH", file=sys.stderr)
        return 2
    try:
        instrument(argv[1], argv[2], argv[3], argv[4])
    except (UnsupportedSetupEtc, OSError, ValueError) as exc:
        print(f"Unsupported setup-etc: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
