<!-- description: Improve readability of generated Rust and TypeScript output -->
# Emit Readability

**Priority:** Medium — Directly affects the quality of generated code that LLMs modify
**Prerequisites:** codegen v3 (TOML template walker) completed
**Goal:** Improve readability of `--target rust` / `--target ts` output for both humans and LLMs

> "Generated code readability directly impacts modification survival rate."

---

## Why

Almide's mission is "the language LLMs can write most accurately." There are cases where LLMs read and modify code emitted via `--target rust/ts` (debugging, integration, learning). The current emit output is correct, but there is room for readability improvement:

- Source structure (blank lines, logical blocks) is lost
- Variable names are preserved, but comments are not
- Generated code formatting is mechanical, sometimes obscuring intent

Highly readable generated code:
1. Makes it easier for LLMs to grasp context, improving modification accuracy
2. Is easier for humans to debug
3. Functions as a "source map" of the generated code

---

## Design

### What to Preserve

| Element | Current | Goal |
|---|---|---|
| Variable names | ✅ Preserved | Maintain |
| Function names | ✅ Preserved | Maintain |
| Blank lines (logical block separators) | ❌ Removed | Reflect source blank lines in emit output |
| Doc comments | ❌ Removed | Emit as `/// ...` / `/** ... */` |
| Inline comments | ❌ Removed | Future consideration |
| Source function order | ✅ Preserved | Maintain |
| Import order | △ Mechanical | Logical grouping |

### Non-Goals

- Complete preservation of source comments (internal compiler comments are not emitted)
- Exact match with hand-written code quality (aim for "readable machine output")

---

## Phases

### Phase 1: Blank Line Preservation

- [ ] Parser: record blank line positions in AST/IR (`blank_lines_before: u32`)
- [ ] IR: add blank line annotation to `IrStmt`
- [ ] Walker: insert blank lines during emit based on blank line annotations
- [ ] Test: verify that source logical block structure is reflected in emit output

### Phase 2: Doc Comment Preservation

- [ ] Parser: record doc comments (`/// ...`) in AST
- [ ] IR: add doc comment field to function/type definitions
- [ ] Rust emit: output as `/// ...`
- [ ] TS emit: output as `/** ... */`
- [ ] WASM: don't emit comments (binary format)

### Phase 3: Import Grouping

- [ ] Rust emit: group by `use std::` / `use crate::` / external crates + blank lines
- [ ] TS emit: group by stdlib / local modules
- [ ] Logical ordering (stdlib → external → local)

### Phase 4: Formatting Quality

- [x] Iterator chain emission: `list.map/filter/fold` → `.into_iter().map().collect()` (v0.10.4)
- [x] Math intrinsics inline: `math.sqrt(x)` → `x.sqrt()` instead of `almide_rt_math_sqrt(x)` (v0.10.4)
- [x] Numeric cast inline: `float.from_int(n)` → `(n as f64)` (v0.10.4)
- [x] Borrow parameter inference: read-only String/List params → `&str` / `&[T]` (v0.10.4)
- [ ] Improve line break rules for long expressions (builder chains, long argument lists)
- [ ] Alignment of match/when arms
- [ ] Target readability level where `rustfmt` / `prettier` aren't needed to read the output

---

## Implementation Notes

- Blank line preservation passes through Parser → IR → Walker, so the change scope is wide
- Doc comments are a lightweight change — adding `Option<String>` to IR
- Add blank line / comment placeholders to TOML templates

---

## Success Criteria

- `almide app.almd --target rust` output preserves the logical structure of the source
- Doc comments are reflected in Rust/TS output
- Improved accuracy when asking an LLM "what does this function do" given emit output (qualitative evaluation)
