#!/usr/bin/env python3
"""The README's "at a glance" program is examples/order-total.almd without its tests.

Each of the three READMEs must show exactly that code, and the output block under it must
be what the program prints. It runs in the Emit & Format job, which the prose lane keeps, so
a README-only change is checked too.

Usage: check-readme-example.py <almide binary>
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = "examples/order-total.almd"
READMES = {
    "README.md": "## Almide at a glance",
    "README.ja.md": "## Almide をひと目で",
    "README.zh-CN.md": "## 一眼看懂 Almide",
}


def fence_after(text, heading, lang, where):
    start = text.find(heading)
    if start < 0:
        sys.exit(f"{where}: no `{heading}` section")
    rest = text[start:]
    opener = f"```{lang}\n"
    at = rest.find(opener)
    if at < 0:
        sys.exit(f"{where}: no ```{lang} fence under `{heading}`")
    body = rest[at + len(opener):]
    end = body.find("\n```")
    if end < 0:
        sys.exit(f"{where}: unclosed ```{lang} fence")
    return body[:end]


def main():
    almide = sys.argv[1]
    code = (ROOT / EXAMPLE).read_text().split('\ntest "')[0].rstrip("\n")
    run = subprocess.run([almide, "run", EXAMPLE, "--target", "wasm"], cwd=ROOT, text=True, capture_output=True)
    if run.returncode != 0:
        sys.exit(f"{EXAMPLE} failed to run:\n{run.stderr}")
    printed = run.stdout.rstrip("\n")
    failed = False
    for name, heading in READMES.items():
        text = (ROOT / name).read_text()
        if fence_after(text, heading, "almd", name) != code:
            print(f"::error::{name}: the code under `{heading}` differs from {EXAMPLE} (without its tests)")
            failed = True
        shown = fence_after(text, heading, "text", name)
        if shown != printed:
            print(f"::error::{name}: the README shows {shown!r}, the program prints {printed!r}")
            failed = True
    if failed:
        sys.exit(1)
    print(f"readme-example: {len(READMES)} READMEs show {EXAMPLE} and its output {printed!r}")


if __name__ == "__main__":
    main()
