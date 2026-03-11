# Self-Tooling: Editor Tools Written in Almide [DONE]

Demonstrate that Almide's entire editor ecosystem can be written in Almide itself, produced by AI from scratch. This proves the mission: "LLMs can mass-produce an ecosystem in Almide."

## tree-sitter Grammar Generator ✅

**100% Almide. Zero JS glue.**

Approach: Model tree-sitter grammar rules as an algebraic data type, serialize to grammar.js via pattern matching.

```almide
type Rule = | Seq(List[Rule]) | Choice(List[Rule]) | Repeat(Rule) | Str(String) | Ref(String) | ...

fn emit(rule: Rule) -> String = match rule {
  Seq(rules) => "seq(" ++ string.join(list.map(rules, emit), ", ") ++ ")"
  Ref(name) => "$." ++ name
  ...
}
```

Files: `generator/rule.almd` (~40 lines), `generator/almide_grammar.almd` (~400 lines), `generator/main.almd` (~20 lines)

Build: `almide build main.almd -o gen-grammar && ./gen-grammar > grammar.js`

### Compiler bugs discovered and fixed
- Cross-module variant type access: `use super::<mod>::<Type>::*` not emitted
- Non-generic recursive variants: `Box::new()` not auto-inserted at constructor call sites
- Both fixed in v0.5.0

## Chrome Extension (Planned)

**Almide + ~30 lines JS bootstrap.**

Approach: Logic in Almide, DOM/shiki via `@extern` FFI.

- `highlight.almd` (~120 lines): theme detection, block search, HTML generation
- `bootstrap.js` (~30 lines): shiki init, MutationObserver, global facade
- Build: `almide build --target npm` → esbuild bundle

## TextMate Grammar Generator (Potential)

Generate `almide.tmLanguage.json` from Almide data structures. Same code-generation approach as tree-sitter.

## Impact

| Tool | Almide purity | Status |
|------|--------------|--------|
| tree-sitter grammar | 100% | ✅ Done |
| Chrome extension | ~80% | Planned |
| TextMate grammar | 100% | Potential |
