#!/usr/bin/env python3
"""Generate the same-model snapshot chart (2026-07) from data.json.

Reads data.json["snapshot_2026_07"] — five languages measured under identical
conditions (model, trials, harness) — and renders a single 2x2-panel figure:
v2 pass rate / total time / v2 LOC / cost.

Usage:
    python3 plot_snapshot.py
"""

import json
from pathlib import Path

import matplotlib.pyplot as plt

SCRIPT_DIR = Path(__file__).parent
DATA_PATH = SCRIPT_DIR / "data.json"
OUTPUT = SCRIPT_DIR / "../../../docs/figures/lang-bench-snapshot-2026-07.png"

ALMIDE_COLOR = "#FF6B35"
OTHER_COLOR = "#6B9BFF"
INK = "#333333"
DPI = 200


def bar_colors(langs):
    return [ALMIDE_COLOR if l["name"] == "Almide" else OTHER_COLOR for l in langs]


def hbar_panel(ax, langs, values, labels, title, xlabel, errs=None):
    """Horizontal bars, sorted ascending (best-at-top for lower-is-better)."""
    order = sorted(range(len(langs)), key=lambda i: values[i])
    langs = [langs[i] for i in order]
    values = [values[i] for i in order]
    labels = [labels[i] for i in order]
    errs = [errs[i] for i in order] if errs else None

    names = [l["name"] for l in langs]
    y = range(len(langs))
    ax.barh(y, values, height=0.62, color=bar_colors(langs),
            edgecolor="#FFFFFF", linewidth=0.5,
            xerr=errs, capsize=3, error_kw={"linewidth": 1, "color": "#555"})
    ax.set_yticks(y)
    ax.set_yticklabels(names, fontsize=10)
    ax.invert_yaxis()

    reach = max(v + (e if errs else 0) for v, e in zip(values, errs or values))
    for yi, (val, label) in enumerate(zip(values, labels)):
        pad = (errs[yi] if errs else 0) + reach * 0.02
        ax.text(val + pad, yi, label, va="center", ha="left", fontsize=8.5, color=INK)

    ax.set_xlim(0, reach * 1.18)
    ax.set_title(title, fontsize=11.5, fontweight="bold", pad=10, color=INK)
    ax.set_xlabel(xlabel, fontsize=9)
    ax.tick_params(axis="x", labelsize=8)
    ax.set_axisbelow(True)
    ax.grid(axis="x", alpha=0.3)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)


def main():
    data = json.loads(DATA_PATH.read_text())
    snap = data["snapshot_2026_07"]
    langs = snap["languages"]

    fig, axes = plt.subplots(2, 2, figsize=(12.5, 8))

    # Pass rate panel: higher is better -> sort by -rate (use negated values trick avoided; sort desc manually)
    rates = [l["v2_pass"] / l["trials"] * 100 for l in langs]
    order = sorted(range(len(langs)), key=lambda i: -rates[i])
    p_langs = [langs[i] for i in order]
    p_rates = [rates[i] for i in order]
    p_labels = [f"{langs[i]['v2_pass']}/{langs[i]['trials']}" for i in order]
    ax = axes[0][0]
    ax.barh(range(len(p_langs)), p_rates, height=0.62, color=bar_colors(p_langs),
            edgecolor="#FFFFFF", linewidth=0.5)
    ax.set_yticks(range(len(p_langs)))
    ax.set_yticklabels([l["name"] for l in p_langs], fontsize=10)
    ax.invert_yaxis()
    for yi, (r, lab) in enumerate(zip(p_rates, p_labels)):
        ax.text(r + 1.5, yi, lab, va="center", ha="left", fontsize=8.5, color=INK)
    ax.set_xlim(0, 118)
    ax.set_title("v2 pass rate — higher is better", fontsize=11.5, fontweight="bold", pad=10, color=INK)
    ax.set_xlabel("pass rate (%)", fontsize=9)
    ax.tick_params(axis="x", labelsize=8)
    ax.set_axisbelow(True)
    ax.grid(axis="x", alpha=0.3)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)

    hbar_panel(
        axes[0][1], langs,
        [l["avg_total_time"] for l in langs],
        [f"{l['avg_total_time']:.0f}s" for l in langs],
        "Total time (v1 + v2) — lower is better", "seconds",
        errs=[l["stddev_time"] for l in langs],
    )
    hbar_panel(
        axes[1][0], langs,
        [l["avg_v2_loc"] for l in langs],
        [str(l["avg_v2_loc"]) for l in langs],
        "Generated code size (v2) — lower is more concise", "lines of code",
        errs=[l["stddev_loc"] for l in langs],
    )
    hbar_panel(
        axes[1][1], langs,
        [l["avg_cost"] for l in langs],
        [f"${l['avg_cost']:.2f}" for l in langs],
        "API cost per trial — lower is better", "USD",
    )

    fig.suptitle(
        "MiniGit benchmark — same model, same task, same harness\n"
        f"{snap['model_display']} · {snap['trials_per_lang']} trials/language · {snap['date']}",
        fontsize=13.5, fontweight="bold", color=INK,
    )
    fig.text(
        0.5, 0.005,
        "Task: mame/ai-coding-lang-bench MiniGit (v1 implement 11 tests, v2 extend 30 tests). "
        "Almide is the only language absent from training data (learns from CHEATSHEET.md in-context).",
        ha="center", fontsize=8, color="#666666",
    )
    fig.tight_layout(rect=(0, 0.025, 1, 0.93))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(OUTPUT, dpi=DPI, bbox_inches="tight")
    plt.close(fig)
    print(f"-> {OUTPUT}")


if __name__ == "__main__":
    main()
