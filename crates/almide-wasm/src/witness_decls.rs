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
    /// What each stub's extern declares, parallel to `stubs`: a plain `fn`
    /// extern's import reaches FOREIGN only, an `effect fn` extern's also
    /// FOREIGN_EFFECT (cert_project.rs).
    pub stub_declared: Vec<Declared>,
    /// `(creator, closure)`: each lifted lambda that is a funcref-table entry,
    /// at its pre-declare index, beside the body that builds it. The
    /// capability witness charges a closure's reach to its creator, not to a
    /// fn that only calls it as an argument (cert_project.rs).
    pub creations: Vec<(u32, u32)>,
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
    fn an_extern_stub_is_renumbered_and_a_plain_caller_of_a_pure_one_records_foreign_within_its_bound() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let src = "@extern(wasm, \"js\", \"js_add\")\nfn js_add(a: Int, b: Int) -> Int\n\nfn sum(a: Int) -> Int = js_add(a, 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(sum(2)))\n}\n";
        let (_, d) = emit_recorded("extern.almd", src);
        assert_eq!(d.stubs.len(), 1, "one @extern stub: {:?}", d.stubs);
        assert_eq!(d.stub_declared, [Declared::Pure], "js_add is a plain fn extern");
        // The stub became an import: its name no longer lands on a body.
        let named = cert_project::decl_names(&d).expect("decl names");
        let imports = cert_project::function_imports(&d.bytes).expect("imports");
        let stub_at = named.iter().find(|(_, n)| *n == "js_add").map(|(i, _)| *i).expect("js_add renumbered");
        assert!(stub_at < imports, "js_add is an import now (index {stub_at} of {imports})");
        // A plain fn reaching a pure-declared foreign function: FOREIGN is
        // recorded, never hidden, and it is inside the plain fn's bound —
        // the checker admits the call, and so does the certificate.
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        let sum = flat.iter().find(|(n, _)| n == "sum").expect("sum's caps witness");
        let reach: Vec<&str> = sum.1.split('|').nth(1).unwrap_or("").split(' ').collect();
        assert!(reach.contains(&cap::FOREIGN.to_string().as_str()), "{sum:?}");
        assert!(!reach.contains(&cap::FOREIGN_EFFECT.to_string().as_str()), "{sum:?}");
        assert!(accepts(almide_verify::Property::Caps, &sum.1), "{sum:?}");
        assert!(accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
    }

    /// The negative control: an `effect fn` extern carries FOREIGN_EFFECT,
    /// which no plain fn's bound admits. The checker refuses a plain fn
    /// calling one (it is an effect call), so the control re-declares the
    /// effect caller as plain in the table — the producer bug the certificate
    /// exists to catch.
    #[test]
    fn a_plain_caller_of_an_effect_extern_is_refused() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let src = "@extern(wasm, \"js\", \"js_load\")\neffect fn js_load(a: Int) -> Int\n\neffect fn sum(a: Int) -> Int = js_load(a)!\n\neffect fn main() -> Unit = {\n  println(int.to_string(sum(2)!))\n}\n";
        let (_, mut d) = emit_recorded("extern_effect.almd", src);
        assert_eq!(d.stub_declared, [Declared::Effect], "js_load is an effect fn extern");
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        let sum = flat.iter().find(|(n, _)| n == "sum").expect("sum's caps witness");
        let reach: Vec<&str> = sum.1.split('|').nth(1).unwrap_or("").split(' ').collect();
        assert!(reach.contains(&cap::FOREIGN.to_string().as_str()), "{sum:?}");
        assert!(reach.contains(&cap::FOREIGN_EFFECT.to_string().as_str()), "{sum:?}");
        assert!(accepts(almide_verify::Property::Caps, &sum.1), "an effect fn admits it: {sum:?}");
        assert!(accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
        d.fns.iter_mut().filter(|f| f.name == "sum").for_each(|f| f.declared = Some(Declared::Pure));
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        let sum = flat.iter().find(|(n, _)| n == "sum").expect("sum's caps witness");
        assert!(!accepts(almide_verify::Property::Caps, &sum.1), "{sum:?}");
        assert!(!accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
    }

    /// `relay` and `use_it` only call the closure they are handed; `make`
    /// builds it and reads the file inside it.
    const CLOSURE_ARG: &str = "import fs\n\neffect fn make() -> (String) -> String = (p) => fs.read_text(p) ?? \"\"\n\nfn use_it(f: (String) -> String) -> String = f(\"notes.txt\")\n\nfn relay(f: (String) -> String) -> String = use_it(f)\n\neffect fn main() -> Unit = {\n  let f = make()!\n  println(relay(f))\n}\n";

    fn reach_of(flat: &[(String, String)], name: &str) -> Vec<u32> {
        let w = &flat.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name}'s caps witness: {flat:?}")).1;
        w.trim().split('|').nth(1).unwrap_or("").split(' ').filter_map(|c| c.parse().ok()).collect()
    }

    /// Effect polymorphism: a closure argument's capabilities are its
    /// creator's. Two plain fns pass and call an FS-reading closure and are
    /// accepted, each reporting the PARAMETRIC reach rather than nothing;
    /// the creator `make` is charged the read.
    #[test]
    fn a_closure_argument_is_charged_to_its_creator_and_its_callers_reach_parametric() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let (_, d) = emit_recorded("closure_arg.almd", CLOSURE_ARG);
        assert!(!d.creations.is_empty(), "make's lambda is recorded as make's: {:?}", d.creations);
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        for f in ["use_it", "relay"] {
            let reach = reach_of(&flat, f);
            assert!(!reach.contains(&cap::FS_READ), "{f} is not charged its argument's read: {reach:?}");
            if f == "use_it" {
                assert!(reach.contains(&cap::PARAMETRIC), "use_it reports the parametric reach: {reach:?}");
            }
        }
        assert!(reach_of(&flat, "make").contains(&cap::FS_READ), "the creator is charged: {flat:?}");
        assert!(flat.iter().all(|(_, w)| accepts(almide_verify::Property::Caps, w)), "{flat:?}");
        assert!(accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
    }

    /// The negative controls. The checker refuses both shapes in source, so
    /// each re-declares an effect fn plain in the table: the producer bug the
    /// certificate exists to catch. (i) A plain fn whose OWN body reads a
    /// file; (ii) a plain fn that creates the FS-reading closure.
    #[test]
    fn a_plain_reader_and_a_plain_creator_of_a_reading_closure_are_refused() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let own = "import fs\n\neffect fn read_it(p: String) -> String = fs.read_text(p) ?? \"\"\n\neffect fn main() -> Unit = {\n  println(read_it(\"x\")!)\n}\n";
        for (src, plain) in [(own, "read_it"), (CLOSURE_ARG, "make")] {
            let (_, mut d) = emit_recorded("closure_neg.almd", src);
            let (flat, graph) = cert_project::caps(&d).expect("caps");
            assert!(accepts(almide_verify::Property::CapsTransitive, &graph), "the control is accepted as declared: {graph}");
            assert!(flat.iter().all(|(_, w)| accepts(almide_verify::Property::Caps, w)), "{flat:?}");
            d.fns.iter_mut().filter(|f| f.name == plain).for_each(|f| f.declared = Some(Declared::Pure));
            let (flat, graph) = cert_project::caps(&d).expect("caps");
            let w = &flat.iter().find(|(n, _)| n == plain).expect("the re-declared fn's witness").1;
            assert!(!accepts(almide_verify::Property::Caps, w), "{plain} declared plain must be refused: {w:?}");
            assert!(!accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
        }
    }

    /// A plain `main` calling the pure intrinsic env.os is admitted: its op
    /// reaches nothing. The control: an effect intrinsic (env.cwd) in a fn
    /// re-declared plain is still refused.
    #[test]
    fn a_pure_intrinsic_is_admitted_in_a_plain_fn_and_an_effect_one_is_not() {
        let _g = ARMED.lock().unwrap_or_else(|e| e.into_inner());
        let (_, d) = emit_recorded("env_os_pure.almd", "fn main() -> Unit = {\n  let _ = env.os()\n}\n");
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        assert!(flat.iter().all(|(_, w)| accepts(almide_verify::Property::Caps, w)), "{flat:?}");
        assert!(accepts(almide_verify::Property::CapsTransitive, &graph), "{graph}");
        let (_, mut d) = emit_recorded("env_cwd.almd", "effect fn here() -> String = env.cwd() ?? \"\"\n\neffect fn main() -> Unit = {\n  println(here()!)\n}\n");
        d.fns.iter_mut().filter(|f| f.name == "here").for_each(|f| f.declared = Some(Declared::Pure));
        let (flat, graph) = cert_project::caps(&d).expect("caps");
        let w = &flat.iter().find(|(n, _)| n == "here").expect("here's witness").1;
        assert!(!accepts(almide_verify::Property::Caps, w), "an env read in a plain fn must be refused: {w:?}");
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
