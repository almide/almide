<!-- description: Shipped wasm HTTP client lanes and the remaining default-component transition -->
# WASM HTTP Client

## Shipped surface

The per-function authority is
[target-availability.toml](../../../proofs/target-availability.toml); the
current API is in the [HTTP reference](../../stdlib/http.md). Do not infer
availability from an older family name or from the presence of a wasm target.

- The full-response client, `http.get_response` and `http.send`, works on
  the WASI 0.3 component lane and on `almide run --target wasm`'s embedded
  host. The former public `http.request_response` spelling is replaced by
  `http.send(http.new_request(...))`.
- The default stock-p1 artifact still has no HTTP import and refuses those
  calls with E081. For the p3 artifact, build with `ALMIDE_COMPONENT_P3=1`
  and `--component`, then run with `wasmtime run -S http=y`.
- Availability is per operation: check the matrix separately for call handles
  and serving rather than extending the full-response claim to every HTTP API.

Evidence: [http_response_cross_test.rs](../../../tests/http_response_cross_test.rs)
compares the full response's status, headers, body and failures with native;
[target-availability.toml](../../../proofs/target-availability.toml) records
`http.get_response` and `http.send` as unavailable only on `stock-p1`.

## Remaining plan

The client is not an unimplemented feature. The remaining deployment transition
belongs to [ADR-0023's implementation plan](../../adr/0023-wasm-http-over-wasi-http.md#7-implementation-plan),
including the default-component flip and retirement of the private host paths.
This page remains at the path that ADR's documentation step cites; it is not a
second implementation checklist. Follow the matrix and current tests when
judging an individual step, not this plan's historical API spellings.
