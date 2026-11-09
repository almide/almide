//! ADR-0033 D1: the rc redirect, checked by RUNNING the shipped runtime
//! bodies (not a Rust model of them) over hand-built memory: one family root
//! (an ordinary `Bytes` block) carved with an interior String and an
//! interior List[String] holding it, in a minimal module with `$alloc`,
//! `$free`, `$inc`, `$dec_flat`, `$cow`, `$block_copy`, `$copy` at their
//! fixed indices and the typed helpers after them.
//!
//! Also pinned here: the family variants of every rc routine are the plain
//! bodies with ONE splice right after the static floor guard (the retarget
//! for `$inc` / `$dec_flat`, the forward for every other dec) — so a module
//! that links no producer emits the plain bodies, byte for byte.

use wasm_encoder::{
    CodeSection, ConstExpr, Encode as _, ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType,
    MemorySection, MemoryType, Module, TypeSection, ValType,
};

use super::{FamilyGuard, INTERIOR};

const HEAP0: i32 = 4096;
/// Below this an address is a static (the guard no-ops it).
const LINE_END: i32 = 4096;
const RC: u32 = almide_layout::RC.offset;
const LEN: u32 = almide_layout::LEN.offset;
const CAP: u32 = almide_layout::CAP.offset;
const PAYLOAD: u32 = almide_layout::PAYLOAD;

// The typed helpers, after the fixed slots.
const F_DROP_LIST_STR: u32 = crate::F_COW + 1;
const F_DROP_TUPLE_STR: u32 = crate::F_COW + 2;
const F_INC_ELEMS: u32 = crate::F_COW + 3;
const F_COW_ELEMS: u32 = crate::F_COW + 4;

fn module_bytes(family: bool) -> Vec<u8> {
    let _g = FamilyGuard::set(family);
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], []); // 0: (i32) -> ()
    types.ty().function([ValType::I32], [ValType::I32]); // 1: (i32) -> i32
    types.ty().function([], []); // 2: () -> ()
    types.ty().function([ValType::I32; 3], []); // 3: $copy
    let ty_of = |k: u32| match k {
        k if k == crate::F_ALLOC || k == crate::F_BLOCK_COPY || k == crate::F_COW || k == F_COW_ELEMS => 1,
        k if k == crate::F_COPY => 3,
        k if [crate::F_FREE, crate::F_INC, crate::F_DEC_FLAT, crate::F_EXIT_IMPORT, crate::F_EPRINTLN_BLOCK].contains(&k)
            || (F_DROP_LIST_STR..=F_INC_ELEMS).contains(&k) =>
        {
            0
        }
        _ => 2,
    };
    let mut funcs = FunctionSection::new();
    let mut code = CodeSection::new();
    for k in 0..=F_COW_ELEMS {
        funcs.function(ty_of(k));
        let body = match k {
            k if k == crate::F_ALLOC => crate::runtime_alloc::emit_alloc(0, None, false, false),
            k if k == crate::F_FREE => crate::runtime_alloc::emit_free(None, false),
            k if k == crate::F_INC => crate::runtime_alloc::emit_inc(),
            k if k == crate::F_DEC_FLAT => crate::runtime_alloc::emit_dec_flat(),
            k if k == crate::F_COW => crate::runtime_alloc::emit_cow().0,
            k if k == crate::F_BLOCK_COPY => crate::emit_block_copy(),
            k if k == crate::F_COPY => crate::emit_copy(),
            F_DROP_LIST_STR => crate::runtime_alloc::emit_drop_list(crate::F_DEC_FLAT),
            F_DROP_TUPLE_STR => crate::runtime_alloc::emit_drop_shape(&[(0, crate::F_DEC_FLAT)], None),
            F_INC_ELEMS => crate::runtime_alloc::emit_inc_elems(),
            F_COW_ELEMS => crate::runtime_alloc::emit_cow_elems(F_INC_ELEMS).0,
            _ => {
                let mut f = Function::new([]);
                if ty_of(k) == 0 {
                    f.instructions().unreachable();
                }
                f.instructions().end();
                f
            }
        };
        code.function(&body);
    }
    let mut mems = MemorySection::new();
    mems.memory(MemoryType { minimum: 1, maximum: None, memory64: false, shared: false, page_size_log2: None });
    let mut globals = GlobalSection::new();
    for g in 0..=crate::G_MAPIDX {
        let v = match g {
            g if g == crate::G_HEAP => HEAP0,
            g if g == crate::G_LINE_END => LINE_END,
            _ => 0,
        };
        globals.global(GlobalType { val_type: ValType::I32, mutable: true, shared: false }, &ConstExpr::i32_const(v));
    }
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    for (name, f) in [
        ("alloc", crate::F_ALLOC),
        ("inc", crate::F_INC),
        ("dec_flat", crate::F_DEC_FLAT),
        ("cow", crate::F_COW),
        ("drop_list", F_DROP_LIST_STR),
        ("drop_tuple", F_DROP_TUPLE_STR),
        ("cow_elems", F_COW_ELEMS),
    ] {
        exports.export(name, ExportKind::Func, f);
    }
    let mut m = Module::new();
    m.section(&types).section(&funcs).section(&mems).section(&globals).section(&exports).section(&code);
    m.finish()
}

struct Rt {
    store: wasmtime::Store<()>,
    inst: wasmtime::Instance,
    mem: wasmtime::Memory,
}

impl Rt {
    fn new(family: bool) -> Self {
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, module_bytes(family)).expect("the minimal module validates");
        let mut store = wasmtime::Store::new(&engine, ());
        let inst = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let mem = inst.get_memory(&mut store, "memory").expect("memory");
        Rt { store, inst, mem }
    }
    fn call1(&mut self, name: &str, b: u32) -> u32 {
        let f = self.inst.get_typed_func::<i32, i32>(&mut self.store, name).expect(name);
        f.call(&mut self.store, b as i32).expect(name) as u32
    }
    /// `Err` = the call trapped.
    fn call0(&mut self, name: &str, b: u32) -> Result<(), wasmtime::Error> {
        let f = self.inst.get_typed_func::<i32, ()>(&mut self.store, name).expect(name);
        f.call(&mut self.store, b as i32)
    }
    fn run(&mut self, name: &str, b: u32) {
        self.call0(name, b).unwrap_or_else(|e| panic!("{name}({b}) trapped: {e:?}"));
    }
    fn word(&self, a: u32) -> u32 {
        u32::from_le_bytes(self.mem.data(&self.store)[a as usize..][..4].try_into().expect("a word"))
    }
    fn set(&mut self, a: u32, v: u32) {
        self.mem.data_mut(&mut self.store)[a as usize..][..4].copy_from_slice(&v.to_le_bytes());
    }
    fn header(&mut self, b: u32, rc: u32, len: u32, cap: u32) {
        self.set(b + RC, rc);
        self.set(b + LEN, len);
        self.set(b + CAP, cap);
    }
}

/// A family: root (an ordinary block, rc 1), its payload's first word left
/// for the free list's link, then an interior String "abc" and an interior
/// List[String] holding it. Returns (root, s, l).
fn family(rt: &mut Rt) -> (u32, u32, u32) {
    let root = rt.call1("alloc", 64);
    assert_eq!(rt.word(root + RC), 1);
    let s = root + PAYLOAD + 16;
    rt.header(s, INTERIOR | (s - root), 3, 4);
    rt.mem.data_mut(&mut rt.store)[(s + PAYLOAD) as usize..][..3].copy_from_slice(b"abc");
    let l = s + PAYLOAD + 4;
    rt.header(l, INTERIOR | (l - root), 4, 4);
    rt.set(l + PAYLOAD, s);
    (root, s, l)
}

#[test]
fn inc_and_dec_of_an_interior_block_credit_the_root() {
    let mut rt = Rt::new(true);
    let (root, s, l) = family(&mut rt);
    let (sw, lw) = (rt.word(s), rt.word(l));
    rt.run("inc", s);
    rt.run("inc", l);
    assert_eq!(rt.word(root), 3, "two interior incs land on the root");
    assert_eq!((rt.word(s), rt.word(l)), (sw, lw), "an interior rc word is never written");
    rt.run("dec_flat", s);
    rt.run("dec_flat", l);
    assert_eq!(rt.word(root), 1, "two interior decs land on the root");
    assert_eq!((rt.word(s), rt.word(l)), (sw, lw));
}

#[test]
fn the_family_is_freed_once_at_root_count_zero_and_children_are_not_walked() {
    let mut rt = Rt::new(true);
    let (root, s, l) = family(&mut rt);
    let sw = rt.word(s);
    // The list's typed drop forwards to the family $dec_flat: the root goes
    // to 0 and is freed; the list's element (an interior String) is NOT
    // released — releasing it would redirect to the freed root and trap.
    rt.run("drop_list", l);
    assert_eq!(rt.word(root), 0, "the root reached 0");
    assert_eq!(rt.word(s), sw, "the interior child was not walked");
    // Freed exactly once: the next same-class request pops the root.
    assert_eq!(rt.call1("alloc", 64), root, "the root block was filed on the free list");
}

#[test]
fn every_typed_drop_forwards_an_interior_block() {
    let mut rt = Rt::new(true);
    let (root, s, _) = family(&mut rt);
    // A tuple block (String) carved inside the family too.
    let t = s + 64;
    rt.header(t, INTERIOR | (t - root), 4, 4);
    rt.set(t + PAYLOAD, s);
    rt.run("inc", t);
    assert_eq!(rt.word(root), 2);
    rt.run("drop_tuple", t);
    assert_eq!(rt.word(root), 1, "the shape drop released the root once and did not walk its slot");
}

#[test]
fn the_double_free_trap_still_fires_through_the_redirect() {
    let mut rt = Rt::new(true);
    let (root, s, _) = family(&mut rt);
    rt.run("dec_flat", s);
    assert_eq!(rt.word(root), 0, "freed");
    assert!(rt.call0("dec_flat", s).is_err(), "an interior dec on a freed family traps");
    assert!(rt.call0("dec_flat", root).is_err(), "a root dec on a freed family traps");
    // And the ordinary path, unchanged.
    let b = rt.call1("alloc", 8);
    rt.run("dec_flat", b);
    assert!(rt.call0("dec_flat", b).is_err(), "an ordinary double free traps");
}

#[test]
fn statics_are_still_untouched() {
    let mut rt = Rt::new(true);
    let st = 2048;
    rt.header(st, INTERIOR | 16, 0, 0);
    rt.run("inc", st);
    rt.run("dec_flat", st);
    rt.run("drop_list", st);
    assert_eq!(rt.word(st), INTERIOR | 16, "the floor guard runs before the rc word is read");
}

#[test]
fn every_cow_judge_copies_an_interior_block() {
    let mut rt = Rt::new(true);
    let (root, s, _) = family(&mut rt);
    rt.run("inc", s); // the caller's credit for the String it hands $cow
    assert_eq!(rt.word(root), 2);
    let c = rt.call1("cow", s);
    assert_ne!(c, s, "an interior String is copied before a mutation");
    assert_eq!((rt.word(c + RC), rt.word(c + LEN)), (1, 3), "the copy is an ordinary unique block");
    assert_eq!(&rt.mem.data(&rt.store)[(c + PAYLOAD) as usize..][..3], b"abc");
    assert_eq!(rt.word(root), 1, "$cow released the caller's credit through the root");
}

#[test]
fn cow_elems_takes_the_copy_credits_before_it_releases_the_family() {
    let mut rt = Rt::new(true);
    let (root, s, l) = family(&mut rt);
    // The family's one reference is the credit handed to $cow_elems.
    let c = rt.call1("cow_elems", l);
    assert_ne!(c, l, "an interior list is copied");
    assert_eq!(rt.word(c + PAYLOAD), s, "the copy's element is the interior String");
    assert_eq!(rt.word(root), 1, "the copy's element credit keeps the family alive");
    assert_eq!(rt.word(c + RC), 1);
    // Never freed meanwhile: a same-class request must not be handed the root
    // (a release before the credits would have filed it on the free list).
    assert_ne!(rt.call1("alloc", 64), root, "the family was freed while the copy still holds its element");
    // Releasing the copy releases its element, which frees the family.
    rt.run("drop_list", c);
    assert_eq!(rt.word(root), 0, "the family freed at the last release");
}

#[test]
fn an_ordinary_unique_block_still_passes_cow_in_place() {
    let mut rt = Rt::new(true);
    let b = rt.call1("alloc", 8);
    assert_eq!(rt.call1("cow", b), b);
    let l = rt.call1("alloc", 4);
    rt.set(l + PAYLOAD, 0);
    assert_eq!(rt.call1("cow_elems", l), l);
}

/// The plain module runs too (the non-family routines are unchanged).
#[test]
fn the_plain_module_validates_and_runs() {
    let mut rt = Rt::new(false);
    let b = rt.call1("alloc", 8);
    rt.run("inc", b);
    rt.run("dec_flat", b);
    rt.run("dec_flat", b);
    assert!(rt.call0("dec_flat", b).is_err());
}

fn raw(f: &Function) -> Vec<u8> {
    let mut cs = CodeSection::new();
    cs.function(f);
    let mut out = Vec::new();
    cs.encode(&mut out);
    out
}

/// The operator bytes of a body (the code section's count + size prefix
/// and the locals vector stripped) — the splice is located in these.
fn ops(f: &Function) -> Vec<u8> {
    let b = raw(f);
    let leb = |p: &mut usize| {
        while b[*p] & 0x80 != 0 {
            *p += 1;
        }
        *p += 1;
    };
    // section size, function count, body size, then the locals vector.
    let mut p = 0;
    leb(&mut p);
    leb(&mut p);
    leb(&mut p);
    let nlocals = b[p];
    assert!(nlocals < 0x80);
    p += 1;
    for _ in 0..nlocals {
        leb(&mut p);
        p += 1; // the valtype
    }
    b[p..].to_vec()
}

/// local.get 0; global.get LINE_END; i32.lt_u; if; return; end
fn guard() -> Vec<u8> {
    vec![0x20, 0, 0x23, crate::G_LINE_END as u8, 0x49, 0x04, 0x40, 0x0f, 0x0b]
}

/// The retarget (W2's decoder shape): `if (load(b) <s 0) b = b - (load(b) & 0x7fffffff)`.
pub(super) fn retarget() -> Vec<u8> {
    vec![
        0x20, 0, 0x28, 2, 0, 0x41, 0, 0x48, 0x04, 0x40, // local.get 0; i32.load; i32.const 0; i32.lt_s; if
        0x20, 0, 0x20, 0, 0x28, 2, 0, // local.get 0; local.get 0; i32.load
        0x41, 0xff, 0xff, 0xff, 0xff, 0x07, 0x71, 0x6b, // i32.const 0x7fffffff; i32.and; i32.sub
        0x21, 0, 0x0b, // local.set 0; end
    ]
}

/// The forward: `if (load(b) <s 0) { $dec_flat(b); return }`.
fn forward() -> Vec<u8> {
    vec![
        0x20, 0, 0x28, 2, 0, 0x41, 0, 0x48, 0x04, 0x40, // local.get 0; i32.load; i32.const 0; i32.lt_s; if
        0x20, 0, 0x10, crate::F_DEC_FLAT as u8, 0x0f, 0x0b, // local.get 0; call $dec_flat; return; end
    ]
}

fn spliced(plain: &[u8], splice: &[u8]) -> Vec<u8> {
    let g = guard();
    assert_eq!(&plain[..g.len()], &g[..], "the body opens with the static floor guard");
    [&plain[..g.len()], splice, &plain[g.len()..]].concat()
}

/// Every rc routine: family = plain + one splice after the guard; plain =
/// exactly the body built with the switch off (so a module linking no
/// producer is byte-identical to one built before families existed: the
/// plain `$inc` / `$dec_flat` / `$free` / `$alloc` trees stay pinned by
/// `runtime_trees_match_the_proof_transcription`).
#[test]
fn family_variants_are_the_plain_bodies_with_one_splice() {
    assert_family_trees();
}

/// Also called by `runtime_trees_match_the_proof_transcription`: the
/// family `$inc` / `$dec_flat` trees proofs/StructuralRuntime.v transcribes
/// are pinned as the hash-pinned plain trees plus the exact retarget bytes.
pub(crate) fn assert_family_trees() {
    use crate::runtime_alloc as ra;
    type Build = Box<dyn Fn() -> Function>;
    let cases: Vec<(&str, Build, Vec<u8>)> = vec![
        ("inc", Box::new(ra::emit_inc), retarget()),
        ("dec_flat", Box::new(ra::emit_dec_flat), retarget()),
        ("drop_list", Box::new(|| ra::emit_drop_list(crate::F_DEC_FLAT)), forward()),
        ("drop_shape", Box::new(|| ra::emit_drop_shape(&[(0, crate::F_DEC_FLAT)], Some((4, vec![(0, vec![(8, crate::F_DEC_FLAT)])])))), forward()),
        ("drop_http_call", Box::new(ra::emit_drop_http_call), forward()),
        ("drop_map_spine", Box::new(|| ra::emit_drop_map_spine(40)), forward()),
        ("drop_entries", Box::new(|| ra::emit_drop_entries(8, [Some((0, crate::F_DEC_FLAT)), None], 40)), forward()),
        ("drop_shape_deep", Box::new(|| crate::deep_walk::emit_drop_shape_deep(&[(0, crate::F_DEC_FLAT, 0)], None, (0, 0), (41, 42))), forward()),
    ];
    for (name, build, splice) in cases {
        let plain = {
            let _g = FamilyGuard::set(false);
            ops(&build())
        };
        let fam = {
            let _g = FamilyGuard::set(true);
            ops(&build())
        };
        assert_eq!(fam, spliced(&plain, &splice), "{name}: the family body is the plain body with one splice after the guard");
        assert!(!plain.windows(5).any(|w| w == [0x41, 0, 0x48, 0x04, 0x40]), "{name}: the plain body carries no sign test");
    }
}

/// The switch reads the producer declaration, never a name.
#[test]
fn the_switch_follows_the_declaration() {
    use almide_base::intern::sym;
    let attr = |n: &str| almide_lang::ast::Attribute { name: sym(n), args: vec![], span: None };
    let plain = vec![attr("pure")];
    assert!(!super::producer_linked([plain.as_slice()]), "another attribute arms nothing");
    assert!(!super::producer_linked(std::iter::empty::<&[almide_lang::ast::Attribute]>()));
    let producer = vec![attr("pure"), attr(super::PRODUCER_ATTR)];
    assert!(super::producer_linked([plain.as_slice(), producer.as_slice()]));
    assert!(!super::armed(), "the switch is off unless a pass arms it");
}
