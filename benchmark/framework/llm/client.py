"""LLM API client — thin wrapper around Anthropic Claude API."""

from __future__ import annotations

import re
import time
from dataclasses import dataclass

@dataclass
class GenerationResult:
    code: str
    raw_response: str
    input_tokens: int
    output_tokens: int
    latency_ms: float
    model: str


def generate_code(
    prompt: str,
    *,
    model: str = "claude-sonnet-4-6",
    max_tokens: int = 4096,
    temperature: float = 0.0,
) -> GenerationResult:
    """Call the Claude API and extract code from the response."""
    import anthropic

    client = anthropic.Anthropic()  # reads ANTHROPIC_API_KEY from env

    start = time.monotonic()
    response = client.messages.create(
        model=model,
        max_tokens=max_tokens,
        temperature=temperature,
        messages=[{"role": "user", "content": prompt}],
    )
    elapsed = (time.monotonic() - start) * 1000

    raw = response.content[0].text
    return GenerationResult(
        code=extract_code(raw),
        raw_response=raw,
        input_tokens=response.usage.input_tokens,
        output_tokens=response.usage.output_tokens,
        latency_ms=elapsed,
        model=model,
    )


def generate_code_dry_run(prompt: str, *, model: str = "claude-sonnet-4-6") -> GenerationResult:
    """Simulate a generation without calling the API."""
    return GenerationResult(
        code="// dry-run: no code generated",
        raw_response="[dry-run]",
        input_tokens=len(prompt) // 4,  # rough estimate
        output_tokens=0,
        latency_ms=0.0,
        model=model,
    )


def extract_code(text: str) -> str:
    """Extract code from a markdown code block, or return the whole text if no block found."""
    # Match ```lang\n...\n``` patterns
    pattern = r"```(?:\w+)?\s*\n(.*?)```"
    matches = re.findall(pattern, text, re.DOTALL)
    if matches:
        # Return the longest match (likely the main implementation)
        return max(matches, key=len).strip()
    # No code block — return the raw text (LLM might have output bare code)
    return text.strip()
