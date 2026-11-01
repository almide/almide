//! The region-window arena prelude (#1991) and its stale-handle trap (#2186).
//!
//! `AlmideRgn<T>` is the `Copy` handle a `__rgn_` twin enum's recursive
//! fields hold instead of `Box<T>`; behind it is a thread-local bump arena.
//! `almide_region_window(|| body)` saves the arena mark, runs the pair,
//! rewinds — every block allocated inside is released at once, no per-node
//! free. Nothing is dropped on rewind: every region type is `Copy` by
//! construction (`RegionWindowPass` admits only scalar / region-enum
//! payloads), so a rewound block owns nothing. Chunks are kept for the next
//! window (64 KiB, doubling to a 64 MiB cap). The handle holds a raw
//! pointer, so it is `!Send` / `!Sync` by construction and a window never
//! spans threads (`fan.*` bodies are outside the pure vocabulary). Lives in
//! the prelude, not the user code, so the rlib split
//! (`slim_main_with_external_runtime`) keeps it.
//!
//! The handle is a `NonNull`, so a twin enum's leaf case rides the null niche
//! instead of a tag word: `Leaf | Node(AlmideRgn, AlmideRgn)` is 16 bytes, not
//! 24, and the arena a window fills is a third smaller (#3586 — on x86-64 the
//! node stores were 46% of binarytrees' time, the leg's own wasm form beat it).
//!
//! The handle's `Deref` is `unsafe { self.0.as_ref() }` over memory the window has
//! rewound: rustc cannot see a handle that outlives its window, so a wrong
//! region-pure verdict in `pass_region_window.rs` is a SILENT use-after-rewind
//! — the slot still holds the old bytes until the next window overwrites it.
//! `ALMIDE_REGION_TRAP_STALE=1` (read at BUILD time) arms the trap: every block
//! gets an 8-byte generation header, the handle carries the generation it was
//! allocated under, a rewind poisons every header it releases and bumps the
//! generation, and every `Deref` compares the two — a stale read panics with a
//! message naming the pass instead of returning the old bytes. Nested windows
//! keep their outer handles live (only the released headers are poisoned) and
//! re-use of a rewound slot is caught by the generation (ABA). Off by default:
//! the shipped prelude is byte-identical to the one before the knob existed.
//! #4023 measured the armed prelude at +30% (binarytrees) and +52%
//! (treealloc) wall time, so instead of shipping it CI runs `spec/` and the
//! differential fuzz with it armed (`scripts/check-memory-traps.sh`).
//! `tests/region_window_trap_test.rs` pins both the trap and the silence, and
//! runs a region-shaped corpus armed on native against the wasm leg.

/// Is the stale-handle trap armed for this build?
pub(crate) fn region_trap_armed() -> bool {
    almide_base::env::flag("ALMIDE_REGION_TRAP_STALE")
}

/// The arena prelude as emitted into a program (`vis` is `pub ` for the
/// `almide_rt` rlib, empty for an inline main). `trap` selects the armed
/// variant; the shipped default passes `region_trap_armed()`.
pub(crate) fn region_arena_prelude(vis: &str, trap: bool) -> String {
    let mut s = String::new();
    push_handle(&mut s, vis, trap);
    push_arena(&mut s, vis, trap);
    s
}

/// The prelude as a standalone-program text (no `pub`), for tests that
/// compile it with rustc and drive the arena directly.
pub fn region_arena_prelude_source(trap: bool) -> String {
    region_arena_prelude("", trap)
}

/// `AlmideRgn<T>` and its by-value forwarding impls.
fn push_handle(s: &mut String, vis: &str, trap: bool) {
    if trap {
        s.push_str(&format!("{vis}struct AlmideRgn<T>({vis}std::ptr::NonNull<T>, {vis}u64);\n"));
        s.push_str(&format!("{vis}const ALMIDE_RGN_POISON: u64 = u64::MAX;\n"));
        // The cold half is out of line so the check itself stays a load and a compare.
        s.push_str(&format!("#[cold] #[inline(never)] {vis}fn almide_rgn_stale(stamp: u64, hdr: u64) -> ! {{ let slot = if hdr == ALMIDE_RGN_POISON {{ String::from(\"poisoned by a rewind\") }} else {{ format!(\"re-allocated under generation {{hdr}}\") }}; panic!(\"almide: region window trap (ALMIDE_REGION_TRAP_STALE): a region handle allocated under generation {{stamp}} was read after its window rewound the arena (slot {{slot}}); pass_region_window's region-pure verdict was wrong — report it\") }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_rgn_check(p: *const u8, stamp: u64) {{ let hdr = unsafe {{ *(p.sub(8) as *const u64) }}; if hdr != stamp {{ almide_rgn_stale(stamp, hdr) }} }}\n"));
    } else {
        s.push_str(&format!("{vis}struct AlmideRgn<T>({vis}std::ptr::NonNull<T>);\n"));
    }
    s.push_str("impl<T> Clone for AlmideRgn<T> { #[inline(always)] fn clone(&self) -> Self { *self } }\n");
    s.push_str("impl<T> Copy for AlmideRgn<T> {}\n");
    if trap {
        s.push_str("impl<T> std::ops::Deref for AlmideRgn<T> { type Target = T; #[inline(always)] fn deref(&self) -> &T { almide_rgn_check(self.0.as_ptr() as *const u8, self.1); unsafe { self.0.as_ref() } } }\n");
    } else {
        s.push_str("impl<T> std::ops::Deref for AlmideRgn<T> { type Target = T; #[inline(always)] fn deref(&self) -> &T { unsafe { self.0.as_ref() } } }\n");
    }
    s.push_str("impl<T: std::fmt::Debug> std::fmt::Debug for AlmideRgn<T> { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { (**self).fmt(f) } }\n");
    s.push_str("impl<T: PartialEq> PartialEq for AlmideRgn<T> { fn eq(&self, other: &Self) -> bool { **self == **other } }\n");
    // By-value forwarding of every derive a twin enum may carry from its
    // original (`has_ord` / `has_hash` templates).
    s.push_str("impl<T: Eq> Eq for AlmideRgn<T> {}\n");
    s.push_str("impl<T: PartialOrd> PartialOrd for AlmideRgn<T> { fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { (**self).partial_cmp(&**other) } }\n");
    s.push_str("impl<T: Ord> Ord for AlmideRgn<T> { fn cmp(&self, other: &Self) -> std::cmp::Ordering { (**self).cmp(&**other) } }\n");
    s.push_str("impl<T: std::hash::Hash> std::hash::Hash for AlmideRgn<T> { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { (**self).hash(state) } }\n");
}

/// The bump arena, its thread-local cell, and the alloc / save / restore /
/// window entry points.
fn push_arena(s: &mut String, vis: &str, trap: bool) {
    // `std::boxed::Box`, never bare `Box`: the prelude shares the user
    // program's module, and a user `type Box = { .. }` shadows the std name
    // (tests/dep_qualified_type_collision_test.rs).
    // The arena's scalars are `Cell`s and its tables `RefCell`s (#4342), so
    // the mark, the rewind and the bump itself are safe code; only the write
    // of a node into its fresh block (and, armed, the header writes) is not.
    let (trap_fields, trap_init) = if trap {
        (", gen: std::cell::Cell<u64>, hdrs: std::cell::RefCell<std::vec::Vec<*mut u64>>", ", gen: std::cell::Cell::new(0), hdrs: std::cell::RefCell::new(std::vec::Vec::new())")
    } else {
        ("", "")
    };
    s.push_str(&format!("{vis}struct AlmideArena {{ chunks: std::cell::RefCell<std::vec::Vec<std::boxed::Box<[std::mem::MaybeUninit<u8>]>>>, cur: std::cell::Cell<usize>, off: std::cell::Cell<usize>, base: std::cell::Cell<usize>, end: std::cell::Cell<usize>{trap_fields} }}\n"));
    s.push_str("impl AlmideArena {\n");
    s.push_str("    #[inline(always)] fn alloc(&self, size: usize, align: usize) -> *mut u8 {\n");
    // The current chunk's bounds ride in `base` / `end` (0 / 0 when there is
    // none), so the fast path is align, add, compare, store — no chunk-table
    // load per node (#3586).
    s.push_str("        let (base, end) = (self.base.get(), self.end.get());\n");
    s.push_str("        let start = (base + self.off.get() + align - 1) & !(align - 1);\n");
    s.push_str("        if end != 0 && start + size <= end { self.off.set(start + size - base); return start as *mut u8; }\n");
    s.push_str("        self.alloc_slow(size, align)\n");
    s.push_str("    }\n");
    s.push_str("    #[inline(never)] fn alloc_slow(&self, size: usize, align: usize) -> *mut u8 {\n");
    s.push_str("        let next = {\n");
    s.push_str("            let mut chunks = self.chunks.borrow_mut();\n");
    s.push_str("            let mut next = if chunks.is_empty() { 0 } else { self.cur.get() + 1 };\n");
    s.push_str("            while next < chunks.len() && chunks[next].len() < size + align { next += 1; }\n");
    s.push_str("            if next == chunks.len() {\n");
    s.push_str("                let want = ((64usize << 10) << chunks.len().min(10)).max(size + align);\n");
    s.push_str("                chunks.push(vec![std::mem::MaybeUninit::uninit(); want].into_boxed_slice());\n");
    s.push_str("            }\n");
    s.push_str("            next\n");
    s.push_str("        };\n");
    s.push_str("        self.seat(next, 0);\n");
    s.push_str("        self.alloc(size, align)\n");
    s.push_str("    }\n");
    s.push_str("    #[inline(always)] fn seat(&self, cur: usize, off: usize) {\n");
    s.push_str("        self.cur.set(cur); self.off.set(off);\n");
    s.push_str("        match self.chunks.borrow_mut().get_mut(cur) { Some(c) => { let b = c.as_mut_ptr() as usize; self.base.set(b); self.end.set(b + c.len()); } None => { self.base.set(0); self.end.set(0); } }\n");
    s.push_str("    }\n");
    s.push_str("}\n");
    s.push_str(&format!("thread_local! {{ static ALMIDE_ARENA: AlmideArena = const {{ AlmideArena {{ chunks: std::cell::RefCell::new(std::vec::Vec::new()), cur: std::cell::Cell::new(0), off: std::cell::Cell::new(0), base: std::cell::Cell::new(0), end: std::cell::Cell::new(0){trap_init} }} }}; }}\n"));
    // SAFETY (the node writes): the block `alloc` returns is fresh, sized and
    // aligned for `T`, and inside a chunk the arena owns for the thread's
    // life. The untrapped `almide_rgn_alloc` takes only the block's address
    // out of `with` and writes `v` outside the closure: a closure capturing
    // `v` by reference kept `with` out of line and sent the node through the
    // stack, where x86 reloaded two 8-byte stores with one 16-byte load that
    // store forwarding cannot serve (#3586).
    if trap {
        // The header sits in the 8 bytes before the value: the block is
        // over-aligned to `max(align, 8)` and the value placed one alignment
        // unit in, so both the header and the value keep their alignment.
        s.push_str(&format!("#[inline(always)] {vis}fn almide_rgn_alloc<T: Copy>(v: T) -> AlmideRgn<T> {{ ALMIDE_ARENA.with(|a| {{ let align = std::mem::align_of::<T>().max(8); let p = a.alloc(std::mem::size_of::<T>() + align, align); let g = a.gen.get(); let hdr = p.wrapping_add(align - 8) as *mut u64; unsafe {{ hdr.write(g) }}; a.hdrs.borrow_mut().push(hdr); let vp = p.wrapping_add(align) as *mut T; unsafe {{ vp.write(v) }}; AlmideRgn(unsafe {{ std::ptr::NonNull::new_unchecked(vp) }}, g) }}) }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_region_save() -> (usize, usize, usize) {{ ALMIDE_ARENA.with(|a| (a.cur.get(), a.off.get(), a.hdrs.borrow().len())) }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_region_restore(mark: (usize, usize, usize)) {{ ALMIDE_ARENA.with(|a| {{ for h in a.hdrs.borrow_mut().drain(mark.2..) {{ unsafe {{ h.write(ALMIDE_RGN_POISON) }} }} a.gen.set(a.gen.get() + 1); a.seat(mark.0, mark.1); }}) }}\n"));
    } else {
        s.push_str(&format!("#[inline(always)] {vis}fn almide_rgn_alloc<T: Copy>(v: T) -> AlmideRgn<T> {{ let p = ALMIDE_ARENA.with(|a| a.alloc(std::mem::size_of::<T>(), std::mem::align_of::<T>())) as *mut T; unsafe {{ p.write(v) }}; AlmideRgn(unsafe {{ std::ptr::NonNull::new_unchecked(p) }}) }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_region_save() -> (usize, usize) {{ ALMIDE_ARENA.with(|a| (a.cur.get(), a.off.get())) }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_region_restore(mark: (usize, usize)) {{ ALMIDE_ARENA.with(|a| a.seat(mark.0, mark.1)) }}\n"));
    }
    s.push_str(&format!("#[inline(always)] {vis}fn almide_region_window<R>(body: impl FnOnce() -> R) -> R {{ let mark = almide_region_save(); let r = body(); almide_region_restore(mark); r }}\n"));
}
