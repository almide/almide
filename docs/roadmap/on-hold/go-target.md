<!-- description: Go codegen target via TOML templates and Go-specific nanopass passes -->
# Go Target

**Priority:** post-1.0
**Prerequisites:** Codegen v3 architecture complete (is_rust()=0, TOML+pass approach)

## Work Items

1. `codegen/templates/go.toml` — Go syntax templates
2. Go-specific passes:
   - `ResultToTuplePass` — Result → (T, error) conversion
   - `GoroutineLoweringPass` — fan → goroutine + channel
3. `runtime/go/` — Go runtime functions
4. CI: cross-target Go tests

## Estimate

Architecture is ready. Can be handled with 1 TOML file + 2-3 passes.
