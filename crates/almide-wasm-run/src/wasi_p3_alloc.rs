// `include!`d part of wasi_p3.rs: the allocator's exit report of an ARMED
// module (#2407, `ALMIDE_WASM_ALLOC_COUNT`) on the p3 component.
//
// The core-module host reads the five counter globals an armed build exports
// after the run and the CLI prints them. A component keeps its core module's
// globals to itself, so on the component route (ADR-0023 step 7) the shim
// writes the same line to stderr: once `main` returns, and in `exit` before
// the code leaves (a trap reaches `exit` too), the moments the core host's
// reading covers. An unarmed module exports no counters and gets none of
// this: a shipped artifact is unchanged.

/// The armed module's counters and heap watermarks, by global index.
#[derive(Clone, Copy)]
struct AllocGlobals {
    allocs: u32,
    reused: u32,
    bytes: u32,
    frees: u32,
    reclaimed: u32,
    heap: u32,
    heap_high: Option<u32>,
}

impl AllocGlobals {
    /// `Some` exactly when the module exports all five counters (the
    /// all-or-none rule the core host reads them by).
    fn find(exports: &[(String, ExportKind, u32)], heap: u32) -> Option<Self> {
        let global = |name: &str| exports.iter().find(|(n, k, _)| n == name && *k == ExportKind::Global).map(|(_, _, i)| *i);
        Some(AllocGlobals {
            allocs: global("__alloc_count")?,
            reused: global("__alloc_reused")?,
            bytes: global("__alloc_bytes")?,
            frees: global("__free_count")?,
            reclaimed: global("__region_reclaimed")?,
            heap,
            heap_high: global("__heap_high"),
        })
    }
}

/// Where the report is written, and how it leaves: the park's staging area
/// (nothing is staged once the program is ending), the park span the heap
/// was moved up by, and the stderr line shim.
#[derive(Clone, Copy)]
struct AllocReportAt {
    buf: u64,
    span: u64,
    f_eprintln: u32,
}

// Locals of the report function.
const AR_POS: u32 = 0;
const AR_V: u32 = 1;
const AR_LO: u32 = 2;
const AR_HI: u32 = 3;
const AR_TMP: u32 = 4;

/// `() -> ()`: `__ALMD_WASM_ALLOC allocs=… reused=… bytes=… frees=…
/// reclaimed=… live=… heap_end=…` on stderr — the core host's line, with
/// `live = allocs − frees − reclaimed` and the heap end measured from the
/// module's own heap base (the park span taken off).
fn shim_alloc_report(a: AllocGlobals, at: AllocReportAt) -> Function {
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64), (3, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const(at.buf as i32).local_set(AR_POS);
    let fields = [
        ("__ALMD_WASM_ALLOC allocs=", Some(a.allocs)),
        (" reused=", Some(a.reused)),
        (" bytes=", Some(a.bytes)),
        (" frees=", Some(a.frees)),
        (" reclaimed=", Some(a.reclaimed)),
        (" live=", None),
    ];
    for (label, global) in fields {
        report_text(&mut i, label);
        match global {
            Some(g) => {
                i.global_get(g);
            }
            None => {
                i.global_get(a.allocs).global_get(a.frees).i64_sub().global_get(a.reclaimed).i64_sub();
            }
        }
        report_decimal(&mut i);
    }
    report_text(&mut i, " heap_end=");
    heap_end(&mut i, a, at.span);
    report_decimal(&mut i);
    i.i32_const(at.buf as i32).local_get(AR_POS).i32_const(at.buf as i32).i32_sub().call(at.f_eprintln);
    i.end();
    f
}

/// The heap end as an i64: the higher of `__heap` and `__heap_high` (a
/// region window rewinds `__heap`), less the park span.
fn heap_end(i: &mut wasm_encoder::InstructionSink<'_>, a: AllocGlobals, span: u64) {
    match a.heap_high {
        // `select` answers its first operand when the condition holds.
        Some(hi) => {
            i.global_get(hi).global_get(a.heap).global_get(hi).global_get(a.heap).i32_gt_u().select();
        }
        None => {
            i.global_get(a.heap);
        }
    }
    i.i32_const(span as i32).i32_sub().i64_extend_i32_u();
}

/// Store `text` at `pos` and advance it.
fn report_text(i: &mut wasm_encoder::InstructionSink<'_>, text: &str) {
    for (k, b) in text.bytes().enumerate() {
        i.local_get(AR_POS).i32_const(i32::from(b)).i32_store8(mem8(k as u64));
    }
    i.local_get(AR_POS).i32_const(text.len() as i32).i32_add().local_set(AR_POS);
}

/// The i64 on the stack in decimal at `pos` (a `-` first when negative),
/// advancing `pos`: digits least significant first, then reversed in place.
fn report_decimal(i: &mut wasm_encoder::InstructionSink<'_>) {
    i.local_set(AR_V);
    i.local_get(AR_V).i64_const(0).i64_lt_s();
    i.if_(BlockType::Empty);
    i.local_get(AR_POS).i32_const(i32::from(b'-')).i32_store8(mem8(0));
    i.local_get(AR_POS).i32_const(1).i32_add().local_set(AR_POS);
    i.i64_const(0).local_get(AR_V).i64_sub().local_set(AR_V);
    i.end();
    i.local_get(AR_POS).local_set(AR_LO);
    i.loop_(BlockType::Empty);
    i.local_get(AR_POS).local_get(AR_V).i64_const(10).i64_rem_u().i32_wrap_i64().i32_const(i32::from(b'0')).i32_add().i32_store8(mem8(0));
    i.local_get(AR_POS).i32_const(1).i32_add().local_set(AR_POS);
    i.local_get(AR_V).i64_const(10).i64_div_u().local_tee(AR_V).i64_const(0).i64_ne().br_if(0);
    i.end();
    i.local_get(AR_POS).i32_const(1).i32_sub().local_set(AR_HI);
    i.block(BlockType::Empty);
    i.loop_(BlockType::Empty);
    i.local_get(AR_LO).local_get(AR_HI).i32_ge_u().br_if(1);
    i.local_get(AR_LO).i32_load8_u(mem8(0)).local_set(AR_TMP);
    i.local_get(AR_LO).local_get(AR_HI).i32_load8_u(mem8(0)).i32_store8(mem8(0));
    i.local_get(AR_HI).local_get(AR_TMP).i32_store8(mem8(0));
    i.local_get(AR_LO).i32_const(1).i32_add().local_set(AR_LO);
    i.local_get(AR_HI).i32_const(1).i32_sub().local_set(AR_HI);
    i.br(0);
    i.end();
    i.end();
}
