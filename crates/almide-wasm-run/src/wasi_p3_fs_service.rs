// `include!`d part of wasi_p3.rs — shares the parent module's imports and
// items; nothing here is pub beyond the parent.
//
// The p3 fs service (#3140). The p3 shim used to answer a hand-written
// subset of the fs ops (read/write/exists/mkdir/remove against the FIRST
// preopen, errors without the `fs.<call>("<path>")` head), so a program that
// built as a p1 module lost fs surface — and behaviour — as a p3 component.
// Now the component carries the p1 fs service itself (crates/almide-wasi/src/
// fs_service.wat, the code the stock-p1 artifact runs) with its preview-1
// imports served by a small adapter over wasi:filesystem@0.3.0
// (`p3_fs_adapter.wat`): path resolution against the full preopen table and
// the cwd, list_dir / walk / glob, the metadata family, copy / rename, the
// temp-dir family, the line readers and env.os / env.temp_dir / env.cwd all
// answer what the p1 artifact answers, because they are the same code.

use crate::wasi::fs_service::{service_wat_ext, FsSplice, FsTypes, SpliceTargets, FS_PAGE};

/// The adapter text: its imports, `;; @@FUNCS@@`, then its functions.
const P3_FS_ADAPTER: &str = include_str!("p3_fs_adapter.wat");

/// wasi_snapshot_preview1's `errno` value for each wasi:filesystem@0.3.0
/// `error-code` case, by NAME (the service's `$errno_text` is keyed on the
/// preview-1 numbers). `other(option<string>)` has no preview-1 twin and
/// reads as `io`, which the service spells as its numbered fallback.
const P1_ERRNO_BY_CASE: &[(&str, i32)] = &[
    ("access", 2),
    ("already", 7),
    ("bad-descriptor", 8),
    ("busy", 10),
    ("deadlock", 16),
    ("quota", 19),
    ("exist", 20),
    ("file-too-large", 22),
    ("illegal-byte-sequence", 25),
    ("in-progress", 26),
    ("interrupted", 27),
    ("invalid", 28),
    ("io", 29),
    ("is-directory", 31),
    ("loop", 32),
    ("too-many-links", 34),
    ("message-size", 35),
    ("name-too-long", 37),
    ("no-device", 43),
    ("no-entry", 44),
    ("no-lock", 46),
    ("insufficient-memory", 48),
    ("insufficient-space", 51),
    ("not-directory", 54),
    ("not-empty", 55),
    ("not-recoverable", 56),
    ("unsupported", 58),
    ("no-tty", 59),
    ("no-such-device", 60),
    ("overflow", 61),
    ("not-permitted", 63),
    ("pipe", 64),
    ("read-only", 69),
    ("invalid-seek", 70),
    ("text-file-busy", 74),
    ("cross-device", 75),
    ("other", 29),
];

/// wasi_snapshot_preview1's `filetype` for each `descriptor-type` case by
/// name; a case without a preview-1 twin (fifo, other) is `unknown` (0).
const P1_FILETYPE_BY_CASE: &[(&str, i32)] = &[
    ("block-device", 1),
    ("character-device", 2),
    ("directory", 3),
    ("regular-file", 4),
    ("socket", 6),
    ("symbolic-link", 7),
];

/// The fs ops the p3 component answers through the service: every
/// [`crate::wasi::FS_SERVICE_OPS`] op the module reaches, and read_text
/// (op 1) whenever the fan await (41) is present — its fallback re-reads
/// through op 1, so the service must carry that arm.
fn p3_service_ops(host_ops: &[i32]) -> Vec<i32> {
    let mut ops: Vec<i32> = host_ops
        .iter()
        .copied()
        .filter(|op| crate::wasi::FS_SERVICE_OPS.iter().any(|(o, _, _)| o == op))
        .collect();
    if ops.contains(&41) && !ops.contains(&1) {
        ops.push(1);
    }
    ops.sort_unstable();
    ops.dedup();
    ops
}

/// The ops `shim_fs_call` forwards to the service: all of them but the fan
/// prefetch triple, which the shim schedules itself (async opens).
fn p3_forwarded_ops(ops: &[i32]) -> Vec<i32> {
    ops.iter().copied().filter(|op| !(40..=42).contains(op)).collect()
}

/// The record `id`'s field offsets by field name.
fn field_offsets(
    resolve: &wit_parser::Resolve,
    sa: &wit_parser::SizeAlign,
    id: wit_parser::TypeId,
) -> anyhow::Result<Vec<(String, u64)>> {
    match &resolve.types[id].kind {
        wit_parser::TypeDefKind::Record(r) => Ok(sa
            .field_offsets(r.fields.iter().map(|f| &f.ty))
            .into_iter()
            .zip(&r.fields)
            .map(|((at, _), f)| (f.name.clone(), at.size_wasm32() as u64))
            .collect()),
        k => Err(anyhow::anyhow!("expected a record, got {k:?}")),
    }
}

fn offset_of(fields: &[(String, u64)], name: &str) -> anyhow::Result<u64> {
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, at)| *at)
        .ok_or_else(|| anyhow::anyhow!("record field {name} not found"))
}

/// A type behind any chain of `type a = b` aliases.
fn deref_alias(resolve: &wit_parser::Resolve, mut ty: wit_parser::Type) -> wit_parser::Type {
    while let wit_parser::Type::Id(id) = ty {
        match &resolve.types[id].kind {
            wit_parser::TypeDefKind::Type(inner) => ty = *inner,
            _ => break,
        }
    }
    ty
}

/// `case index -> value` arms of a generated WAT lookup function, from a
/// variant's cases by name. `strict`: an unnamed case is a WIT drift.
fn case_arms(
    resolve: &wit_parser::Resolve,
    id: wit_parser::TypeId,
    table: &[(&str, i32)],
    strict: bool,
) -> anyhow::Result<String> {
    let wit_parser::TypeDefKind::Variant(v) = &resolve.types[id].kind else {
        return Err(anyhow::anyhow!("expected a variant"));
    };
    let mut arms = String::new();
    for (k, c) in v.cases.iter().enumerate() {
        let value = match table.iter().find(|(n, _)| *n == c.name) {
            Some((_, value)) => *value,
            None if strict => return Err(anyhow::anyhow!("no preview-1 twin for case {}", c.name)),
            None => continue,
        };
        arms.push_str(&format!(
            "    (if (i32.eq (local.get $c) (i32.const {k})) (then (return (i32.const {value}))))\n"
        ));
    }
    Ok(arms)
}

/// The adapter's `@NAME@` holes, DERIVED from the vendored WIT (the fs_abi
/// doctrine), plus the generated `$p3_errno` / `$p3_ftype` lookups.
fn adapter_funcs(resolve: &wit_parser::Resolve, abi: &FsAbi) -> anyhow::Result<String> {
    use wit_parser::Type;
    let mut sa = wit_parser::SizeAlign::default();
    sa.fill(resolve)?;
    let stat = field_offsets(resolve, &sa, fs_type(resolve, "descriptor-stat")?)?;
    let entry_id = fs_type(resolve, "directory-entry")?;
    let entry = field_offsets(resolve, &sa, entry_id)?;
    // option<instant>: the payload sits past the 1-byte tag, at the
    // instant's alignment; the instant is { seconds s64, nanoseconds u32 }.
    let wit_parser::TypeDefKind::Record(r) = &resolve.types[fs_type(resolve, "descriptor-stat")?].kind else {
        return Err(anyhow::anyhow!("descriptor-stat is not a record"));
    };
    let mtime = r
        .fields
        .iter()
        .find(|f| f.name == "data-modification-timestamp")
        .ok_or_else(|| anyhow::anyhow!("descriptor-stat has no data-modification-timestamp"))?;
    let Type::Id(opt) = deref_alias(resolve, mtime.ty) else {
        return Err(anyhow::anyhow!("the timestamp is not an option"));
    };
    let wit_parser::TypeDefKind::Option(inst) = &resolve.types[opt].kind else {
        return Err(anyhow::anyhow!("the timestamp is not an option"));
    };
    let Type::Id(inst_id) = deref_alias(resolve, *inst) else {
        return Err(anyhow::anyhow!("instant is not a record"));
    };
    let opt_pay = sa.payload_offset(wit_parser::Int::U8, [None, Some(inst)]).size_wasm32() as u64;
    let instant = field_offsets(resolve, &sa, inst_id)?;
    let de_name = offset_of(&entry, "name")?;
    let holes: [(&str, u64); 18] = [
        ("OPEN_PAY", abi.open_payload),
        ("UNIT_PAY", abi.unit_payload),
        ("STAT_PAY", abi.stat_payload),
        ("ST_TYPE", offset_of(&stat, "type")?),
        ("ST_NLINK", offset_of(&stat, "link-count")?),
        ("ST_SIZE", offset_of(&stat, "size")?),
        ("ST_ATIM", offset_of(&stat, "data-access-timestamp")?),
        ("ST_MTIM", offset_of(&stat, "data-modification-timestamp")?),
        ("ST_CTIM", offset_of(&stat, "status-change-timestamp")?),
        ("OPT_SEC", opt_pay + offset_of(&instant, "seconds")?),
        ("OPT_NS", opt_pay + offset_of(&instant, "nanoseconds")?),
        ("DE_SIZE", sa.size(&Type::Id(entry_id)).size_wasm32() as u64),
        ("DE_TYPE", offset_of(&entry, "type")?),
        ("DE_NAME", de_name),
        ("DE_NAME_LEN", de_name + 4),
        ("STAT_TYPE_AT", abi.stat_payload + offset_of(&stat, "type")?),
        // the #2119 reservations, shared with the shims that lower the same lists
        ("PREOPEN_RESERVE", PREOPEN_RESERVE as u64),
        ("ENV_RESERVE", ENV_RESERVE as u64),
    ];
    // The adapter's retptr scratch is fsp+128..240 (below the preopen table
    // at fsp+256): every result it lands there must fit.
    if abi.stat_size > 112 {
        return Err(anyhow::anyhow!("the stat result ({} bytes) outgrew the adapter's scratch", abi.stat_size));
    }
    let (_, funcs) = P3_FS_ADAPTER
        .split_once(";; @@FUNCS@@")
        .ok_or_else(|| anyhow::anyhow!("adapter: no @@FUNCS@@ marker"))?;
    let mut funcs = funcs.to_string();
    for (name, value) in holes {
        funcs = funcs.replace(&format!("@{name}@"), &value.to_string());
    }
    if let Some(at) = funcs.find('@') {
        let hole: String = funcs[at..].chars().take(24).collect();
        return Err(anyhow::anyhow!("adapter: an unfilled hole at {hole}"));
    }
    funcs.push_str("  (func $p3_errno (param $c i32) (result i32)\n");
    funcs.push_str(&case_arms(resolve, fs_type(resolve, "error-code")?, P1_ERRNO_BY_CASE, true)?);
    funcs.push_str("    (i32.const 29))\n");
    funcs.push_str("  (func $p3_ftype (param $c i32) (result i32)\n");
    funcs.push_str(&case_arms(resolve, fs_type(resolve, "descriptor-type")?, P1_FILETYPE_BY_CASE, false)?);
    funcs.push_str("    (i32.const 0))\n");
    Ok(funcs)
}

/// The whole service WAT for the p3 component: the p1 template for `ops`
/// with its preview-1 import lines replaced by the adapter's imports and the
/// adapter's functions placed ahead of the dispatcher. `prefetch` adds the
/// three pseudo-op arms the fan triple calls (-1 resolve, -2 UTF-8 check,
/// -3 regular-file check).
fn p3_service_wat(fsp: u32, ops: &[i32], funcs: &str, prefetch: bool) -> anyhow::Result<String> {
    let arms = if prefetch {
        "    (if (i32.eq (local.get $op) (i32.const -1)) (then (return (call $p3_resolve_op (local.get $a) (local.get $al)))))\n    \
         (if (i32.eq (local.get $op) (i32.const -2)) (then (return (call $p3_utf8_op (local.get $a) (local.get $al)))))\n    \
         (if (i32.eq (local.get $op) (i32.const -3)) (then (return (call $p3_streamable_op (local.get $a)))))\n"
    } else {
        ""
    };
    let base = service_wat_ext(fsp, ops, arms);
    let (imports, _) = P3_FS_ADAPTER
        .split_once(";; @@FUNCS@@")
        .ok_or_else(|| anyhow::anyhow!("adapter: no @@FUNCS@@ marker"))?;
    let mut out = String::with_capacity(base.len() + P3_FS_ADAPTER.len());
    let (mut imported, mut placed) = (false, false);
    for line in base.lines() {
        if line.trim_start().starts_with("(import \"wasi_snapshot_preview1\"") {
            if !imported {
                out.push_str(imports);
                imported = true;
            }
            continue;
        }
        if line.starts_with("  (func $fs (export \"fs\")") {
            out.push_str(funcs);
            placed = true;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !(imported && placed) {
        return Err(anyhow::anyhow!("the fs service template moved: no preview-1 imports or no dispatcher"));
    }
    Ok(out)
}

/// The planned splice for one artifact.
struct P3Fs {
    splice: FsSplice,
    /// The ops `shim_fs_call` forwards to the dispatcher.
    forwarded: Vec<i32>,
}

impl P3Fs {
    /// The service for `host_ops`, its page at `fsp`, or `None` when no fs
    /// op is reached. `has(module, name)`: the import the artifact already
    /// carries — only the others are appended.
    fn plan(
        host_ops: &[i32],
        fsp: u32,
        resolve: &wit_parser::Resolve,
        abi: &FsAbi,
        has: &dyn Fn(&str, &str) -> bool,
    ) -> anyhow::Result<Option<Self>> {
        let ops = p3_service_ops(host_ops);
        if ops.is_empty() {
            return Ok(None);
        }
        let prefetch = ops.iter().any(|op| (40..=42).contains(op));
        let wat = p3_service_wat(fsp, &ops, &adapter_funcs(resolve, abi)?, prefetch)?;
        let shared = |module: &str, name: &str| {
            (module == "shim" && matches!(name, "fs_call" | "await" | "alloc" | "reserve")) || has(module, name)
        };
        let splice = FsSplice::from_wat(&wat, ops.clone(), &shared)?;
        Ok(Some(Self { splice, forwarded: p3_forwarded_ops(&ops) }))
    }
}

/// Where the service's imports and globals land in the p3 module.
struct P3FsTargets<'a> {
    /// Every import block already laid out (base, http, env).
    blocks: &'a [&'a [(u32, &'a str, &'a str, u32)]],
    /// (`fs_call`, `$await`, `$alloc`, `$reserve`) — the shims the adapter calls.
    shims: (u32, u32, u32, u32),
}

impl P3FsTargets<'_> {
    fn at(&self, module: &str, name: &str) -> Option<u32> {
        if module == "shim" {
            let (fs_call, wait, alloc, reserve) = self.shims;
            return match name {
                "fs_call" => Some(fs_call),
                "await" => Some(wait),
                "alloc" => Some(alloc),
                "reserve" => Some(reserve),
                _ => None,
            };
        }
        self.blocks
            .iter()
            .flat_map(|b| b.iter())
            .find(|(_, m, n, _)| *m == module && *n == name)
            .map(|(at, ..)| *at)
    }
}

/// Append the service's fresh imports after `next_import` and answer where
/// every service import lands.
fn p3_fs_imports(
    fs: &P3Fs,
    imports: &mut ImportSection,
    types: &FsTypes,
    next_import: &mut u32,
    to: &P3FsTargets<'_>,
) -> Vec<u32> {
    fs.splice.import_mapped(imports, types, next_import, &|m, n| to.at(m, n))
}

/// Emit the shipped service bodies and statics (nothing when no service
/// ships). `env`: the environment cache globals the adapter shares with the
/// env service.
fn p3_fs_emit(
    fs: Option<((&P3Fs, &FsTypes), &SpliceTargets)>,
    code: &mut CodeSection,
    data: &mut wasm_encoder::DataSection,
    env: (u32, u32),
) -> anyhow::Result<()> {
    match fs {
        Some(((fs, types), to)) => fs.splice.emit_with_globals(code, data, types, to, &[("env", env.0), ("envn", env.1)]),
        None => Ok(()),
    }
}
