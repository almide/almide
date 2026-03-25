"""Aggregate benchmark results from JSONL files."""

from __future__ import annotations

import json
import math
from pathlib import Path
from collections import defaultdict
from dataclasses import dataclass


@dataclass
class FARSummary:
    language: str
    problem: str
    trials: int
    successes: int
    rate: float  # success / trials


@dataclass
class MSRSummary:
    language: str
    problem: str
    trials: int
    survivals: int
    rate: float


@dataclass
class FLESummary:
    language: str
    problem: str
    trials: int
    fixed: int
    avg_attempts: float  # average attempts to fix (among those that were fixed)
    rate: float  # fixed / trials


def _load_results(results_dir: Path, metric: str) -> list[dict]:
    """Load all JSONL entries for a given metric."""
    records = []
    for f in sorted(results_dir.glob(f"{metric}_*.jsonl")):
        for line in f.read_text().splitlines():
            line = line.strip()
            if line:
                records.append(json.loads(line))
    return records


def aggregate_far(results_dir: Path) -> list[FARSummary]:
    """Aggregate FAR results by (language, problem)."""
    records = _load_results(results_dir, "far")
    groups: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for r in records:
        groups[(r["language"], r["problem"])].append(r)

    summaries = []
    for (lang, problem), entries in sorted(groups.items()):
        successes = sum(1 for e in entries if e["success"])
        total = len(entries)
        summaries.append(FARSummary(
            language=lang,
            problem=problem,
            trials=total,
            successes=successes,
            rate=successes / total if total > 0 else 0.0,
        ))
    return summaries


def aggregate_msr(results_dir: Path) -> list[MSRSummary]:
    """Aggregate MSR results by (language, problem)."""
    records = _load_results(results_dir, "msr")
    groups: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for r in records:
        groups[(r["language"], r["problem"])].append(r)

    summaries = []
    for (lang, problem), entries in sorted(groups.items()):
        survivals = sum(1 for e in entries if e["success"])
        total = len(entries)
        summaries.append(MSRSummary(
            language=lang,
            problem=problem,
            trials=total,
            survivals=survivals,
            rate=survivals / total if total > 0 else 0.0,
        ))
    return summaries


def aggregate_fle(results_dir: Path) -> list[FLESummary]:
    """Aggregate FLE results by (language, problem)."""
    records = _load_results(results_dir, "fle")
    groups: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for r in records:
        groups[(r["language"], r["problem"])].append(r)

    summaries = []
    for (lang, problem), entries in sorted(groups.items()):
        fixed = [e for e in entries if e.get("fixed", False)]
        total = len(entries)
        avg_att = (
            sum(e["attempts"] for e in fixed) / len(fixed) if fixed else 0.0
        )
        summaries.append(FLESummary(
            language=lang,
            problem=problem,
            trials=total,
            fixed=len(fixed),
            avg_attempts=avg_att,
            rate=len(fixed) / total if total > 0 else 0.0,
        ))
    return summaries


def fisher_exact_p(a: int, b: int, c: int, d: int) -> float:
    """One-sided Fisher exact test p-value for a 2x2 contingency table.

    Table:
        a  b
        c  d

    Tests whether a/(a+b) > c/(c+d).
    Uses log-factorials to avoid overflow.
    """
    n = a + b + c + d

    def log_fact(k: int) -> float:
        return sum(math.log(i) for i in range(1, k + 1))

    def hypergeom_log_pmf(x: int) -> float:
        r1, r2, c1, c2 = a + b, c + d, a + c, b + d
        return (
            log_fact(r1) + log_fact(r2) + log_fact(c1) + log_fact(c2)
            - log_fact(n) - log_fact(x)
            - log_fact(r1 - x) - log_fact(c1 - x) - log_fact(r2 - c1 + x)
        )

    r1 = a + b
    c1 = a + c
    x_min = max(0, c1 - (c + d))
    x_max = min(r1, c1)

    base_log_p = hypergeom_log_pmf(a)
    p_value = 0.0
    for x in range(x_min, x_max + 1):
        lp = hypergeom_log_pmf(x)
        if lp >= base_log_p - 1e-10:  # as extreme or more
            p_value += math.exp(lp)

    return min(p_value, 1.0)


def compare_languages(
    summaries: list[FARSummary] | list[MSRSummary],
    lang_a: str,
    lang_b: str,
) -> dict[str, float]:
    """Compare two languages using Fisher exact test on each problem.

    Returns a dict mapping problem -> p-value.
    """
    by_problem: dict[str, dict[str, object]] = defaultdict(dict)
    for s in summaries:
        by_problem[s.problem][s.language] = s

    results = {}
    for problem, langs in sorted(by_problem.items()):
        sa = langs.get(lang_a)
        sb = langs.get(lang_b)
        if sa is None or sb is None:
            continue
        a = sa.successes if hasattr(sa, "successes") else sa.survivals
        b = sa.trials - a
        c = sb.successes if hasattr(sb, "successes") else sb.survivals
        d = sb.trials - c
        results[problem] = fisher_exact_p(a, b, c, d)

    return results
