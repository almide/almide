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
//! The handle's `Deref` is `unsafe { &*self.0 }` over memory the window has
//! rewound: rustc cannot see a handle that outlives its window, so a wrong
//! region-pure verdict in `pass_region_window.rs` is a SILENT use-after-rewind
//! — the slot still holds the old bytes until the next window overwrites it.
//! `ALMIDE_REGION_TRAP_STALE=1` (read at BUILD time, like
//! `ALMIDE_RC_TRAP_DOUBLE_FREE` on the wasm leg) arms the trap: every block
//! gets an 8-byte generation header, the handle carries the generation it was
//! allocated under, a rewind poisons every header it releases and bumps the
//! generation, and every `Deref` compares the two — a stale read panics with a
//! message naming the pass instead of returning the old bytes. Nested windows
//! keep their outer handles live (only the released headers are poisoned) and
//! re-use of a rewound slot is caught by the generation (ABA). Off by default:
//! the shipped prelude is byte-identical to the one before the knob existed.
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
        s.push_str(&format!("{vis}struct AlmideRgn<T>({vis}*const T, {vis}u64);\n"));
        s.push_str(&format!("{vis}const ALMIDE_RGN_POISON: u64 = u64::MAX;\n"));
        // The cold half is out of line so the check itself stays a load and a compare.
        s.push_str(&format!("#[cold] #[inline(never)] {vis}fn almide_rgn_stale(stamp: u64, hdr: u64) -> ! {{ let slot = if hdr == ALMIDE_RGN_POISON {{ String::from(\"poisoned by a rewind\") }} else {{ format!(\"re-allocated under generation {{hdr}}\") }}; panic!(\"almide: region window trap (ALMIDE_REGION_TRAP_STALE): a region handle allocated under generation {{stamp}} was read after its window rewound the arena (slot {{slot}}); pass_region_window's region-pure verdict was wrong — report it\") }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_rgn_check(p: *const u8, stamp: u64) {{ let hdr = unsafe {{ *(p.sub(8) as *const u64) }}; if hdr != stamp {{ almide_rgn_stale(stamp, hdr) }} }}\n"));
    } else {
        s.push_str(&format!("{vis}struct AlmideRgn<T>({vis}*const T);\n"));
    }
    s.push_str("impl<T> Clone for AlmideRgn<T> { #[inline(always)] fn clone(&self) -> Self { *self } }\n");
    s.push_str("impl<T> Copy for AlmideRgn<T> {}\n");
    if trap {
        s.push_str("impl<T> std::ops::Deref for AlmideRgn<T> { type Target = T; #[inline(always)] fn deref(&self) -> &T { almide_rgn_check(self.0 as *const u8, self.1); unsafe { &*self.0 } } }\n");
    } else {
        s.push_str("impl<T> std::ops::Deref for AlmideRgn<T> { type Target = T; #[inline(always)] fn deref(&self) -> &T { unsafe { &*self.0 } } }\n");
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
    let (trap_fields, trap_init) = if trap {
        (", gen: u64, hdrs: std::vec::Vec<*mut u64>", ", gen: 0, hdrs: std::vec::Vec::new()")
    } else {
        ("", "")
    };
    s.push_str(&format!("{vis}struct AlmideArena {{ chunks: std::vec::Vec<std::boxed::Box<[std::mem::MaybeUninit<u8>]>>, cur: usize, off: usize{trap_fields} }}\n"));
    s.push_str("impl AlmideArena {\n");
    s.push_str("    #[inline(always)] fn alloc(&mut self, size: usize, align: usize) -> *mut u8 {\n");
    s.push_str("        if self.cur < self.chunks.len() {\n");
    s.push_str("            let chunk = &mut self.chunks[self.cur];\n");
    s.push_str("            let base = chunk.as_mut_ptr() as usize;\n");
    s.push_str("            let start = (base + self.off + align - 1) & !(align - 1);\n");
    s.push_str("            if start + size <= base + chunk.len() { self.off = start + size - base; return start as *mut u8; }\n");
    s.push_str("        }\n");
    s.push_str("        self.alloc_slow(size, align)\n");
    s.push_str("    }\n");
    s.push_str("    #[inline(never)] fn alloc_slow(&mut self, size: usize, align: usize) -> *mut u8 {\n");
    s.push_str("        let mut next = if self.chunks.is_empty() { 0 } else { self.cur + 1 };\n");
    s.push_str("        while next < self.chunks.len() && self.chunks[next].len() < size + align { next += 1; }\n");
    s.push_str("        if next == self.chunks.len() {\n");
    s.push_str("            let want = ((64usize << 10) << self.chunks.len().min(10)).max(size + align);\n");
    s.push_str("            self.chunks.push(vec![std::mem::MaybeUninit::uninit(); want].into_boxed_slice());\n");
    s.push_str("        }\n");
    s.push_str("        self.cur = next; self.off = 0;\n");
    s.push_str("        self.alloc(size, align)\n");
    s.push_str("    }\n");
    s.push_str("}\n");
    s.push_str(&format!("thread_local! {{ static ALMIDE_ARENA: std::cell::UnsafeCell<AlmideArena> = const {{ std::cell::UnsafeCell::new(AlmideArena {{ chunks: std::vec::Vec::new(), cur: 0, off: 0{trap_init} }}) }}; }}\n"));
    // SAFETY (all three): the cell is thread-local and the `&mut` never
    // leaves the closure; `alloc` calls no user code, so no re-entry.
    if trap {
        // The header sits in the 8 bytes before the value: the block is
        // over-aligned to `max(align, 8)` and the value placed one alignment
        // unit in, so both the header and the value keep their alignment.
        s.push_str(&format!("#[inline(always)] {vis}fn almide_rgn_alloc<T: Copy>(v: T) -> AlmideRgn<T> {{ ALMIDE_ARENA.with(|a| {{ let a = unsafe {{ &mut *a.get() }}; let align = std::mem::align_of::<T>().max(8); let p = a.alloc(std::mem::size_of::<T>() + align, align); let hdr = unsafe {{ p.add(align - 8) }} as *mut u64; unsafe {{ hdr.write(a.gen) }}; a.hdrs.push(hdr); let vp = unsafe {{ p.add(align) }} as *mut T; unsafe {{ vp.write(v) }}; AlmideRgn(vp, a.gen) }}) }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_region_save() -> (usize, usize, usize) {{ ALMIDE_ARENA.with(|a| {{ let a = unsafe {{ &*a.get() }}; (a.cur, a.off, a.hdrs.len()) }}) }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_region_restore(mark: (usize, usize, usize)) {{ ALMIDE_ARENA.with(|a| {{ let a = unsafe {{ &mut *a.get() }}; for h in a.hdrs.drain(mark.2..) {{ unsafe {{ h.write(ALMIDE_RGN_POISON) }} }} a.gen += 1; a.cur = mark.0; a.off = mark.1; }}) }}\n"));
    } else {
        s.push_str(&format!("#[inline(always)] {vis}fn almide_rgn_alloc<T: Copy>(v: T) -> AlmideRgn<T> {{ ALMIDE_ARENA.with(|a| {{ let a = unsafe {{ &mut *a.get() }}; let p = a.alloc(std::mem::size_of::<T>(), std::mem::align_of::<T>()) as *mut T; unsafe {{ p.write(v) }}; AlmideRgn(p) }}) }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_region_save() -> (usize, usize) {{ ALMIDE_ARENA.with(|a| {{ let a = unsafe {{ &*a.get() }}; (a.cur, a.off) }}) }}\n"));
        s.push_str(&format!("#[inline(always)] {vis}fn almide_region_restore(mark: (usize, usize)) {{ ALMIDE_ARENA.with(|a| {{ let a = unsafe {{ &mut *a.get() }}; a.cur = mark.0; a.off = mark.1; }}) }}\n"));
    }
    s.push_str(&format!("#[inline(always)] {vis}fn almide_region_window<R>(body: impl FnOnce() -> R) -> R {{ let mark = almide_region_save(); let r = body(); almide_region_restore(mark); r }}\n"));
}
