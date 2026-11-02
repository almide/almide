<!-- description: Shipped wasm HTTP client lanes and the remaining default-component transition -->
# WASM HTTP Client

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](wasm-http-client.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-http-client.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-http-client.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-http-client.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


## Shipped surface

The per-function authority is
[target-availability.toml](../../../../../../proofs/target-availability.toml); the
current API is in the [HTTP reference](../../../../../stdlib/http.md). Do not infer
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

Evidence: [http_response_cross_test.rs](../../../../../../tests/http_response_cross_test.rs)
compares the full response's status, headers, body and failures with native;
[target-availability.toml](../../../../../../proofs/target-availability.toml) records
`http.get_response` and `http.send` as unavailable only on `stock-p1`.

## Remaining plan

The client is not an unimplemented feature. The remaining deployment transition
belongs to [ADR-0023's implementation plan](../../../../../adr/0023-wasm-http-over-wasi-http.md#7-implementation-plan),
including the default-component flip and retirement of the private host paths.
This page remains at the path that ADR's documentation step cites; it is not a
second implementation checklist. Follow the matrix and current tests when
judging an individual step, not this plan's historical API spellings.
