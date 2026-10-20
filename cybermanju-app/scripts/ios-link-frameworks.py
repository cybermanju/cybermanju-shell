#!/usr/bin/env python3
# CyberManju OS — iOS framework linker settings (scripts/ios-link-frameworks.py).
# Role: patch the generated Xcode project with native frameworks missing from Tauri's template.
# Exports: add_system_configuration(path) and a small command-line entry point.
# Integrates: scripts/ios-configure.sh, Tauri's generated gen/apple project.pbxproj.
# Contracts: idempotent; adds SystemConfiguration to build settings without deleting existing linker flags.

from __future__ import annotations

import re
import sys
from pathlib import Path


BUILD_SETTINGS = re.compile(
    r"(?ms)(?P<prefix>^[ \t]*buildSettings[ \t]*=[ \t]*\{)"
    r"(?P<body>.*?)"
    r"(?P<suffix>^[ \t]*\};)"
)
ARRAY_FLAGS = re.compile(
    r"(?ms)(?P<prefix>^[ \t]*OTHER_LDFLAGS[ \t]*=[ \t]*\()"
    r"(?P<items>.*?)"
    r"(?P<suffix>^[ \t]*\);)"
)
SCALAR_FLAGS = re.compile(
    r"(?m)(?P<indent>^[ \t]*)OTHER_LDFLAGS[ \t]*=[ \t]*(?P<value>[^;\n]*);"
)


def _patch_build_settings(match: re.Match[str]) -> tuple[str, bool]:
    body = match.group("body")
    array = ARRAY_FLAGS.search(body)
    if array:
        if "SystemConfiguration" in array.group(0):
            return match.group(0), False
        indent = re.match(r"[ \t]*", array.group("prefix")).group(0)
        items = array.group("items").rstrip()
        if items and not items.endswith(","):
            items += ","
        item_indent_match = re.search(r"(?m)^([ \t]+)\S", items)
        item_indent = item_indent_match.group(1) if item_indent_match else indent + "\t"
        addition = (
            f"\n{item_indent}\"-framework\","
            f"\n{item_indent}\"SystemConfiguration\",\n"
        )
        patched_array = (
            array.group("prefix") + items + addition + array.group("suffix")
        )
        new_body = body[: array.start()] + patched_array + body[array.end() :]
        return match.group("prefix") + new_body + match.group("suffix"), True

    scalar = SCALAR_FLAGS.search(body)
    if scalar:
        if "SystemConfiguration" in scalar.group(0):
            return match.group(0), False
        value = scalar.group("value").strip()
        if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
            content = value[1:-1].rstrip()
            new_value = f'"{content} -framework SystemConfiguration"'
        else:
            new_value = f'"{value} -framework SystemConfiguration"'
        replacement = (
            f"{scalar.group('indent')}OTHER_LDFLAGS = {new_value};"
        )
        new_body = (
            body[: scalar.start()]
            + replacement
            + body[scalar.end() :]
        )
        return match.group("prefix") + new_body + match.group("suffix"), True

    setting = re.search(r"(?m)^([ \t]+)[A-Z0-9_]+[ \t]*=", body)
    indent = setting.group(1) if setting else "\t\t\t\t"
    addition = (
        f'\n{indent}OTHER_LDFLAGS = "$(inherited) -framework '
        'SystemConfiguration";\n'
    )
    return (
        match.group("prefix") + body.rstrip() + addition + match.group("suffix"),
        True,
    )


def add_system_configuration(path: Path) -> int:
    text = path.read_text(encoding="utf-8")
    matched = 0
    changed = 0

    def replace(match: re.Match[str]) -> str:
        nonlocal matched, changed
        matched += 1
        replacement, did_change = _patch_build_settings(match)
        changed += int(did_change)
        return replacement

    updated = BUILD_SETTINGS.sub(replace, text)
    if matched == 0:
        raise ValueError(f"no Xcode buildSettings blocks found in {path}")
    if changed:
        path.write_text(updated, encoding="utf-8")
    return changed


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {Path(sys.argv[0]).name} <project.pbxproj>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    if not path.is_file():
        print(f"error: Xcode project file does not exist: {path}", file=sys.stderr)
        return 2
    try:
        changed = add_system_configuration(path)
    except (OSError, UnicodeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    if changed:
        print(
            "linked SystemConfiguration.framework in "
            f"{changed} Xcode build-settings block(s): {path}"
        )
    else:
        print(f"SystemConfiguration.framework already linked: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
