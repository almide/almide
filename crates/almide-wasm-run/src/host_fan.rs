//! The instance-parallel fan offer on the embedded host (#3003 stage 1,
//! ADR-0011 §D2a). The guest hands over one pure scalar chunk map
//! (`crates/almide-wasm/src/fan_par_lower.rs` documents the request); this
//! host runs the elements on fresh instances of the SAME compiled module,
//! one instance per OS thread, each with its own linear memory — nothing is
//! shared, no atomics exist — and writes the results back in list order.
//!
//! Every failure answers 0 ("not served") and the guest runs the map
//! sequentially: a chunk that traps, aborts or exits, an instantiation
//! error, a malformed request. The chunk is pure (the emitter's
//! qualification), so whatever a child did is unobservable — its stdout,
//! stderr and exit are captured in its own `Host` and dropped — and the
//! sequential run then produces exactly the observation it always did.

use std::sync::{Arc, Mutex};

use super::{Host, StdinSource};

/// `fs_call` op of the offer (crates/almide-wasm `OP_FAN_PAR`).
pub(super) const OP_FAN_PAR: i32 = 74;

/// What a chunk instance needs: the run's engine, compiled module and
/// import set, and the run's own resource bounds.
pub(super) struct ParCtx {
    pub(super) engine: wasmtime::Engine,
    pub(super) module: wasmtime::Module,
    pub(super) linker: wasmtime::Linker<Host>,
    pub(super) max_memory_bytes: Option<usize>,
    /// The run has a watchdog: a chunk store traps on the same epoch tick.
    pub(super) epoch: bool,
    pub(super) args: Vec<String>,
}

/// The parsed request.
struct Request {
    site: String,
    /// `(kind, absolute offset)` per tuple field; empty = a scalar result.
    fields: Vec<(i64, u32)>,
    caps: Vec<i64>,
    elems: Vec<i64>,
}

fn parse(a: &[u8]) -> Option<Request> {
    let slot = |j: usize| -> Option<i64> { Some(i64::from_le_bytes(a.get(8 * j..8 * j + 8)?.try_into().ok()?)) };
    let (k, n, m, r) = (slot(0)?, usize::try_from(slot(1)?).ok()?, usize::try_from(slot(2)?).ok()?, usize::try_from(slot(3)?).ok()?);
    if a.len() != 8 * (4 + 2 * r + m + n) {
        return None;
    }
    let fields = (0..r).map(|j| Some((slot(4 + 2 * j)?, u32::try_from(slot(5 + 2 * j)?).ok()?))).collect::<Option<Vec<_>>>()?;
    let caps = (0..m).map(|j| slot(4 + 2 * r + j)).collect::<Option<Vec<_>>>()?;
    let elems = (0..n).map(|j| slot(4 + 2 * r + m + j)).collect::<Option<Vec<_>>>()?;
    Some(Request { site: format!("__fan_site_{k}"), fields, caps, elems })
}

/// Serve the offer: 1 = the answer room holds every element's result.
pub(super) fn serve(
    caller: &mut wasmtime::Caller<'_, Host>,
    (a_ptr, a_len): (i32, i32),
    (b_ptr, b_len): (i32, i32),
) -> wasmtime::Result<i64> {
    let Some(ctx) = caller.data().par.clone() else { return Ok(0) };
    let mem = caller.get_export("memory").and_then(|e| e.into_memory()).expect("exported memory");
    let mut a = vec![0u8; a_len as u32 as usize];
    mem.read(&*caller, a_ptr as u32 as usize, &mut a)?;
    let Some(req) = parse(&a) else { return Ok(0) };
    let width = req.fields.len().max(1);
    if b_len as u32 as usize != 8 * width * req.elems.len() {
        return Ok(0);
    }
    let Some(results) = run_all(&ctx, &req) else {
        if almide_base::env::flag("ALMIDE_DBG_FAN") {
            eprintln!("[fan-dbg] {}: not served (a chunk failed), sequential", req.site);
        }
        return Ok(0);
    };
    if almide_base::env::flag("ALMIDE_DBG_FAN") {
        eprintln!("[fan-dbg] {}: served on separate instances ({} elements)", req.site, req.elems.len());
    }
    let mut b = Vec::with_capacity(b_len as u32 as usize);
    for v in results {
        b.extend_from_slice(&v.to_le_bytes());
    }
    mem.write(&mut *caller, b_ptr as u32 as usize, &b)?;
    Ok(1)
}

/// Every element's result slots, element-major — `None` if any chunk failed.
fn run_all(ctx: &ParCtx, req: &Request) -> Option<Vec<i64>> {
    let n = req.elems.len();
    if n == 0 {
        return Some(Vec::new());
    }
    let threads = std::thread::available_parallelism().map_or(1, |p| p.get()).min(n);
    let width = req.fields.len().max(1);
    let parts: Vec<Option<Vec<(usize, Vec<i64>)>>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads).map(|t| s.spawn(move || run_part(ctx, req, t, threads).ok())).collect();
        handles.into_iter().map(|h| h.join().ok().flatten()).collect()
    });
    let mut out = vec![0i64; n * width];
    for part in parts {
        for (i, vals) in part? {
            out[i * width..i * width + width].copy_from_slice(&vals);
        }
    }
    Some(out)
}

/// One thread: one fresh instance, the elements `t, t + threads, …`.
fn run_part(ctx: &ParCtx, req: &Request, t: usize, threads: usize) -> anyhow::Result<Vec<(usize, Vec<i64>)>> {
    let limits = match ctx.max_memory_bytes {
        Some(cap) => wasmtime::StoreLimitsBuilder::new().memory_size(cap).build(),
        None => wasmtime::StoreLimits::default(),
    };
    let exit = Arc::new(Mutex::new(None));
    let host = Host {
        out: Arc::default(),
        err: Arc::default(),
        exit: exit.clone(),
        fs_buf: Arc::default(),
        stdin: Arc::new(Mutex::new(StdinSource::Buf(Vec::new()))),
        args: ctx.args.clone(),
        limits,
        calls: Arc::default(),
        serve: Arc::default(),
        live_out: None,
        err_last: Arc::default(),
        par: None,
    };
    let mut store = wasmtime::Store::new(&ctx.engine, host);
    store.limiter(|h| &mut h.limits);
    if ctx.epoch {
        store.set_epoch_deadline(1);
    }
    let instance = ctx.linker.instantiate(&mut store, &ctx.module)?;
    let func = instance.get_func(&mut store, &req.site).ok_or_else(|| anyhow::anyhow!("no chunk export"))?;
    let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| anyhow::anyhow!("no memory"))?;
    let ty = func.ty(&store);
    let params: Vec<wasmtime::ValType> = ty.params().collect();
    if params.len() != 1 + req.caps.len() || ty.results().len() != 1 {
        anyhow::bail!("chunk signature");
    }
    let mut done = Vec::new();
    for i in (t..req.elems.len()).step_by(threads) {
        let bits = std::iter::once(req.elems[i]).chain(req.caps.iter().copied());
        let args: Vec<wasmtime::Val> = params.iter().zip(bits).map(|(p, b)| to_val(p, b)).collect();
        let mut ret = [wasmtime::Val::I32(0)];
        func.call(&mut store, &args, &mut ret)?;
        if exit.lock().expect("chunk exit").is_some() {
            anyhow::bail!("chunk exited");
        }
        let vals = if req.fields.is_empty() {
            vec![from_val(&ret[0])?]
        } else {
            let base = ret[0].i32().ok_or_else(|| anyhow::anyhow!("tuple handle"))? as u32 as usize;
            let data = memory.data(&store);
            req.fields
                .iter()
                .map(|&(kind, off)| {
                    let at = base + off as usize;
                    let len = if kind == 2 { 4 } else { 8 };
                    let raw = data.get(at..at + len).ok_or_else(|| anyhow::anyhow!("tuple read"))?;
                    Ok(if kind == 2 {
                        i64::from(u32::from_le_bytes(raw.try_into()?))
                    } else {
                        i64::from_le_bytes(raw.try_into()?)
                    })
                })
                .collect::<anyhow::Result<Vec<i64>>>()?
        };
        done.push((i, vals));
    }
    Ok(done)
}

fn to_val(ty: &wasmtime::ValType, bits: i64) -> wasmtime::Val {
    match ty {
        wasmtime::ValType::I64 => wasmtime::Val::I64(bits),
        wasmtime::ValType::F64 => wasmtime::Val::F64(bits as u64),
        _ => wasmtime::Val::I32(bits as i32),
    }
}

fn from_val(v: &wasmtime::Val) -> anyhow::Result<i64> {
    Ok(match v {
        wasmtime::Val::I64(x) => *x,
        wasmtime::Val::F64(x) => *x as i64,
        wasmtime::Val::I32(x) => i64::from(*x as u32),
        _ => anyhow::bail!("chunk result kind"),
    })
}
