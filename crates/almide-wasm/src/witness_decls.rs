//! The DECLARATION table of each emission pass (#2759) — what the name and
//! capability witnesses need that the bytes do not carry.
//!
//! A wasm function index says nothing about the source function it came
//! from, and the capability witness bounds each function by what its SOURCE
//! declares (`effect fn` or not). While a witness sweep collects, every
//! emission pass records, beside its finished module bytes:
//!
//! - each program function's and `main`'s index, name and declared class
//!   (pure / effect), and each lifted lambda's index and name;
//! - the `@extern(wasm, ..)` stub indices `imports::declare` turned into
//!   imports, so the projector can renumber the table into the index space
//!   of the bytes (the declare map, [`crate::imports::remap_index`]).
//!
//! Indices are recorded in the PRE-declare space the emitter assigned; the
//! projector (cert_project.rs) renumbers them. A caller picks the pass whose
//! `bytes` equal the module `emit_program` returned — the shipped pass.

use std::sync::Mutex;

/// What a function's source signature declares about host effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declared {
    /// A plain `fn`: console output only (the language admits `println` in
    /// any function; every other host effect needs `effect fn`).
    Pure,
    /// An `effect fn`: every modeled capability.
    Effect,
}

/// One function the source declares, at its pre-declare wasm index.
#[derive(Clone, Debug)]
pub struct DeclFn {
    pub index: u32,
    pub name: String,
    /// `None` = no source declaration (a lifted lambda): the witness gives
    /// it the least bound its own reach needs, checked by the graph form.
    pub declared: Option<Declared>,
}

/// One emission pass's declaration table and the module it produced.
#[derive(Clone, Debug, Default)]
pub struct PassDecls {
    pub fns: Vec<DeclFn>,
    /// Pre-declare indices of the `@extern(wasm, ..)` stubs, ascending.
    pub stubs: Vec<u32>,
    /// The pass's finished module (post-declare, pre-`to_wasi`).
    pub bytes: Vec<u8>,
}

fn sink() -> &'static Mutex<Option<Vec<PassDecls>>> {
    use std::sync::OnceLock;
    static S: OnceLock<Mutex<Option<Vec<PassDecls>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

pub(crate) fn start() {
    *sink().lock().expect("decls sink") = Some(Vec::new());
}

pub(crate) fn collecting() -> bool {
    sink().lock().expect("decls sink").is_some()
}

pub(crate) fn record(pass: PassDecls) {
    if let Some(v) = sink().lock().expect("decls sink").as_mut() {
        v.push(pass);
    }
}

/// Every pass's table, in emission order; the sink is disarmed.
pub fn take() -> Vec<PassDecls> {
    sink().lock().expect("decls sink").take().unwrap_or_default()
}

/// The table of the pass that produced `shipped` (the bytes `emit_program`
/// returned), if a sweep recorded one.
pub fn take_for(shipped: &[u8]) -> Option<PassDecls> {
    take().into_iter().rev().find(|p| p.bytes == shipped)
}
