# MSR Benchmark (archived)

> **Daily MSR measurement has moved to [almide/almide-dojo](https://github.com/almide/almide-dojo).**

This directory contains the original one-shot MSR benchmark scripts and outputs that validated Almide's modification survival rate during early development. The code and results are preserved for historical reference.

For continuous MSR measurement, task bank management, and malicious-hint detection, see the Dojo repository. The Dojo harness is written in Almide itself and uses `claude` CLI for model invocation.

## What's here

- `msr.almd` — original Almide-based benchmark runner (single-run)
- `scripts/` — shell scripts for batch runs and measurements
- `prompts/` — system prompts used in benchmark runs
- `outputs/` — LLM-generated solutions (Opus, Haiku, Sonnet)
- `results/` — aggregated results
- `python/`, `moonbit/` — cross-language comparison variants

## Status

No new work should be added here. Use `almide/almide-dojo` instead.
