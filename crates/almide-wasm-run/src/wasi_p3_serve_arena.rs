// `include!`d part of wasi_p3.rs: the stock serve export's per-request
// arena (#3444) — split from wasi_p3_serve.rs for the file budget.

/// The per-request arena (#3444): what `handle` saves on entry and restores
/// once the request is answered — the guest allocator's state
/// (`almide_wasm::host_exports::arena`: the bump pointer, the line buffer, the
/// lookup side table, the free-list heads) and the shim's caches that point
/// into the heap (the fan slot table, the environment snapshot). Nothing a
/// request allocates outlives it: the host has copied the body and the
/// trailers when their writes complete, top-lets are re-evaluated by the
/// next `main` (a plain store), and the guest's own long-lived blocks are
/// the ones restored here.
struct ServeArena {
    globals: Vec<u32>,
    words: Vec<u32>,
}

impl ServeArena {
    const LOCALS: u32 = 26;

    fn new(heap_global: u32, g: P3Globals) -> Self {
        use almide_wasm::host_exports::arena;
        assert_eq!(heap_global, arena::GLOBALS[0], "the module's __heap is not the structural leg's bump pointer");
        let globals: Vec<u32> = arena::GLOBALS.into_iter().chain([g.g_slots, g.g_slotn, g.g_env, g.g_envn]).collect();
        let words: Vec<u32> = arena::head_words().collect();
        assert_eq!((globals.len() + words.len()) as u32, Self::LOCALS, "ServeArena::LOCALS drift");
        ServeArena { globals, words }
    }

    /// Save the state into the locals from `HandleLocals::ARENA`, then zero
    /// the heads: a block filed before the request must not be taken in it.
    fn open(&self, i: &mut wasm_encoder::InstructionSink<'_>) {
        for (at, &gl) in (HandleLocals::ARENA..).zip(&self.globals) {
            i.global_get(gl).local_set(at);
        }
        for (at, &w) in (self.words_at()..).zip(&self.words) {
            i.i32_const(w as i32).i32_load(mem(0)).local_set(at);
            i.i32_const(w as i32).i32_const(0).i32_store(mem(0));
        }
    }

    /// Put the saved state back: every block the request made is free.
    fn close(&self, i: &mut wasm_encoder::InstructionSink<'_>) {
        for (at, &gl) in (HandleLocals::ARENA..).zip(&self.globals) {
            i.local_get(at).global_set(gl);
        }
        for (at, &w) in (self.words_at()..).zip(&self.words) {
            i.i32_const(w as i32).local_get(at).i32_store(mem(0));
        }
    }

    /// The first local holding a head word: past the saved globals.
    fn words_at(&self) -> u32 {
        HandleLocals::ARENA + self.globals.len() as u32
    }
}
