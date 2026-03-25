#!/usr/bin/env python3
"""Almide LLM benchmark runner.

Measures LLM code generation accuracy across languages:
  FAR  — First-Attempt success Rate
  MSR  — Modification Survival Rate
  FLE  — Fix-Loop Efficiency

Usage:
  python3 benchmark/runner.py --metric far --lang almide,python,ts --trials 10
  python3 benchmark/runner.py --metric far --lang almide --dry-run
  python3 benchmark/runner.py --metric far --problem pangram --trials 5
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

# Resolve paths relative to this script
BENCH_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = BENCH_DIR.parent
PROBLEMS_DIR = BENCH_DIR / "problems"
RESULTS_DIR = BENCH_DIR / "results"

sys.path.insert(0, str(BENCH_DIR))

from adapters import ADAPTERS
from llm.client import generate_code, generate_code_dry_run
from llm.prompt import build_far_prompt, build_msr_prompt, build_fle_prompt, load_cheatsheet
from analysis.aggregate import aggregate_far, aggregate_msr, aggregate_fle
from analysis.report import generate_report


# Language directory names in problem sets -> adapter keys
LANG_MAP = {
    "almide": "almide",
    "python": "python",
    "ts": "ts",
}

# Language -> subdirectory name in problems/
LANG_DIRS = {
    "almide": "almide",
    "python": "python",
    "ts": "typescript",
}

# Language -> test file glob pattern
TEST_PATTERNS = {
    "almide": "tests.almd",
    "python": "test_*.py",
    "ts": "*.test.ts",
}

# Language -> template file name
TEMPLATE_NAMES = {
    "almide": "template.almd",
    "python": "template.py",
    "ts": "template.ts",
}


def load_config(config_path: Path) -> dict:
    """Load YAML config if available, otherwise return defaults."""
    defaults = {
        "defaults": {
            "model": "claude-sonnet-4-6",
            "max_tokens": 4096,
            "temperature": 0.0,
            "trials": 10,
            "timeout": 30,
        },
        "languages": ["almide", "python", "ts"],
        "problems": [],
        "almide": {"binary": "almide", "include_cheatsheet": True},
        "fle": {"max_attempts": 5},
    }
    if config_path.exists():
        try:
            import yaml
            with open(config_path) as f:
                user_cfg = yaml.safe_load(f) or {}
            # Merge: user overrides defaults
            for key in defaults:
                if key in user_cfg:
                    if isinstance(defaults[key], dict):
                        defaults[key].update(user_cfg[key])
                    else:
                        defaults[key] = user_cfg[key]
        except ImportError:
            print("Warning: pyyaml not installed, using defaults", file=sys.stderr)
    return defaults


def discover_problems(problem_filter: str | None = None) -> list[str]:
    """Discover available problems in the problems/ directory."""
    problems = []
    for d in sorted(PROBLEMS_DIR.iterdir()):
        if d.is_dir() and (d / "SPEC.md").exists():
            problems.append(d.name)
    if problem_filter:
        problems = [p for p in problems if p == problem_filter]
    return problems


def find_test_file(problem: str, lang: str) -> Path | None:
    """Find the test file for a given problem and language."""
    lang_dir = PROBLEMS_DIR / problem / LANG_DIRS[lang]
    if not lang_dir.exists():
        return None
    pattern = TEST_PATTERNS[lang]
    matches = list(lang_dir.glob(pattern))
    return matches[0] if matches else None


def find_template(problem: str, lang: str) -> Path | None:
    """Find the template file for a given problem and language."""
    p = PROBLEMS_DIR / problem / LANG_DIRS[lang] / TEMPLATE_NAMES[lang]
    return p if p.exists() else None


def find_solution(problem: str, lang: str) -> Path | None:
    """Find the solution file for a given problem and language."""
    lang_dir = PROBLEMS_DIR / problem / LANG_DIRS[lang]
    for name in [f"solution{ADAPTERS[lang].extension}", f"solution.{lang}"]:
        p = lang_dir / name
        if p.exists():
            return p
    return None


def load_spec(problem: str) -> str:
    """Load the SPEC.md for a problem."""
    return (PROBLEMS_DIR / problem / "SPEC.md").read_text()


def run_far(
    problems: list[str],
    languages: list[str],
    trials: int,
    model: str,
    dry_run: bool,
    cheatsheet: str | None,
    timeout: int,
) -> None:
    """Run First-Attempt success Rate benchmark."""
    RESULTS_DIR.mkdir(exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
    output_path = RESULTS_DIR / f"far_{timestamp}.jsonl"

    total = len(problems) * len(languages) * trials
    done = 0

    print(f"=== FAR Benchmark ===")
    print(f"Model: {model}")
    print(f"Problems: {', '.join(problems)}")
    print(f"Languages: {', '.join(languages)}")
    print(f"Trials: {trials}")
    print(f"Total runs: {total}")
    if dry_run:
        print(f"Mode: DRY RUN (no API calls)")
    print()

    for problem in problems:
        spec = load_spec(problem)
        for lang in languages:
            template_path = find_template(problem, lang)
            test_path = find_test_file(problem, lang)
            if not template_path or not test_path:
                print(f"  SKIP {problem}/{lang} — missing template or test file")
                continue

            template = template_path.read_text()
            adapter = ADAPTERS[lang]()

            lang_label = "almide" if lang == "almide" else lang
            cs = cheatsheet if lang == "almide" else None

            for trial in range(1, trials + 1):
                done += 1
                prompt = build_far_prompt(spec, template, lang_label, cheatsheet=cs)

                if dry_run:
                    gen = generate_code_dry_run(prompt, model=model)
                else:
                    gen = generate_code(prompt, model=model)

                # Test the generated code
                if dry_run:
                    success = None
                    compile_error = None
                    test_output = "[dry-run]"
                else:
                    result = adapter.compile_and_test(gen.code, test_path, timeout=timeout)
                    success = result.success
                    compile_error = result.compile_error
                    test_output = result.stdout + result.stderr

                record = {
                    "metric": "far",
                    "problem": problem,
                    "language": lang,
                    "trial": trial,
                    "model": model,
                    "success": success,
                    "compile_error": compile_error,
                    "input_tokens": gen.input_tokens,
                    "output_tokens": gen.output_tokens,
                    "latency_ms": gen.latency_ms,
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                }

                with open(output_path, "a") as f:
                    f.write(json.dumps(record) + "\n")

                status = "?" if dry_run else ("PASS" if success else "FAIL")
                print(f"  [{done}/{total}] {problem}/{lang} trial {trial}: {status}")

    print(f"\nResults written to {output_path}")

    if not dry_run:
        summaries = aggregate_far(RESULTS_DIR)
        report = generate_report(far_results=summaries, model=model, trials=trials)
        report_path = RESULTS_DIR / f"report_{timestamp}.md"
        report_path.write_text(report)
        print(f"Report written to {report_path}")


def run_msr(
    problems: list[str],
    languages: list[str],
    trials: int,
    model: str,
    dry_run: bool,
    cheatsheet: str | None,
    timeout: int,
) -> None:
    """Run Modification Survival Rate benchmark."""
    RESULTS_DIR.mkdir(exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
    output_path = RESULTS_DIR / f"msr_{timestamp}.jsonl"

    print(f"=== MSR Benchmark ===")
    print(f"Model: {model}")
    print(f"Problems: {', '.join(problems)}")
    print(f"Languages: {', '.join(languages)}")
    print(f"Trials: {trials}")
    if dry_run:
        print(f"Mode: DRY RUN")
    print()
    print("NOTE: MSR requires modification specs in SPEC.md (not yet implemented for these problems)")
    print("      This is a placeholder showing the flow.")
    print()

    # MSR needs modification requests defined per problem — future work
    for problem in problems:
        for lang in languages:
            solution_path = find_solution(problem, lang)
            if not solution_path:
                print(f"  SKIP {problem}/{lang} — no solution file")
                continue
            print(f"  {problem}/{lang}: solution found at {solution_path.name}")

    print(f"\nResults would be written to {output_path}")


def run_fle(
    problems: list[str],
    languages: list[str],
    trials: int,
    model: str,
    dry_run: bool,
    cheatsheet: str | None,
    timeout: int,
    max_attempts: int = 5,
) -> None:
    """Run Fix-Loop Efficiency benchmark."""
    RESULTS_DIR.mkdir(exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
    output_path = RESULTS_DIR / f"fle_{timestamp}.jsonl"

    total = len(problems) * len(languages) * trials
    done = 0

    print(f"=== FLE Benchmark ===")
    print(f"Model: {model}")
    print(f"Problems: {', '.join(problems)}")
    print(f"Languages: {', '.join(languages)}")
    print(f"Trials: {trials}")
    print(f"Max fix attempts: {max_attempts}")
    if dry_run:
        print(f"Mode: DRY RUN")
    print()

    for problem in problems:
        spec = load_spec(problem)
        for lang in languages:
            template_path = find_template(problem, lang)
            test_path = find_test_file(problem, lang)
            if not template_path or not test_path:
                print(f"  SKIP {problem}/{lang} — missing template or test file")
                continue

            template = template_path.read_text()
            adapter = ADAPTERS[lang]()
            lang_label = "almide" if lang == "almide" else lang
            cs = cheatsheet if lang == "almide" else None

            for trial in range(1, trials + 1):
                done += 1

                # Step 1: initial FAR attempt
                prompt = build_far_prompt(spec, template, lang_label, cheatsheet=cs)
                if dry_run:
                    gen = generate_code_dry_run(prompt, model=model)
                    print(f"  [{done}/{total}] {problem}/{lang} trial {trial}: DRY-RUN (would attempt up to {max_attempts} fixes)")
                    record = {
                        "metric": "fle",
                        "problem": problem,
                        "language": lang,
                        "trial": trial,
                        "model": model,
                        "fixed": None,
                        "attempts": 0,
                        "timestamp": datetime.now(timezone.utc).isoformat(),
                    }
                    with open(output_path, "a") as f:
                        f.write(json.dumps(record) + "\n")
                    continue

                gen = generate_code(prompt, model=model)
                current_code = gen.code
                result = adapter.compile_and_test(current_code, test_path, timeout=timeout)

                if result.success:
                    print(f"  [{done}/{total}] {problem}/{lang} trial {trial}: PASS on first attempt")
                    record = {
                        "metric": "fle",
                        "problem": problem,
                        "language": lang,
                        "trial": trial,
                        "model": model,
                        "fixed": True,
                        "attempts": 1,
                        "timestamp": datetime.now(timezone.utc).isoformat(),
                    }
                    with open(output_path, "a") as f:
                        f.write(json.dumps(record) + "\n")
                    continue

                # Step 2: fix loop
                fixed = False
                for attempt in range(2, max_attempts + 1):
                    error_output = result.stderr + result.stdout
                    fix_prompt = build_fle_prompt(
                        spec, current_code, error_output, lang_label,
                        cheatsheet=cs, attempt=attempt,
                    )
                    gen = generate_code(fix_prompt, model=model)
                    current_code = gen.code
                    result = adapter.compile_and_test(current_code, test_path, timeout=timeout)

                    if result.success:
                        fixed = True
                        print(f"  [{done}/{total}] {problem}/{lang} trial {trial}: FIXED on attempt {attempt}")
                        record = {
                            "metric": "fle",
                            "problem": problem,
                            "language": lang,
                            "trial": trial,
                            "model": model,
                            "fixed": True,
                            "attempts": attempt,
                            "timestamp": datetime.now(timezone.utc).isoformat(),
                        }
                        with open(output_path, "a") as f:
                            f.write(json.dumps(record) + "\n")
                        break

                if not fixed:
                    print(f"  [{done}/{total}] {problem}/{lang} trial {trial}: FAILED after {max_attempts} attempts")
                    record = {
                        "metric": "fle",
                        "problem": problem,
                        "language": lang,
                        "trial": trial,
                        "model": model,
                        "fixed": False,
                        "attempts": max_attempts,
                        "timestamp": datetime.now(timezone.utc).isoformat(),
                    }
                    with open(output_path, "a") as f:
                        f.write(json.dumps(record) + "\n")

    print(f"\nResults written to {output_path}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Almide LLM benchmark runner",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--metric",
        choices=["far", "msr", "fle"],
        required=True,
        help="Benchmark metric: far (first-attempt rate), msr (modification survival rate), fle (fix-loop efficiency)",
    )
    parser.add_argument(
        "--lang",
        default="almide,python,ts",
        help="Comma-separated languages to benchmark (default: almide,python,ts)",
    )
    parser.add_argument(
        "--model",
        default=None,
        help="LLM model to use (default: from config or claude-sonnet-4-6)",
    )
    parser.add_argument(
        "--trials",
        type=int,
        default=None,
        help="Number of trials per (problem, language) pair (default: from config or 10)",
    )
    parser.add_argument(
        "--problem",
        default=None,
        help="Run only a specific problem (default: all)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Skip API calls, show execution flow only",
    )
    parser.add_argument(
        "--config",
        default=None,
        help="Path to config.yaml (default: benchmark/config.yaml)",
    )

    args = parser.parse_args()

    config_path = Path(args.config) if args.config else BENCH_DIR / "config.yaml"
    cfg = load_config(config_path)

    model = args.model or cfg["defaults"]["model"]
    trials = args.trials or cfg["defaults"]["trials"]
    timeout = cfg["defaults"]["timeout"]
    languages = [l.strip() for l in args.lang.split(",")]
    problems = discover_problems(args.problem)

    if not problems:
        print(f"No problems found in {PROBLEMS_DIR}", file=sys.stderr)
        if args.problem:
            print(f"  (filtered by --problem {args.problem})", file=sys.stderr)
        sys.exit(1)

    # Validate languages
    for lang in languages:
        if lang not in ADAPTERS:
            print(f"Unknown language: {lang}. Available: {', '.join(ADAPTERS.keys())}", file=sys.stderr)
            sys.exit(1)

    # Load cheatsheet for Almide prompts
    cheatsheet = None
    if "almide" in languages and cfg.get("almide", {}).get("include_cheatsheet", True):
        cheatsheet = load_cheatsheet(PROJECT_ROOT)

    dispatch = {
        "far": run_far,
        "msr": run_msr,
        "fle": run_fle,
    }

    kwargs = dict(
        problems=problems,
        languages=languages,
        trials=trials,
        model=model,
        dry_run=args.dry_run,
        cheatsheet=cheatsheet,
        timeout=timeout,
    )

    if args.metric == "fle":
        kwargs["max_attempts"] = cfg.get("fle", {}).get("max_attempts", 5)

    dispatch[args.metric](**kwargs)


if __name__ == "__main__":
    main()
