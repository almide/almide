#!/usr/bin/env python3
"""Generate AI Coding Language Benchmark charts from data.json.

Usage:
    python3 plot.py                    # Generate all charts
    python3 plot.py --output-dir DIR   # Custom output directory

Data source: https://github.com/mame/ai-coding-lang-bench
"""

import json
import re
import argparse
import time
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np

SCRIPT_DIR = Path(__file__).parent
DEFAULT_DATA = SCRIPT_DIR / "data.json"
DEFAULT_OUTPUT = SCRIPT_DIR / "../../../docs/figures"
README_PATH = SCRIPT_DIR / "../../../README.md"

LANG_BENCH_IMAGE_RE = re.compile(
    r"(\!\[[^\]]*\]\(docs/figures/lang-bench-[^)]+\.png)(?:\?v=\d+)?(\))"
)

ALMIDE_COLOR = "#FF6B35"
OTHER_COLOR = "#6B9BFF"
EDGE_COLOR = "#FFFFFF"

BAR_WIDTH = 0.7
FIG_SIZE = (14, 6)
DPI = 200
FONT_SIZE_TITLE = 14
FONT_SIZE_LABEL = 11
FONT_SIZE_BAR = 8
FONT_SIZE_TICK = 9


def load_data(path: Path) -> list[dict]:
    with open(path) as f:
        return json.load(f)["languages"]


def bar_colors(langs: list[dict]) -> list[str]:
    return [ALMIDE_COLOR if l["name"] == "Almide" else OTHER_COLOR for l in langs]


def format_label(lang: dict) -> str:
    name = lang["name"]
    model = lang["model"]
    if model != "opus":
        return f"{name}\n({model})"
    return name


def plot_time(langs: list[dict], output: Path):
    sorted_langs = sorted(langs, key=lambda l: l["avg_total_time"])
    names = [format_label(l) for l in sorted_langs]
    times = [l["avg_total_time"] for l in sorted_langs]
    errs = [l["stddev_time"] for l in sorted_langs]
    colors = bar_colors(sorted_langs)

    fig, ax = plt.subplots(figsize=FIG_SIZE)
    bars = ax.bar(names, times, width=BAR_WIDTH, color=colors,
                  edgecolor=EDGE_COLOR, linewidth=0.5,
                  yerr=errs, capsize=3, error_kw={"linewidth": 1, "color": "#555"})

    for bar, val in zip(bars, times):
        label = f"{val:.0f}" if val >= 100 else f"{val:.1f}"
        ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height() + max(errs) * 0.15,
                label, ha="center", va="bottom", fontsize=FONT_SIZE_BAR, color="#333")

    ax.set_title("AI Coding Language Benchmark: Total Execution Time (v1 + v2)\nLower is better",
                 fontsize=FONT_SIZE_TITLE, fontweight="bold", pad=15)
    ax.set_ylabel("Total Time (seconds)", fontsize=FONT_SIZE_LABEL)
    ax.tick_params(axis="x", labelsize=FONT_SIZE_TICK, rotation=35)
    ax.tick_params(axis="y", labelsize=FONT_SIZE_TICK)
    ax.yaxis.set_major_locator(ticker.MaxNLocator(integer=True))
    ax.set_axisbelow(True)
    ax.grid(axis="y", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    fig.tight_layout()
    fig.savefig(output / "lang-bench-time.png", dpi=DPI, bbox_inches="tight")
    plt.close(fig)
    print(f"  -> {output / 'lang-bench-time.png'}")


def plot_loc(langs: list[dict], output: Path):
    sorted_langs = sorted(langs, key=lambda l: l["avg_v2_loc"])
    names = [format_label(l) for l in sorted_langs]
    locs = [l["avg_v2_loc"] for l in sorted_langs]
    errs = [l["stddev_loc"] for l in sorted_langs]
    colors = bar_colors(sorted_langs)

    fig, ax = plt.subplots(figsize=FIG_SIZE)
    bars = ax.bar(names, locs, width=BAR_WIDTH, color=colors,
                  edgecolor=EDGE_COLOR, linewidth=0.5,
                  yerr=errs, capsize=3, error_kw={"linewidth": 1, "color": "#555"})

    for bar, val in zip(bars, locs):
        ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height() + max(errs) * 0.15,
                str(val), ha="center", va="bottom", fontsize=FONT_SIZE_BAR, color="#333")

    ax.set_title("AI Coding Language Benchmark: Generated Code Size\nLower is more concise",
                 fontsize=FONT_SIZE_TITLE, fontweight="bold", pad=15)
    ax.set_ylabel("Lines of Code (v2)", fontsize=FONT_SIZE_LABEL)
    ax.tick_params(axis="x", labelsize=FONT_SIZE_TICK, rotation=35)
    ax.tick_params(axis="y", labelsize=FONT_SIZE_TICK)
    ax.yaxis.set_major_locator(ticker.MaxNLocator(integer=True))
    ax.set_axisbelow(True)
    ax.grid(axis="y", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    fig.tight_layout()
    fig.savefig(output / "lang-bench-loc.png", dpi=DPI, bbox_inches="tight")
    plt.close(fig)
    print(f"  -> {output / 'lang-bench-loc.png'}")


def plot_pass_rate(langs: list[dict], output: Path):
    sorted_langs = sorted(langs, key=lambda l: l["v2_pass"] / l["trials"], reverse=True)
    names = [format_label(l) for l in sorted_langs]
    rates = [l["v2_pass"] / l["trials"] * 100 for l in sorted_langs]
    labels = [f"{l['v2_pass']}/{l['trials']}" for l in sorted_langs]
    colors = bar_colors(sorted_langs)

    fig, ax = plt.subplots(figsize=FIG_SIZE)
    bars = ax.bar(names, rates, width=BAR_WIDTH, color=colors,
                  edgecolor=EDGE_COLOR, linewidth=0.5)

    for bar, label in zip(bars, labels):
        ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height() + 0.5,
                f"({label})", ha="center", va="bottom", fontsize=FONT_SIZE_BAR, color="#333")

    ax.set_title("AI Coding Language Benchmark: Test Pass Rate\nHigher is better",
                 fontsize=FONT_SIZE_TITLE, fontweight="bold", pad=15)
    ax.set_ylabel("v2 Pass Rate (%)", fontsize=FONT_SIZE_LABEL)
    ax.set_ylim(0, 108)
    ax.tick_params(axis="x", labelsize=FONT_SIZE_TICK, rotation=35)
    ax.tick_params(axis="y", labelsize=FONT_SIZE_TICK)
    ax.set_axisbelow(True)
    ax.grid(axis="y", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    fig.tight_layout()
    fig.savefig(output / "lang-bench-pass-rate.png", dpi=DPI, bbox_inches="tight")
    plt.close(fig)
    print(f"  -> {output / 'lang-bench-pass-rate.png'}")


def bust_readme_cache(readme: Path):
    if not readme.exists():
        return
    v = int(time.time())
    text = readme.read_text()
    new_text = LANG_BENCH_IMAGE_RE.sub(rf"\g<1>?v={v}\2", text)
    if new_text != text:
        readme.write_text(new_text)
        print(f"  -> README.md cache busted (v={v})")


def main():
    parser = argparse.ArgumentParser(description="Generate AI Coding Language Benchmark charts")
    parser.add_argument("--data", type=Path, default=DEFAULT_DATA, help="Path to data.json")
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT, help="Output directory for PNGs")
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    langs = load_data(args.data)

    print(f"Generating charts from {args.data} ({len(langs)} languages)")
    plot_time(langs, args.output_dir)
    plot_loc(langs, args.output_dir)
    plot_pass_rate(langs, args.output_dir)

    bust_readme_cache(README_PATH)
    print("Done.")


if __name__ == "__main__":
    main()
