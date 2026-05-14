<!-- description: lumen — pure graphics math library (vec3, mat4, color), used by webgl/canvas/obsid -->
# lumen — Pure Graphics Math

## Status

v0.1.0 shipped (2026-04-12). Three submodules, 26 tests, zero @extern.

Repository: [github.com/almide/lumen](https://github.com/almide/lumen)

## What it does

Provides the math foundation that graphics packages share:

- `lumen.vec3` — 3D vectors, dot/cross product, normalize, lerp
- `lumen.mat4` — 4x4 matrices, perspective, look_at, rotate, translate
- `lumen.color` — Color type, RGB/hex conversion, mix, lighten/darken

All pure computation — no `@extern(wasm)`, works on every target (Rust native, WASM).

## Integration

- wasm-webgl: replaced inline `mat.almd` with `import lumen.mat4`
- obsid: added `lumen` dependency
- wasm-canvas: added `lumen` dependency

## What's next

- `lumen.vec2` — 2D vectors
- `lumen.quat` — quaternion rotations
- `lumen.mesh` — vertex buffer builder (extract from obsid examples)
- `lumen.noise` — Perlin/simplex noise
