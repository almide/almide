// Fan concurrency runtime functions

// `fan.map` runs SEQUENTIALLY over an `Rc<dyn Fn>` thunk (the uniform closure
// repr). This is what lets a closure VALUE bound to a var reach `fan.map`: that
// value is an `Rc<dyn Fn>`, which is neither `Send` nor `Sync`, so it could not
// be moved across the `thread::scope` boundary the parallel version required.
// Results are identical to a parallel map (input order preserved); only the
// (unobservable) parallelism is dropped. `fan.race` never reaches this file:
// it lowers in the frontend to a sequential least-(spend, index) fold
// (almide-frontend lower/expressions_fan.rs). `fan.settle` lowers to a tuple
// (block) or `list.map` (mapper), so `almide_rt_fan_race` and
// `almide_rt_fan_settle` below have no producer (docs/design/fan-semantics-audit.md).
//
// `fan.map` is EFFECTFUL and runs EVERY element (ADR-0024 D1, C-005): an
// element's `Err` does not stop the map, the elements after it still run, and
// the LOWEST-INDEX `Err` is the map's defined Result error — the rule the block
// form `fan { }` follows (C-199). The caller's `!` then routes it to the
// effect-main termination path (`Error: <msg>` + exit 1), byte-identical to
// the wasm `__main_runner`.
pub fn almide_rt_fan_map<A, B>(
    items: Vec<A>,
    f: std::rc::Rc<dyn Fn(A) -> Result<B, AlmideString>>,
) -> Result<Vec<B>, AlmideString> {
    let __almide_r: Result<Vec<B>, AlmideString> = (|| {
        let mut out = Vec::with_capacity(items.len());
        let mut first_err: Option<AlmideString> = None;
        for item in items {
            match (f(item), &first_err) {
                (Ok(v), None) => out.push(v),
                (Err(e), None) => first_err = Some(e),
                _ => {}
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(out),
        }
    })();
    __almide_r.map_err(Into::into)
}

// `fan.map`'s PARALLEL twin (#2044). RustLowering's fan routing (native only) routes a
// `fan.map` here when its callback is a PURE lambda LITERAL whose element,
// result and captured types are all Send-safe (`is_send_safe_ty` in
// pass_rust_lowering_fan.rs: the scalars, String, List, Set, Map, Result and
// tuples/records of those), or an effect callback whose only effect is a
// nested fan (`Purity::for_fan`). Closure values and every other EFFECT
// callback (its side effects must stay in list order) keep the sequential
// `almide_rt_fan_map`.
// The callback is a raw `impl Fn` (the box pass leaves fan args unboxed), so
// a non-Send capture the pass missed is a loud rustc error, never a race.
//
// Observably identical to the sequential twin: every element runs (ADR-0024
// D1 — no worker stops at an Err), results land in pre-sized slots by index,
// and the map's Err is the LOWEST-INDEX Err in LIST ORDER. No "big list"
// cut-off (a 64-chunk fan over 14 cores would never clear a size threshold):
// the cost model (#3341, list.rs `almide_rt_fan_plan`) goes parallel only
// when the site's measured time per element is worth the offer's cost.
// `ALMIDE_FAN_SEQUENTIAL=1` forces the sequential path — the ablation knob.
//
// Every element writes into its own output timeline (ADR-0024 D5), flushed in
// list order. Every element runs, so no Err cuts the group: a trap in element k
// waits for the elements below it and discards everything above (ADR-0024 D6).
pub fn almide_rt_fan_map_par<A: Send + Sync + Clone, B: Send, F: Fn(A) -> Result<B, AlmideString> + Send + Sync>(
    items: Vec<A>,
    f: F,
) -> Result<Vec<B>, AlmideString> {
    // The slots cross threads, so an element's error is held as an owned
    // `String` (Send) and converted once at the boundary.
    let __almide_r: Result<Vec<B>, String> = (|| {
        let mut slots: Vec<Option<Result<B, String>>> = (0..items.len()).map(|_| None).collect();
        if items.len() < 2 || almide_rt_list_par_sequential() {
            for (slot, item) in slots.iter_mut().zip(items) {
                *slot = Some(f(item).map_err(String::from));
            }
            return almide_fan_map_settle_slots(slots);
        }
        // #3341: the cost model (list.rs `almide_rt_fan_plan`) decides from the
        // site's last measured time per element whether this offer is worth a
        // parallel run; every offer records what its elements took.
        let site = almide_rt_fan_map_par::<A, B, F> as fn(Vec<A>, F) -> Result<Vec<B>, AlmideString> as usize;
        let n = items.len();
        let (workers, clock) = AlmideFanClock::offer(site, n);
        if workers < 2 {
            clock.calling(|| {
                for (slot, item) in slots.iter_mut().zip(items) {
                    *slot = Some(f(item).map_err(String::from));
                }
                ((), n)
            });
            clock.record();
            return almide_fan_map_settle_slots(slots);
        }
        let block = almide_fan_block(n, workers);
        // The group flushes this thread's stdout first: a runtime abort on a worker
        // exits without reaching this thread's buffer (C-197, see rust.toml fan_expr).
        let group = almide_fan_group(n);
        // dealt in blocks to this thread and the resident pool (list.rs `almide_fan_deal`)
        let blocks: Vec<_> = items.chunks(block).zip(slots.chunks_mut(block)).enumerate().collect();
        almide_fan_panic_if_any(almide_fan_deal(blocks, workers, |blocks| {
            let elems = blocks.flat_map(|(b, (chunk, out))| out.iter_mut().zip(chunk.iter()).enumerate().map(move |(i, e)| (b * block + i, e)));
            clock.worker(elems, |(index, (slot, item))| {
                let _elem = almide_fan_enter(&group, index);
                *slot = Some(f(item.clone()).map_err(String::from));
                Some(true)
            });
        }));
        clock.record();
        almide_fan_map_settle_slots(slots)
    })();
    __almide_r.map_err(Into::into)
}

// The map's result from its filled slots: the ok values in list order, or the
// LOWEST-INDEX Err (every element ran — ADR-0024 D1).
fn almide_fan_map_settle_slots<B>(slots: Vec<Option<Result<B, String>>>) -> Result<Vec<B>, String> {
    let mut out = Vec::with_capacity(slots.len());
    for slot in slots {
        match slot {
            Some(Ok(v)) => out.push(v),
            Some(Err(e)) => return Err(e),
            // Every worker fills every slot of its chunk.
            None => unreachable!("fan.map_par: an element that never ran"),
        }
    }
    Ok(out)
}

// NO PRODUCER. This is the pre-0.42 thunk-list `fan.race(thunks)`, which
// evaluated only thunk[0]. The call form is now refused with E027
// (static_dispatch.rs), and the live `fan.race { }` / `fan.race(xs, f)` heads
// lower in the frontend to the least-(spend, index) fold, so they never call
// this function. Kept only until the runtime registry drops it.
pub fn almide_rt_fan_race<T>(
    thunks: Vec<impl Fn() -> Result<T, AlmideString>>,
) -> Result<T, AlmideString> {
    let __almide_r: Result<T, AlmideString> = (|| {
        match thunks.into_iter().next() {
            Some(thunk) => thunk(),
            None => Err(AlmideString::from("fan.race: no candidates")),
        }
    })();
    __almide_r.map_err(Into::into)
}

// `fan.any` tries the thunks in LIST ORDER and returns the FIRST `Ok`
// (deterministic — NOT wall-clock fastest). If every candidate fails it returns
// a defined `Err`, never panicking or trapping. Later thunks are never started.
// (`fan.race` is a different selection: least compute spent, tie -> list order.)
pub fn almide_rt_fan_any<T>(
    thunks: Vec<impl Fn() -> Result<T, AlmideString>>,
) -> Result<T, AlmideString> {
    let __almide_r: Result<T, AlmideString> = (|| {
        for thunk in &thunks {
            if let Ok(val) = thunk() {
                return Ok(val);
            }
        }
        Err(AlmideString::from("fan.any: all candidates failed"))
    })();
    __almide_r.map_err(Into::into)
}

// `fan.any_map` -- the T2-3 MAPPER form: apply `f` to the items IN LIST ORDER
// and return the FIRST `Ok`; an element's Err disqualifies that element only.
// All-fail (and the empty list) is the ledger-constant Err. Deterministic --
// the early cut is structural (later elements are never evaluated).
pub fn almide_rt_fan_any_map<A, B>(
    items: Vec<A>,
    f: std::rc::Rc<dyn Fn(A) -> Result<B, AlmideString>>,
) -> Result<B, AlmideString> {
    let __almide_r: Result<B, AlmideString> = (|| {
        for item in items {
            if let Ok(v) = f(item) {
                return Ok(v);
            }
        }
        Err(AlmideString::from("fan.any: all candidates failed"))
    })();
    __almide_r.map_err(Into::into)
}

// NO PRODUCER: `fan.settle { }` lowers to a tuple and `fan.settle(xs, f)` to
// `list.map`, both sequential. One output timeline per thunk, flushed in list order (ADR-0024 D5). The
// group flushes this thread's stdout first: a runtime abort on a worker exits
// without reaching this thread's buffer (C-197, see rust.toml fan_expr).
pub fn almide_rt_fan_settle<T: Send + 'static>(
    thunks: Vec<impl Fn() -> Result<T, AlmideString> + Send + Sync>,
) -> Vec<Result<T, AlmideString>> {
    let group = almide_fan_group(thunks.len());
    let group = &group;
    std::thread::scope(|s| {
        let handles: Vec<_> = thunks
            .iter()
            .enumerate()
            .map(|(i, thunk)| s.spawn(move || {
                let _elem = almide_fan_enter(group, i);
                // The error crosses the thread as an owned `String` (Send);
                // the identity while the alias is `String`.
                thunk().map_err(String::from)
            }))
            .collect();
        handles
            .into_iter()
            .map(|h| almide_fan_join(h).map_err(AlmideString::from))
            .collect()
    })
}

