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
//! - per function, how many of its clock reads are the fuel meter's
//!   (`DeclFn::meter_clock_reads`, the #3041 ruling);
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
    /// How many of the body's wall-clock reads (`almide.fs_call` op 34) are
    /// the FUEL METER's deadline test (`emit_det_cut_check`), not the
    /// source's.
    ///
    /// RULING (#3041): the meter's read is a RUNTIME-INTERNAL capability,
    /// outside the frame's declared bound. It fires only while a
    /// `fan.timeout` region is armed (deadline != MAX), and it exists to
    /// enforce that region's deadline, not to observe the clock: the frame
    /// never sees the value, only the cut. The region's opener
    /// (`almide_rt_prim_timeout_enter`) reads the clock itself to arm the
    /// deadline, and that read stays in its frame's direct capabilities.
    /// So the capability is charged where the source asked for it, and a
    /// plain fn called inside the region, or a lambda or synthesized region
    /// body, is not held to CLOCK for the meter's test. The alternative,
    /// moving the read into a helper, leaves the graph checker charging
    /// every caller with the helper's bound, so it would not change the
    /// verdict.
    pub meter_clock_reads: u32,
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

#[cfg(test)]
mod tests {
    //! The declaration table as the emitter records it (emit.rs
    //! `record_decls`) and as the name / capability projector reads it
    //! (cert_project.rs) — driven through real programs, accept and refuse.
    use super::*;
    use crate::cert_project::{self, cap};
    use std::sync::Mutex;

    /// The sinks are process-wide: these tests arm them one at a time.
    static ARMED: Mutex<()> = Mutex::new(());

    /// Lower and emit `src` with the witness sinks armed; the shipped
    /// module and the declaration table of the pass that produced it.
    fn emit_recorded(name: &str, src: &str) -> (Vec<u8>, PassDecls) {
        let ir = almide_spine::s5::lower_to_ir(name, src).expect("front");
        crate::witness::start_collecting();
        let (bytes, _) = crate::emit_program_with_ops(&ir).expect("emits");
        let _ = crate::witness::take();
        let decls = take_for(&bytes).expect("the shipped pass recorded its declarations");
        (bytes, decls)
    }

    fn by_name<'a>(d: &'a PassDecls, name: &str) -> &'a DeclFn {
        d.fns.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no declaration for {name}: {:?}", d.fns))
    }

    fn accepts(property: almide_verify::Property, w: &str) -> bool {
        almide_verify::check(property, w.as_bytes())
    }

    const PROGRAM: &str = r#"
fn twice(xs: List[Int]) -> List[Int] = xs + xs
fn adder(n: Int) -> (Int) -> Int = (x) => x + n
effect fn greet(who: String) -> Unit = {
  println("hi " + who)
}
effect fn main() -> Unit = {
  let f = adder(2)
  println(int.to_string(list.len(twice([1, 2]))) + int.to_string(f(3)))
  greet("w")!
}
"#;

    #[test]
    fn a_program_records_each_source_declaration_and_its_lambdas() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let (bytes, d) = emit_recorded("decls.almd", PROGRAM);
        assert!(d.stubs.is_empty());
        assert_eq!(by_name(&d, "twice").declared, Some(Declared::Pure));
        assert_eq!(by_name(&d, "greet").declared, Some(Declared::Effect));
        assert_eq!(by_name(&d, "main").declared, Some(Declared::Effect));
        assert!(d.fns.iter().any(|f| f.name.starts_with("<lambda#") && f.declared.is_none()), "{:?}", d.fns);
        // Every witness the projector builds from them is accepted.
        let names = cert_project::names(&bytes, &|i| format!("f{i}")).expect("names");
        assert!(names.iter().all(|(_, w)| accepts(almide_verify::Property::Names, w)), "{names:?}");
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        assert!(flat.iter().all(|(_, w)| accepts(almide_verify::Property::Caps, w)), "{flat:?}");
        assert!(accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
        // The declared names land on real indices of the module.
        let named = cert_project::decl_names(&d).expect("decl names");
        assert!(named.values().any(|n| n == "greet"));
    }

    #[test]
    fn a_self_host_body_is_not_a_declaration() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let src = "import random\neffect fn main() -> Unit = {\n  let n = random.int(1, 6)\n  println(int.to_string(n - n))\n}\n";
        let (_, d) = emit_recorded("selfhost.almd", src);
        let impls: Vec<&DeclFn> = d.fns.iter().filter(|f| f.name.starts_with("__selfhost_")).collect();
        assert!(!impls.is_empty(), "random.int links a self-host body: {:?}", d.fns);
        assert!(impls.iter().all(|f| f.declared.is_none()));
        // Its entropy draw is within main's `effect` bound, not the body's own `fn`.
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        let main = flat.iter().find(|(n, _)| n == "main").expect("main's caps witness");
        assert!(main.1.split('|').nth(1).is_some_and(|u| u.split(' ').any(|c| c == cap::ENTROPY.to_string())), "{main:?}");
        assert!(accepts(almide_verify::Property::Caps, &main.1));
        assert!(accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
    }

    #[test]
    fn an_extern_stub_is_renumbered_and_a_plain_caller_of_it_is_refused() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let src = "@extern(wasm, \"js\", \"js_add\")\nfn js_add(a: Int, b: Int) -> Int\n\nfn sum(a: Int) -> Int = js_add(a, 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(sum(2)))\n}\n";
        let (_, d) = emit_recorded("extern.almd", src);
        assert_eq!(d.stubs.len(), 1, "one @extern stub: {:?}", d.stubs);
        // The stub became an import: its name no longer lands on a body.
        let named = cert_project::decl_names(&d).expect("decl names");
        let imports = cert_project::function_imports(&d.bytes).expect("imports");
        let stub_at = named.iter().find(|(_, n)| *n == "js_add").map(|(i, _)| *i).expect("js_add renumbered");
        assert!(stub_at < imports, "js_add is an import now (index {stub_at} of {imports})");
        // A plain fn reaching a foreign host function is outside its bound.
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        let sum = flat.iter().find(|(n, _)| n == "sum").expect("sum's caps witness");
        assert!(sum.1.ends_with(&cap::FOREIGN.to_string()), "{sum:?}");
        assert!(!accepts(almide_verify::Property::Caps, &sum.1), "{sum:?}");
        assert!(!accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
    }

    #[test]
    fn nothing_is_recorded_unless_a_sweep_collects() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let ir = almide_spine::s5::lower_to_ir("quiet.almd", PROGRAM).expect("front");
        let _ = take();
        let (bytes, _) = crate::emit_program_with_ops(&ir).expect("emits");
        assert!(take_for(&bytes).is_none());
    }
}
