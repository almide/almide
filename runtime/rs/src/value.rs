// The Value data model and all value.* operations are now implemented in pure
// Almide (stdlib/value.almd) over a Value ADT, compiled by the normal pipeline
// to both the Rust target and the WASM engine. This native module is retained
// only as an empty placeholder; nothing references it. See
// docs/roadmap/active/json-self-host.md.
