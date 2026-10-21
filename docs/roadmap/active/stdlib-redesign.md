<!-- description: Three-layer redesign of the standard library against nine languages — rules, category list, per-module role tables, a verdict for every public function, and the issues the adopted proposals became -->
# Stdlib redesign (three layers)

The design record lives in [docs/roadmap/stdlib-redesign/](../stdlib-redesign/README.md):

- [L0](../stdlib-redesign/L0-conventions.md): the cross-module rules. These cover naming, placement, in-place writers, failure, conversions and compatibility.
- [L1](../stdlib-redesign/L1-categories.md): the module list compared against Rust, Go, Python, TypeScript/JavaScript, Gleam, Elixir, OCaml, Kotlin and Swift.
- `L2-<module>.md`: one page per module. Each has the role comparison and a verdict for every public function (999 today).
- [README](../stdlib-redesign/README.md): every proposal with its verdict (○ / × / 保留) and its issue.

Renames and removals use the epoch-15 deprecation window, with removal in epoch 16 (R-L0-14).
