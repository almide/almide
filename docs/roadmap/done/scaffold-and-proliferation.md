<!-- description: Scaffold command and LLM module proliferation pipeline -->
# almide scaffold & Module Proliferation Pipeline [MERGED]

> Merged into [LLM Integration](../active/llm-integration.md) as `almide forge`.

Infrastructure for mass-producing Almide modules. Enables LLMs to autonomously generate, verify, and publish modules — the core loop of AI proliferation.

## Part 1: `almide scaffold` Command

### Concept

Given only "what to build", scaffold generates a convention-compliant skeleton.
Filling in a skeleton is far more accurate for LLMs than writing from scratch (confirmed by template fidelity findings in minigit benchmark).

### Usage

```bash
# Basic (name + description)
almide scaffold csv "CSV parser and writer"

# With function signatures
almide scaffold csv "CSV parser and writer" \
  --fn "parse(input: String) -> List[List[String]]" \
  --fn "parse_with_header(input: String) -> List[Map[String, String]]" \
  --fn "stringify(rows: List[List[String]]) -> String"

# With test skeleton
almide scaffold csv "CSV parser and writer" --with-tests
```

### Output

```
stdlib/csv.almd           # Module body (signatures + stubs)
tests/csv_test.almd       # Test skeleton (with --with-tests)
```

#### Generated csv.almd

```almide
// csv — CSV parser and writer
// Bundled stdlib module (written in Almide, auto multi-target)

fn parse(input: String) -> List[List[String]] = {
  // TODO: implement
  []
}

fn parse_with_header(input: String) -> List[Map[String, String]] = {
  // TODO: implement
  []
}

fn stringify(rows: List[List[String]]) -> String = {
  // TODO: implement
  ""
}
```

#### Generated csv_test.almd

```almide
import csv

fn main() = {
  // parse
  let result = csv.parse("a,b,c\n1,2,3")
  assert(list.len(result) == 2)

  // parse_with_header
  let rows = csv.parse_with_header("name,age\nAlice,30")
  assert(list.len(rows) == 1)

  // stringify
  let output = csv.stringify([["a", "b"], ["1", "2"]])
  assert(string.contains?(output, "a,b"))

  io.println("csv: all tests passed")
}
```

### Implementation Phases

| Phase | What | Effort |
|-------|------|--------|
| **S1** | `almide scaffold <name> <desc>` → template-based .almd generation | Small |
| **S2** | `--fn` flag → typed stub generation from signatures | Small |
| **S3** | `--with-tests` → test skeleton generation | Small |
| **S4** | `--fill` → call LLM API to implement the stubs (integrates with Part 2) | Medium |

S1–S3 are pure template expansion. A few dozen lines added to the compiler.
S4 is the real payload — connects to the pipeline in Part 2.

---

## Part 1.5: Reference-Driven Scaffold (Prior Art Mining)

### Concept

Every useful module is a "reinvention of the wheel." Other languages already have battle-tested implementations with comprehensive test suites. Instead of inventing specs from scratch, mine existing implementations for:

1. **API surface** — what functions does a mature csv library expose?
2. **Edge cases** — what do their tests cover that we'd never think of?
3. **Behavioral spec** — tests ARE the spec, more precise than any prose description.

### Pipeline

```
almide scaffold csv "CSV parser and writer"
        │
        ▼
┌───────────────────────────────────────────┐
│  1. Reference Discovery                   │
│  Search GitHub/package registries for:    │
│  - Python: csv (stdlib), pandas.read_csv  │
│  - Ruby: CSV (stdlib)                     │
│  - Go: encoding/csv                       │
│  - Rust: csv crate                        │
│  - JavaScript: papaparse, csv-parse       │
└───────────────┬───────────────────────────┘
                │
┌───────────────▼───────────────────────────┐
│  2. License Filter                        │
│  Keep only: MIT, Apache-2.0, BSD-2/3,     │
│             ISC, Unlicense, public domain  │
│  Reject: GPL, LGPL, AGPL, proprietary     │
│  Record: license + source URL per ref     │
└───────────────┬───────────────────────────┘
                │
┌───────────────▼───────────────────────────┐
│  3. Test Case Analysis                    │
│  From each reference, extract:            │
│  - Test file paths (test_csv.py, etc.)    │
│  - Test case names + descriptions         │
│  - Input/output pairs                     │
│  - Edge cases (empty input, quoting,      │
│    newlines in fields, Unicode, BOM, etc.) │
└───────────────┬───────────────────────────┘
                │
┌───────────────▼───────────────────────────┐
│  4. Test Aspect Synthesis                 │
│  Merge test observations across all refs  │
│  into a unified coverage matrix:          │
│                                           │
│  csv test aspects:                        │
│  ☐ basic comma-separated parsing          │
│  ☐ quoted fields ("hello, world")         │
│  ☐ escaped quotes ("say ""hi""")          │
│  ☐ newlines within quoted fields          │
│  ☐ empty fields (a,,b)                    │
│  ☐ trailing newline                       │
│  ☐ custom delimiter (tab, semicolon)      │
│  ☐ header row extraction                  │
│  ☐ empty input → empty list              │
│  ☐ single row, single column             │
│  ☐ Unicode content                        │
│  ☐ stringify round-trip (parse ∘ stringify │
│    = identity for well-formed input)      │
└───────────────┬───────────────────────────┘
                │
┌───────────────▼───────────────────────────┐
│  5. Generate Almide Tests                 │
│  Translate aspects into csv_test.almd     │
│  with concrete input/output assertions    │
│  (NOT copied code — original test logic   │
│   derived from observed patterns)         │
└───────────────┬───────────────────────────┘
                │
                ▼
        Standard pipeline continues:
        LLM Fill → Verify → Publish
```

### Why This Works for AI Proliferation

- **LLM accuracy scales with test quality.** The #1 reason LLM-generated code is wrong is missing edge cases. Mining 5 mature implementations surfaces edge cases no single developer would enumerate.
- **Tests-first = specification-first.** The LLM receives concrete input→output pairs, not vague descriptions. This is the most unambiguous way to communicate intent to an LLM.
- **License safety is automated.** No human needs to audit — the pipeline rejects GPL-family before any code is read. We never copy implementation code, only derive test aspects from observed patterns.
- **Cross-language synthesis.** Python's csv handles quoting well, Go's handles streaming, Rust's handles custom delimiters. The union of their test suites produces better coverage than any single reference.

### Implementation

| Phase | What | Effort |
|-------|------|--------|
| **R1** | Reference discovery script (GitHub API + registry queries) | Small |
| **R2** | License checker (read LICENSE/package.json/Cargo.toml) | Small |
| **R3** | Test extraction (LLM reads test files → outputs aspect list) | Medium |
| **R4** | Aspect synthesis (merge + deduplicate across references) | Medium |
| **R5** | Almide test generation (aspects → .almd test assertions) | Medium |

R1–R2 are mechanical scripts. R3–R5 use LLM calls — the same LLM that will later implement the module. This means the LLM understands the test suite deeply before writing a single line of implementation.

### Attribution

Generated modules include a comment header recording provenance:

```almide
// csv — CSV parser and writer
// Test aspects derived from: Python csv (PSF), Go encoding/csv (BSD-3),
//   Rust csv crate (MIT), papaparse (MIT)
// No source code was copied. Tests were independently written based on
// observed behavioral patterns across reference implementations.
```

---

## Part 2: Module Auto-Generation Pipeline

### Goal

Given "module name + description + function signatures", an LLM generates the implementation, passes tests, and produces a ready-to-use module. Works for both stdlib and external packages.

### Pipeline Overview

```
                 ┌─────────────────────────────────────┐
                 │  Input: module spec                  │
                 │  name: "csv"                         │
                 │  desc: "CSV parser and writer"       │
                 │  fns: [parse, stringify, ...]        │
                 └──────────────┬──────────────────────┘
                                │
                 ┌──────────────▼──────────────────────┐
   Part 1        │  almide scaffold                     │
                 │  → csv.almd (stub)                   │
                 └──────────────┬──────────────────────┘
                                │
                 ┌──────────────▼──────────────────────┐
   Part 1.5      │  Prior Art Mining                    │
                 │  → reference discovery + license     │
                 │  → test aspect extraction            │
                 │  → csv_test.almd (comprehensive)     │
                 └──────────────┬──────────────────────┘
                                │
                 ┌──────────────▼──────────────────────┐
   LLM Fill      │  Implement                           │
                 │  Input: stub + tests + CLAUDE.md     │
                 │  Output: csv.almd (implemented)      │
                 └──────────────┬──────────────────────┘
                                │
                 ┌──────────────▼──────────────────────┐
                 │  Verify                              │
                 │  1. almide check csv.almd            │
                 │  2. almide run csv_test.almd         │
                 │  3. almide run csv_test.almd --target ts │
                 │  (both targets pass = multi-target OK)   │
                 └──────────────┬──────────────────────┘
                                │
                 ┌──────────────▼──────────────────────┐
                 │  Publish                             │
                 │  stdlib → bundled in compiler binary  │
                 │  external → git repo + registry      │
                 └──────────────────────────────────────┘
```

### LLM Fill Details (S4)

Context provided to the LLM:

```
1. CLAUDE.md (language spec summary)
2. Stub file (type signatures + TODOs)
3. Test file (expected behavior)
4. One similar existing module (e.g., args.almd as pattern reference)
5. Stdlib function list (available tools)
```

LLM output: implemented .almd file.

**Key insight**: scaffold locks the type signatures, so the LLM only fills in bodies.
This applies the "higher template fidelity → faster completion" finding from minigit benchmarks.

### Extending to External Packages

The only difference between stdlib and external packages is the deployment target. The generation pipeline is identical.

```
stdlib module:
  stdlib/csv.almd → bundled in compiler binary → ships with almide

external package:
  packages/csv/
    almide.toml          # Package metadata
    src/csv.almd         # Implementation
    tests/csv_test.almd  # Tests
    README.md            # Auto-generated docs
```

#### External package almide.toml

```toml
[package]
name = "csv"
version = "0.1.0"
description = "CSV parser and writer"
generated_by = "almide-proliferate"  # Explicitly marks LLM-generated origin

[dependencies]
# Dependencies on other Almide packages (if any)
```

### Batch Production: `almide proliferate`

Mass-produce modules from a spec file.

```bash
# Define multiple modules in spec.toml
almide proliferate spec.toml

# Single module
almide proliferate --name csv --desc "CSV parser and writer" \
  --fn "parse(input: String) -> List[List[String]]" \
  --fn "stringify(rows: List[List[String]]) -> String"
```

#### spec.toml Example

```toml
[[module]]
name = "csv"
description = "CSV parser and writer"
fns = [
  "parse(input: String) -> List[List[String]]",
  "parse_with_header(input: String) -> List[Map[String, String]]",
  "stringify(rows: List[List[String]]) -> String",
]

[[module]]
name = "toml"
description = "TOML parser"
fns = [
  "parse(input: String) -> Map[String, String]",
  "stringify(data: Map[String, String]) -> String",
]

[[module]]
name = "ini"
description = "INI file parser"
fns = [
  "parse(input: String) -> Map[String, Map[String, String]]",
  "stringify(sections: Map[String, Map[String, String]]) -> String",
]

[[module]]
name = "semver"
description = "Semantic versioning"
fns = [
  "parse(version: String) -> Result[Map[String, Int], String]",
  "compare(a: String, b: String) -> Int",
  "satisfies?(version: String, constraint: String) -> Bool",
]
```

```bash
almide proliferate spec.toml
# → csv: ✅ generated, ✅ type-check, ✅ test-rust, ✅ test-ts
# → toml: ✅ generated, ✅ type-check, ✅ test-rust, ✅ test-ts
# → ini: ✅ generated, ✅ type-check, ✅ test-rust, ✅ test-ts
# → semver: ✅ generated, ✅ type-check, ❌ test-rust (1 failure)
#   → retry with error context...
#   → semver: ✅ generated, ✅ type-check, ✅ test-rust, ✅ test-ts
```

### Retry Strategy

When LLM-generated code fails tests:

```
Attempt 1: stub + spec + example module
Attempt 2: Attempt 1 output + compile error / test failure message
Attempt 3: Attempt 2 output + errors + hints (common pattern guide)
Max 3 attempts. After 3 failures → human review queue.
```

The better Almide's compiler errors (with fix suggestions), the higher the success rate on Attempt 2.
→ Generics Phase 1 (error improvements) directly boosts pipeline success rate.

---

## Part 3: Registry & Discovery

How other LLM agents find and use mass-produced modules.

### Phase R1: Git-based Registry (Minimal)

```
almide-packages/          # Single monorepo
  index.toml              # Package index
  csv/
    almide.toml
    src/csv.almd
    tests/csv_test.almd
  toml/
    almide.toml
    src/toml.almd
    tests/toml_test.almd
```

```bash
# Consumer side
almide add csv                   # git clone + add to almide.toml
import csv
let rows = csv.parse(input)
```

### Phase R2: CLAUDE.md Integration

When an LLM works on an Almide project, auto-inject available packages into CLAUDE.md:

```markdown
## Available Packages
- csv: parse, parse_with_header, stringify
- toml: parse, stringify
- semver: parse, compare, satisfies?
```

This lets the LLM know packages exist before writing code.
More packages → higher LLM accuracy → more code generated → proliferation accelerates.

---

## Implementation Priority

| Priority | Item | Depends On | Impact |
|----------|------|------------|--------|
| **P0** | `almide scaffold` (S1-S3) | None | Foundation for skeleton generation |
| **P0.5** | Prior art mining (R1-R5) | None | Comprehensive test generation from existing libs |
| **P1** | LLM Fill as external script | scaffold + tests | Single module auto-generation |
| **P2** | Verify loop (check + test both targets) | P1 | Quality assurance |
| **P3** | `almide proliferate` (batch) | P1 + P2 | Mass production |
| **P4** | External package structure (almide.toml) | module-system-v2 | Expansion beyond stdlib |
| **P5** | Git registry + `almide add` | P4 | Distribution |
| **P6** | CLAUDE.md auto-injection | P5 | Automated LLM discovery |

**P0 + P0.5 + P1 is the minimum viable loop.** Scaffold generates stubs, prior art mining generates comprehensive tests from existing OSS implementations, then an external script (Ruby/Python) calls the LLM API to fill the implementation. Verify with `almide check` + `almide run` on both targets.

---

## Success Metrics

| Metric | Target |
|--------|--------|
| scaffold → fill → verify success rate | > 80% (1st attempt) |
| Average time per module generation | < 120s |
| Average cost per module generation | < $0.50 |
| Module candidates defined in spec.toml | 50+ |
| Stdlib functions listed in CLAUDE.md | 200+ (currently ~80) |
