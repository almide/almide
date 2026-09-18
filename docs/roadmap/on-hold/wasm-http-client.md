<!-- description: Current wasm HTTP lanes and their target-availability authority -->
# WASM HTTP Client

The per-function authority is [target-availability.toml](../../../proofs/target-availability.toml).
HTTP client calls do not silently return placeholder success values.

- Stock `almide build --target wasm` refuses unavailable operations with E081.
- `almide run --target wasm` serves the body/status/bytes HTTP client family
  through the embedded host. Native/embedded parity is covered by C-328.
- The `ALMIDE_COMPONENT_P3` component path serves the supported client family
  over `wasi:http@0.3` (wasmtime `-S http=y`). This is a distinct lane from
  the default stock artifact and from the default WASI 0.2 component.

The full-response family (`request_response`, `get_response`, and their
siblings) remains unavailable on the wasm lanes, including embedded. Read
the exact function's matrix entry when choosing a deployment target.

Implementation and remaining work are tracked in
[#1710](https://github.com/almide/almide/issues/1710) and
[#1628](https://github.com/almide/almide/issues/1628). The earlier proposal
on this page predated these lanes and is superseded by the availability matrix.
