<!-- description: Package boundary, runtime sandbox, and supply chain integrity layers -->
# Security Model — Layer 3–5

Almide's security consists of 5 layers.

- **Layer 1** (Effect Isolation) — Implemented ([done/effect-isolation.md](../done/effect-isolation.md))
- **Layer 2** (Capability Restriction) — Implemented ([done/effect-system-phase1-2.md](../done/effect-system-phase1-2.md))
  - Automatic capability inference (IO/Net/Env/Time/Rand/Fan/Log)
  - Restrict via `almide.toml [permissions]`
  - Visualize with `almide check --effects`
  - Integrated into regular `almide check`

## Layer 3: Package Boundary — Capability restriction for dependencies

```toml
[dependencies.api-client]
git = "https://github.com/example/api-client"
allow = ["Net"]  # IO denied

[dependencies.markdown-lib]
allow = []  # pure only
```

- Compile error if a dependency package uses a capability not permitted by `allow`
- → [active/effect-system.md](../active/effect-system.md) Phase 3

## Layer 4: Runtime Sandbox — Runtime isolation

- Capability-based security on WASM target
- File system access virtualization
- Network access allowlist

## Layer 5: Supply Chain Integrity — Package trust verification

- Verify consistency between package capability declarations and code
- Packages with only pure fn are automatically marked "safe"
- Packages containing effect fn require capability audit

## Implementation Priority

| Layer | Content | Status |
|-------|---------|--------|
| 1 | Effect Isolation | ✅ Done |
| 2 | Capability Restriction | ✅ Done |
| 3 | Package Boundary | Not implemented (2.x) |
| 4 | Runtime sandbox | Not implemented (2.x+) |
| 5 | Supply chain | Not implemented (2.x+) |
