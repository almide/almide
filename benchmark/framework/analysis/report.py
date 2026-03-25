"""Generate Markdown benchmark reports."""

from __future__ import annotations

from datetime import datetime, timezone
from .aggregate import FARSummary, MSRSummary, FLESummary, compare_languages


def generate_report(
    far_results: list[FARSummary] | None = None,
    msr_results: list[MSRSummary] | None = None,
    fle_results: list[FLESummary] | None = None,
    *,
    model: str = "",
    trials: int = 0,
) -> str:
    """Generate a Markdown report from aggregated benchmark results."""
    lines: list[str] = []
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    lines.append(f"# Almide LLM Benchmark Report")
    lines.append("")
    lines.append(f"**Date**: {now}  ")
    if model:
        lines.append(f"**Model**: `{model}`  ")
    if trials:
        lines.append(f"**Trials per problem**: {trials}  ")
    lines.append("")

    if far_results:
        lines.extend(_far_section(far_results))

    if msr_results:
        lines.extend(_msr_section(msr_results))

    if fle_results:
        lines.extend(_fle_section(fle_results))

    lines.append("")
    return "\n".join(lines)


def _far_section(results: list[FARSummary]) -> list[str]:
    lines = [
        "## First-Attempt Success Rate (FAR)",
        "",
        "| Problem | Language | Trials | Successes | Rate |",
        "|---------|----------|--------|-----------|------|",
    ]
    for r in results:
        lines.append(
            f"| {r.problem} | {r.language} | {r.trials} | {r.successes} | {r.rate:.0%} |"
        )
    lines.append("")

    # Add language comparison if multiple languages present
    languages = sorted(set(r.language for r in results))
    if len(languages) >= 2:
        lines.append("### Pairwise Comparison (Fisher exact test)")
        lines.append("")
        for i, la in enumerate(languages):
            for lb in languages[i + 1 :]:
                p_values = compare_languages(results, la, lb)
                if p_values:
                    lines.append(f"**{la} vs {lb}**:")
                    for problem, p in sorted(p_values.items()):
                        sig = " *" if p < 0.05 else ""
                        lines.append(f"- {problem}: p={p:.4f}{sig}")
                    lines.append("")

    return lines


def _msr_section(results: list[MSRSummary]) -> list[str]:
    lines = [
        "## Modification Survival Rate (MSR)",
        "",
        "| Problem | Language | Trials | Survivals | Rate |",
        "|---------|----------|--------|-----------|------|",
    ]
    for r in results:
        lines.append(
            f"| {r.problem} | {r.language} | {r.trials} | {r.survivals} | {r.rate:.0%} |"
        )
    lines.append("")
    return lines


def _fle_section(results: list[FLESummary]) -> list[str]:
    lines = [
        "## Fix-Loop Efficiency (FLE)",
        "",
        "| Problem | Language | Trials | Fixed | Avg Attempts | Rate |",
        "|---------|----------|--------|-------|--------------|------|",
    ]
    for r in results:
        avg = f"{r.avg_attempts:.1f}" if r.fixed > 0 else "-"
        lines.append(
            f"| {r.problem} | {r.language} | {r.trials} | {r.fixed} | {avg} | {r.rate:.0%} |"
        )
    lines.append("")
    return lines
