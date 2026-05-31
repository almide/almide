// JSON parse / stringify / query are now implemented in pure Almide
// (stdlib/json.almd) over the Value ADT, compiled by the normal pipeline to both
// the Rust target and the WASM engine. This native module is retained only as an
// empty placeholder; nothing references it (SSE keeps its own private parser in
// sse.rs). See docs/roadmap/active/json-self-host.md.
