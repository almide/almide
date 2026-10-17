//! The direct components' bump allocator and its RESERVATION discipline
//! (#2119) — shared verbatim by the p2 and p3 shims.
//!
//! `cabi_realloc` is the canonical ABI's landing zone: the host calls it
//! to lower a produced value (a read's bytes, an entropy list, a preopen
//! path) into guest memory. It bumps the module's own `__heap` frontier
//! (8-aligned raw bytes, never freed — the block allocator's free lists
//! stay undisturbed) and grows memory on demand.
//!
//! ## Why the failure path cannot speak
//!
//! C-197's form is `Error: out of memory` on stderr plus exit 1, never a
//! raw trap (T6). Writing that line needs a host call, and a host call
//! from `cabi_realloc` is INADMISSIBLE: the canonical ABI runs realloc
//! inside the lowering window, where the component is forbidden from
//! calling imports. Wasmtime states it outright —
//! `wasmtime/src/runtime/component/func.rs`: "while this is running the
//! component is forbidden from calling imports", `set_may_leave(false)`
//! around every lower — and enforces it in the lowered-import trampoline
//! (`check_may_leave`, `Trampoline::LowerImport` ⇒ trap
//! `CannotLeaveComponent`). The shim used to call `exit` from there, so a
//! host-driven OOM surfaced as `wasm trap: cannot leave component
//! instance` and exit 134: a raw trap that blames the component's
//! structure for a memory shortage.
//!
//! ## The discipline that replaces it
//!
//! Every allocation the ABI will demand is RESERVED first, in ordinary
//! guest code, where the abort is admissible:
//!
//!   - before an import whose result lands via `cabi_realloc`, the shim
//!     calls `$reserve` with the bound it itself chose for that call (the
//!     read length, the entropy length) — the host cannot answer with
//!     more than it was asked for, so the reservation is EXACT;
//!   - guest-side allocations (p3's read-to-end buffers, the fan slot
//!     table, the http body buffer) go through `$realloc_checked`, which
//!     is `$reserve` followed by the same bump.
//!
//! `$reserve` grows memory, and when the grow fails it writes C-197's
//! line and exits 1 — the defined abort, identical to `$alloc`'s. After
//! it returns, `cabi_realloc` has the room and cannot need a grow, which
//! is what makes its own failure branch dead for every op the shims
//! serve. That branch is a bare `unreachable`: the canonical ABI leaves
//! no channel for a diagnosis there, and every other guest in the
//! ecosystem ends the same way (wit-bindgen's `cabi_realloc` calls Rust's
//! `handle_alloc_error`, which aborts).

use wasm_encoder::{BlockType, Function, ValType};

/// `(need) -> ()`: guarantee `need` bytes of bump room past the frontier,
/// growing memory when it is short. A refused grow is the DEFINED abort —
/// C-197's line on stderr, exit 1 — emitted HERE, in guest control flow,
/// because the canonical ABI forbids it from `cabi_realloc` itself.
///
/// `msg` is the park offset of the message text and `msg_len` its length
/// WITHOUT the trailing newline: the eprintln shim appends one.
pub(crate) fn shim_reserve(
    heap_global: u32,
    f_eprintln: u32,
    i_exit: u32,
    msg: u32,
    msg_len: usize,
) -> Function {
    let need = 0u32;
    let head = 1u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    // head = the 8-aligned frontier cabi_realloc would hand out. The
    // frontier never passes the memory end and the end is page-aligned,
    // so the aligned frontier never passes it either — the subtraction
    // below cannot underflow, which is why the room test is written as
    // `need > end - head` rather than an overflow-prone `head + need`.
    i.global_get(heap_global).i32_const(7).i32_add().i32_const(-8).i32_and();
    i.local_set(head);
    i.local_get(need);
    i.memory_size(0).i32_const(16).i32_shl().local_get(head).i32_sub();
    i.i32_gt_u();
    i.if_(BlockType::Empty);
    i.local_get(need).i32_const(0xFFFF).i32_add().i32_const(16).i32_shr_u();
    i.memory_grow(0);
    i.i32_const(-1).i32_eq();
    i.if_(BlockType::Empty);
    i.i32_const(msg as i32).i32_const(msg_len as i32).call(f_eprintln);
    i.i32_const(1).call(i_exit);
    i.unreachable();
    i.end();
    i.end();
    i.end();
    f
}

/// `(old_ptr, old_size, align, new_size) -> ptr`: the shims' own
/// allocator — `$reserve` then the bump. Guest-side callers use THIS and
/// never `cabi_realloc` directly, so a shortage they hit reports C-197
/// instead of reaching the ABI export's mute branch.
pub(crate) fn shim_realloc_checked(f_reserve: u32, f_realloc: u32) -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(3).call(f_reserve);
    i.local_get(0).local_get(1).local_get(2).local_get(3);
    i.call(f_realloc);
    i.end();
    f
}

/// `cabi_realloc(old_ptr, old_size, align, new_size) -> ptr`: bump the
/// module's own `__heap` frontier (8-aligned raw bytes, never freed),
/// growing memory on demand. The grow is a fallback only — every caller
/// reserves first (see the module header) — and its failure branch has no
/// admissible way to speak, so it ends the instance rather than lying.
pub(crate) fn shim_cabi_realloc(heap_global: u32) -> Function {
    let (old_ptr, old_size, _align, new_size) = (0u32, 1u32, 2u32, 3u32);
    let result = 4u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    // result = (heap + 7) & !7 — or, for the frontier block (the last one
    // bumped, ending exactly at `heap`), the block itself: it grows (or
    // shrinks) where it stands and nothing is copied. A read-to-end drain
    // doubles one buffer with nothing allocated between two doublings, so
    // copying it cost up to the body's size again in steps at each doubling.
    i.local_get(old_ptr);
    i.global_get(heap_global).i32_const(7).i32_add().i32_const(-8).i32_and();
    i.local_get(old_ptr).i32_const(0).i32_ne();
    i.local_get(old_ptr).local_get(old_size).i32_add().global_get(heap_global).i32_eq();
    i.i32_and();
    i.select();
    i.local_set(result);
    // grow if result + new_size overruns memory
    i.local_get(result).local_get(new_size).i32_add();
    i.memory_size(0).i32_const(16).i32_shl();
    i.i32_gt_u();
    i.if_(BlockType::Empty);
    i.local_get(new_size).i32_const(0xFFFF).i32_add().i32_const(16).i32_shr_u();
    i.memory_grow(0);
    i.i32_const(-1).i32_eq();
    i.if_(BlockType::Empty);
    // UNREACHABLE BY THE RESERVATION DISCIPLINE. A host call here traps
    // `CannotLeaveComponent` (the canonical ABI's lowering window), so
    // there is no message to write and no exit code to set; the
    // alternative — answering with a pointer memory cannot back — would
    // corrupt the guest silently.
    i.unreachable();
    i.end();
    i.end();
    i.local_get(result).local_get(new_size).i32_add().global_set(heap_global);
    // realloc semantics: copy min(old_size, new_size) from old_ptr (a block
    // grown in place is already there).
    i.local_get(old_ptr).i32_const(0).i32_ne();
    i.local_get(old_ptr).local_get(result).i32_ne();
    i.i32_and();
    i.if_(BlockType::Empty);
    i.local_get(result);
    i.local_get(old_ptr);
    i.local_get(old_size).local_get(new_size).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32));
    i.local_get(old_size);
    i.else_();
    i.local_get(new_size);
    i.end();
    i.memory_copy(0, 0);
    i.end();
    i.local_get(result);
    i.end();
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_encoder::{
        CodeSection, ConstExpr, ExportKind, ExportSection, FunctionSection, GlobalSection,
        GlobalType, MemorySection, MemoryType, Module, TypeSection,
    };

    /// A module with one page of memory, `__heap` at 1024, and `cabi_realloc`
    /// exported, run on wasmtime.
    fn instance() -> (
        wasmtime::Store<()>,
        wasmtime::TypedFunc<(i32, i32, i32, i32), i32>,
        wasmtime::Memory,
    ) {
        let mut m = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([ValType::I32; 4], [ValType::I32]);
        m.section(&types);
        let mut funcs = FunctionSection::new();
        funcs.function(0);
        m.section(&funcs);
        let mut mems = MemorySection::new();
        mems.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        m.section(&mems);
        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(1024),
        );
        m.section(&globals);
        let mut exports = ExportSection::new();
        exports.export("cabi_realloc", ExportKind::Func, 0);
        exports.export("memory", ExportKind::Memory, 0);
        m.section(&exports);
        let mut code = CodeSection::new();
        code.function(&shim_cabi_realloc(0));
        m.section(&code);
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, m.finish()).expect("module");
        let mut store = wasmtime::Store::new(&engine, ());
        let inst = wasmtime::Instance::new(&mut store, &module, &[]).expect("instance");
        let f = inst
            .get_typed_func(&mut store, "cabi_realloc")
            .expect("realloc");
        let mem = inst.get_memory(&mut store, "memory").expect("memory");
        (store, f, mem)
    }

    /// The read-to-end drains (p3 http bodies, fs reads) double one buffer
    /// with this realloc and allocate nothing between two doublings, so the
    /// buffer is always the frontier block. Copying it anyway made a body of
    /// n bytes cost up to 2n bytes of copies in steps at every doubling: wasm
    /// http.get_response's bytes moved read 7.5-7.9 per body byte while the
    /// body fit the first 64 KiB and 8.0-8.5 past it (GCE c4-standard-4), a
    /// series no class explains.
    /// The frontier block grows where it stands.
    #[test]
    fn the_frontier_block_grows_in_place() {
        let (mut store, realloc, mem) = instance();
        let p = realloc.call(&mut store, (0, 0, 8, 64)).unwrap();
        mem.data_mut(&mut store)[p as usize..p as usize + 4].copy_from_slice(b"abcd");
        let q = realloc.call(&mut store, (p, 64, 8, 128 * 1024)).unwrap();
        assert_eq!(q, p, "the frontier block moved");
        assert_eq!(&mem.data(&store)[q as usize..q as usize + 4], b"abcd");
        // The next block starts past the grown one.
        let r = realloc.call(&mut store, (0, 0, 8, 16)).unwrap();
        assert!(r as usize >= q as usize + 128 * 1024);
    }

    /// A block below the frontier still moves, with its bytes.
    #[test]
    fn a_block_below_the_frontier_moves_with_its_bytes() {
        let (mut store, realloc, mem) = instance();
        let p = realloc.call(&mut store, (0, 0, 8, 64)).unwrap();
        mem.data_mut(&mut store)[p as usize..p as usize + 4].copy_from_slice(b"wxyz");
        let _later = realloc.call(&mut store, (0, 0, 8, 16)).unwrap();
        let q = realloc.call(&mut store, (p, 64, 8, 256)).unwrap();
        assert_ne!(q, p);
        assert_eq!(&mem.data(&store)[q as usize..q as usize + 4], b"wxyz");
    }
}
