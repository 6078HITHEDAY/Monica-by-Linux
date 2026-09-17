#!/usr/bin/env python3
"""CI 键集校验：GTK t()/tf() keys must exist in every gettext catalog."""
from __future__ import annotations

import re
import sys
from pathlib import Path

KEY_RE = re.compile(r'\btf?\(\s*"([A-Za-z0-9_.]+)"')
MSGID_RE = re.compile(r'^msgid\s+"(.*)"\s*$')
MSGSTR_RE = re.compile(r'^msgstr\s+"(.*)"\s*$')
PLACEHOLDER_RE = re.compile(r"\{\d+\}")


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def parse_po(path: Path) -> dict[str, str]:
    catalog: dict[str, str] = {}
    msgid: str | None = None
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        match = MSGID_RE.match(line)
        if match:
            msgid = match.group(1).encode("utf-8").decode("unicode_escape")
            continue
        match = MSGSTR_RE.match(line)
        if match and msgid is not None:
            value = match.group(1).encode("utf-8").decode("unicode_escape")
            if msgid:
                catalog[msgid] = value
            msgid = None
    return catalog


def rust_keys(src_root: Path) -> set[str]:
    keys: set[str] = set()
    for path in src_root.rglob("*.rs"):
        text = path.read_text(encoding="utf-8")
        # Skip unit-test modules so missing-key fallbacks are not required.
        text = re.split(r"#\[cfg\(test\)\]", text, maxsplit=1)[0]
        keys.update(KEY_RE.findall(text))
    return keys


def placeholders(text: str) -> list[str]:
    return sorted(PLACEHOLDER_RE.findall(text))


def main() -> int:
    root = repo_root()
    po_dir = root / "monica-gtk" / "po"
    linguas = (po_dir / "LINGUAS").read_text(encoding="utf-8").split()
    catalogs = {lang: parse_po(po_dir / f"{lang}.po") for lang in linguas}
    if "zh_CN" not in catalogs or "en" not in catalogs:
        print("LINGUAS must include zh_CN and en", file=sys.stderr)
        return 1

    errors: list[str] = []
    base = catalogs["zh_CN"]
    for lang, catalog in catalogs.items():
        missing = sorted(set(base) - set(catalog))
        extra = sorted(set(catalog) - set(base))
        if missing:
            errors.append(f"{lang}: missing keys: {', '.join(missing)}")
        if extra:
            errors.append(f"{lang}: extra keys: {', '.join(extra)}")
        empty = sorted(key for key, value in catalog.items() if value == "")
        if empty:
            errors.append(f"{lang}: empty msgstr: {', '.join(empty)}")
        for key, value in catalog.items():
            if placeholders(value) != placeholders(base.get(key, "")):
                errors.append(f"{lang}: placeholder mismatch for {key}")

    used = rust_keys(root / "monica-gtk" / "crates" / "monica-gtk" / "src")
    missing_in_po = sorted(used - set(base))
    unused = sorted(set(base) - used)
    if missing_in_po:
        errors.append("rust t()/tf() keys missing from po: " + ", ".join(missing_in_po))
    # Catalog-only keys are allowed as documentation (e.g. blocked copy
    # referenced from README) but listed so reviewers see drift.
    if unused:
        print("info: catalog keys not referenced in rust: " + ", ".join(unused))

    if errors:
        print("check-i18n: FAIL")
        for item in errors:
            print(f"  - {item}")
        return 1

    print(f"check-i18n: ok ({len(base)} keys, zh_CN+en, {len(used)} used in rust)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
