#!/usr/bin/env python3
"""把版本號往上加一版，讓自檢修正能觸發自動更新。"""
from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def next_version(version: str) -> str:
    major, minor, patch = (int(part) for part in version.split("."))
    return f"{major}.{minor}.{patch + 1}"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", help="指定版本，例如 0.6.78；不給就 patch + 1")
    args = parser.parse_args()

    cargo = ROOT / "Cargo.toml"
    text = cargo.read_text(encoding="utf-8")
    match = re.search(r'^version = "(\d+\.\d+\.\d+)"', text, re.M)
    if not match:
        raise SystemExit("Cargo.toml 找不到版本")
    old = match.group(1)
    new = args.version or next_version(old)
    if new == old:
        raise SystemExit("版本沒有變")

    cargo.write_text(text.replace(f'version = "{old}"', f'version = "{new}"', 1), encoding="utf-8", newline="")
    lock = ROOT / "Cargo.lock"
    lock_text = lock.read_text(encoding="utf-8")
    lock_text, count = re.subn(
        r'(name = "kuidao"\nversion = ")\d+\.\d+\.\d+(")',
        rf"\g<1>{new}\g<2>",
        lock_text,
        count=1,
    )
    if not count:
        raise SystemExit("Cargo.lock 找不到 kuidao 版本")
    lock.write_text(lock_text, encoding="utf-8", newline="")

    rc = ROOT / "app.rc"
    rc_text = rc.read_text(encoding="utf-8")
    rc_text = re.sub(r"FILEVERSION \d+,\d+,\d+,0", f"FILEVERSION {new.replace('.', ',')},0", rc_text)
    rc_text = re.sub(r"PRODUCTVERSION \d+,\d+,\d+,0", f"PRODUCTVERSION {new.replace('.', ',')},0", rc_text)
    rc_text = re.sub(r'("(?:File|Product)Version", )"\d+\.\d+\.\d+"', rf'\g<1>"{new}"', rc_text)
    rc.write_text(rc_text, encoding="utf-8", newline="")

    nsi = ROOT / "installer" / "setup.nsi"
    nsi_text = nsi.read_text(encoding="utf-8")
    nsi_text = re.sub(r'VIProductVersion "\d+\.\d+\.\d+\.0"', f'VIProductVersion "{new}.0"', nsi_text)
    nsi_text = re.sub(r'("(?:File|Product)Version", )"\d+\.\d+\.\d+"', rf'\g<1>"{new}"', nsi_text)
    nsi.write_text(nsi_text, encoding="utf-8", newline="")

    print(new)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
