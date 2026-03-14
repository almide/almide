# Self-Hosting

**Status**: Far Future
**Priority**: Low
**Prerequisite**: Language spec stabilization, benchmark parity with Python/Go

## Summary

Rewrite the Almide compiler in Almide itself, replacing the current Rust implementation.

## Why Not Now

- Language spec is still evolving (generics Phase 1, no trait/impl yet, no recursive generic variants)
- Every spec change would create a bootstrapping problem (need old compiler to build new compiler)
- Development velocity would drop significantly
- Almide's mission is **modification survival rate**, not self-proof — effort is better spent on benchmarks and stdlib

## Prerequisites Before Starting

- [ ] type system extensions complete (row polymorphism, container protocols — see [Type System Extensions](../active/type-system.md))
- [ ] Type system extensions done
- [ ] Generics fully stable (recursive variants, trait bounds)
- [ ] Benchmark results competitive with Python/Go across multiple tasks
- [ ] Language spec freeze (or near-freeze)

## When It Makes Sense

- Language spec is stable enough that bootstrap breakage is rare
- Need to demonstrate "Almide can build real systems" for adoption
- Dogfooding value outweighs development cost
