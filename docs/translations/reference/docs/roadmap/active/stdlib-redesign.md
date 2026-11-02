<!-- description: Three-layer redesign of the standard library against nine languages — rules, category list, per-module role tables, a verdict for every public function, and the issues the adopted proposals became -->
# Stdlib redesign (three layers)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](stdlib-redesign.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](stdlib-redesign.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](stdlib-redesign.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/stdlib-redesign.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


The design record lives in [docs/roadmap/stdlib-redesign/](../../../../../roadmap/stdlib-redesign/README.md):

- [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md): the cross-module rules. These cover naming, placement, in-place writers, failure, conversions and compatibility.
- [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md): the module list compared against Rust, Go, Python, TypeScript/JavaScript, Gleam, Elixir, OCaml, Kotlin and Swift.
- `L2-<module>.md`: one page per module. Each has the role comparison and a verdict for every public function (1,001 today).
- [README](../../../../../roadmap/stdlib-redesign/README.md): every proposal with its verdict (○ / × / 保留) and its issue.
- [Target surface](../../../../../roadmap/stdlib-redesign/target-surface.md): the standard library once every adopted proposal lands — every public function with its post-redesign signature (1,001 today → 1,289), per module, and where each removed function goes.
- [Protocol pyramid](../../../../../roadmap/stdlib-redesign/protocol-view.md): the standard protocols layered after Swift, each type as the protocols it conforms to plus its own functions, the holes the conformances expose and the decisions they force.

Renames and removals use the epoch-15 deprecation window, with removal in epoch 16 (R-L0-14).
