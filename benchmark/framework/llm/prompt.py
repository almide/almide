"""Prompt construction for each benchmark metric."""

from __future__ import annotations

from pathlib import Path


def build_far_prompt(
    problem_spec: str,
    template: str,
    language: str,
    *,
    cheatsheet: str | None = None,
) -> str:
    """Build the prompt for First-Attempt success Rate measurement.

    The LLM sees the problem spec + a function-signature template and must
    produce a complete, working implementation on its first try.
    """
    parts: list[str] = []

    if cheatsheet:
        parts.append(f"## Language Reference\n{cheatsheet}")

    parts.append(f"""## Problem
{problem_spec}

## Template (fill in the implementation)
```{language}
{template}
```

Output ONLY the complete source code. No explanation.""")

    return "\n\n".join(parts)


def build_msr_prompt(
    problem_spec: str,
    solution: str,
    modification_request: str,
    language: str,
    *,
    cheatsheet: str | None = None,
) -> str:
    """Build the prompt for Modification Survival Rate measurement.

    The LLM receives a working solution and a modification request.
    It must modify the code so that it still passes all existing tests
    plus any new tests for the modification.
    """
    parts: list[str] = []

    if cheatsheet:
        parts.append(f"## Language Reference\n{cheatsheet}")

    parts.append(f"""## Problem
{problem_spec}

## Current working implementation
```{language}
{solution}
```

## Modification request
{modification_request}

Modify the code to satisfy the request. The modified code must still pass all existing tests.
Output ONLY the complete modified source code. No explanation.""")

    return "\n\n".join(parts)


def build_fle_prompt(
    problem_spec: str,
    broken_code: str,
    error_output: str,
    language: str,
    *,
    cheatsheet: str | None = None,
    attempt: int = 1,
) -> str:
    """Build the prompt for Fix-Loop Efficiency measurement.

    The LLM receives broken code + compiler/test error output and must fix it.
    `attempt` tracks which iteration of the fix loop we're on.
    """
    parts: list[str] = []

    if cheatsheet:
        parts.append(f"## Language Reference\n{cheatsheet}")

    iteration_note = ""
    if attempt > 1:
        iteration_note = f"\n\nThis is fix attempt #{attempt}. Previous fixes did not resolve all issues."

    parts.append(f"""## Problem
{problem_spec}

## Broken code
```{language}
{broken_code}
```

## Error output
```
{error_output}
```{iteration_note}

Fix the code so that it compiles and passes all tests.
Output ONLY the complete fixed source code. No explanation.""")

    return "\n\n".join(parts)


def load_cheatsheet(project_root: Path) -> str | None:
    """Load the Almide cheatsheet from docs/CHEATSHEET.md if it exists."""
    path = project_root / "docs" / "CHEATSHEET.md"
    if path.exists():
        return path.read_text()
    return None
