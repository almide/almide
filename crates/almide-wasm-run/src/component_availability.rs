//! Validate emitted operations against the selected direct component shim.
//! Preview1 availability does not imply availability in a component world.

/// Whether the direct component shim serves `op`. Both worlds serve stdio,
/// entropy, the clocks and panic; p3 adds env.get (26), the program
/// arguments (29) and env.sleep_ms (36) over wasi:cli/environment and
/// monotonic-clock.wait-for (ADR-0023 step 3), env.set (37) through the
/// guest-side overlay env.get reads first (#3223), the http client
/// (43..=50), and — through the spliced p1 fs service over its
/// wasi:filesystem@0.3 adapter (#3140) — every op the p1 fs service
/// answers, the fan prefetch triple (40..=42) and env.os / env.temp_dir /
/// env.cwd (27 / 28 / 33) among them.
pub fn serves(op: i32, p3: bool) -> bool {
    // 74: the instance-parallel fan offer (#3003), answered "not served" by
    // both component shims — the guest then runs its chunks sequentially.
    // 76: a deliberate exit's mark (#4203), answered 0.
    let common = matches!(op, 30..=32 | 34..=35 | 60 | 73 | 74 | 76);
    let extra = p3
        && (matches!(op, 26 | 29 | 36 | 37 | 40..=50 | 65)
            || crate::wasi::FS_SERVICE_OPS.iter().any(|(o, _, _)| *o == op));
    common || extra
}

/// The p1-served ops the p3 component does NOT serve, each with the reason
/// (#3140's gate: the p3 served set covers `P1_SERVED_OPS` minus exactly
/// these; `tests` below hold both directions). Since #3223 only the
/// subprocess family (#2589): the p1 core module carries it as the private
/// `almide:process/spawn` import, and no component world declares that
/// interface (ADR-0025).
const PROC_EXCLUDED: &str = "the private almide:process/spawn import has no place in a component world (ADR-0025)";
pub const P3_EXCLUDED_P1_OPS: &[(i32, &str)] = &[
    (80, PROC_EXCLUDED),
    (81, PROC_EXCLUDED),
    (82, PROC_EXCLUDED),
    (83, PROC_EXCLUDED),
    (84, PROC_EXCLUDED),
    (85, PROC_EXCLUDED),
    (86, PROC_EXCLUDED),
    (87, PROC_EXCLUDED),
    (88, PROC_EXCLUDED),
    (89, PROC_EXCLUDED),
    (90, PROC_EXCLUDED),
];

/// The ops the WASI 0.3 shim serves and the embedded core-module host does
/// not: ADR-0023 step 4 put them in the shim only (op 65, the full-response
/// client), and decision 1 forbids a host-served twin. On the embedded lane a
/// program naming one runs as its p3 component (step 7, `run_component`),
/// whose wasi:http transport is native's rt-core client. Every other program
/// keeps the core-module route until step 7's full switch, which waits on the
/// call handle in the shim (ops 53..=59) and on exit codes above 1, which
/// the p3 shim's `wasi:cli/exit` cannot carry (C-350).
pub const COMPONENT_ONLY_OPS: &[i32] = &[65];

/// Whether `almide run --target wasm` runs this op set as the p3 component:
/// it names a component-only op and the p3 shim serves all of it.
pub fn embedded_runs_component(host_ops: &[i32]) -> bool {
    host_ops.iter().any(|op| COMPONENT_ONLY_OPS.contains(op)) && check(host_ops, true).is_ok()
}

/// The refusal for a program no single route of the embedded lane serves: it
/// names a component-only op AND one the p3 shim does not serve, so neither
/// the component nor the core-module host (which has no op 65, decision 1)
/// can run all of it.
pub fn embedded_split(host_ops: &[i32]) -> Option<String> {
    let only = host_ops.iter().copied().find(|op| COMPONENT_ONLY_OPS.contains(op))?;
    let other = host_ops.iter().copied().find(|op| !serves(*op, true))?;
    Some(format!(
        "error[E081]: {} (host op {only}) runs only as the WASI 0.3 component and {} (host op {other}) only on the \
         embedded core-module host, so no wasm route serves this program\n  \
         reason: ADR-0023 decision 1 keeps the full-response client out of the core host, and the component world \
         has no import for host op {other}\n  \
         note: the native target serves both",
        operation_label(only),
        operation_label(other),
    ))
}

/// Reject an artifact before writing it when its direct shim cannot serve it.
/// P3 HTTP imports are selected separately whenever an HTTP operation is emitted.
pub fn check(host_ops: &[i32], p3: bool) -> Result<(), String> {
    match host_ops.iter().copied().find(|op| !serves(*op, p3)) {
        Some(op) => Err(format!(
            "error[E081]: {} (host op {op}) is unavailable in the direct WASI {} component. \
             Use a target that serves this operation; `almide run --target wasm` uses the embedded host.",
            operation_label(op), if p3 { "0.3" } else { "0.2" },
        )),
        None => Ok(()),
    }
}

/// Whether the p3 SERVICE shim (the stock serve export, #2659, C-375) serves
/// `op`: the p3 command shim's set without the filesystem — the service
/// world imports no `wasi:filesystem`, which `wasmtime serve` does not link
/// without a flag (#2659, measured on wasmtime 47) — plus `http.serve`'s
/// own ops 70..=72 and 75 (the contained loop, #4203, answered "not
/// served"), which the shim answers guest-side.
pub fn serves_service(op: i32) -> bool {
    let fs = matches!(op, 40..=42) || crate::wasi::FS_SERVICE_OPS.iter().any(|(o, _, _)| *o == op);
    matches!(op, 70..=72 | 75) || (!fs && serves(op, true))
}

/// [`check`] for the service shape: an op the export cannot answer is E081
/// with the reason the service world gives.
pub fn check_service(host_ops: &[i32]) -> Result<(), String> {
    match host_ops.iter().copied().find(|op| !serves_service(*op)) {
        Some(op) => Err(format!(
            "error[E081]: {} (host op {op}) is unavailable in the stock serve export\n  \
             reason: an http.serve program builds as a wasi:http/handler@0.3.0 component, whose world imports no \
             wasi:filesystem (`wasmtime serve` links none without a flag) and no capability beyond the WASI 0.3 service \
             world (#2659)\n  \
             note: `almide run --target wasm` and the native target serve it",
            operation_label(op),
        )),
        None => Ok(()),
    }
}

/// `operation_name`, with the subprocess family (#2589) named as one.
fn operation_label(op: i32) -> &'static str {
    if (80..=90).contains(&op) { "the subprocess family (process.exec / exec_status / spawn / kill / …)" } else { operation_name(op) }
}

fn operation_name(op: i32) -> &'static str {
    const NAMES: &[&str] = &[
        "unknown operation", "fs.read_text", "fs.write_text", "fs.write_bytes",
        "fs.exists", "fs.is_dir", "fs.is_file", "fs.mkdir_p", "fs.remove",
        "fs.remove_all", "fs.create_temp_dir", "fs.list_dir", "fs.read_lines",
        "fs.read_text_if_exists", "fs.read_bytes", "fs.write_bytes", "fs.append_text",
        "fs.file_size", "fs.modified_at", "fs.copy", "fs.rename",
        "fs.create_temp_file", "fs.is_symlink", "fs.walk", "fs.read_lines_if_exists",
        "fs.read_bytes_if_exists", "env.get", "env.os", "env.temp_dir",
        "env.args / process.args (also used by args.option and other args helpers)",
        "io.write", "io.read_all", "random", "env.cwd", "time.now",
        "io.read_byte / io.read_line / io.read_line_opt", "env.sleep_ms", "env.set",
    ];
    usize::try_from(op).ok().and_then(|index| NAMES.get(index)).copied()
        .unwrap_or(match op {
            38 => "fs.stat", 39 => "fs.glob",
            40..=42 => "filesystem fan prefetch",
            43 => "http.get", 44 => "http.post", 45 => "http.put",
            46 => "http.patch", 47 => "http.delete",
            48..=50 | 65 => "the framed http client (http.request, its status / bytes shapes, the *_response family)",
            51 => "fs.fold_lines", 52 => "fs.for_each_line",
            53..=59 => "the http call handle (http.start / poll / read_new / wait / cancel)",
            60 => "datetime.monotonic_ns",
            61 => "fs.fold_lines_range", 62 => "fs.fold_lines_chunked",
            63 => "fs.read_bytes", 64 => "fs.read_bytes_if_exists",
            73 => "panic",
            91..=103 => "the net core (net.tcp_*)",
            // Interim embedded-only ops (#3620): WASI has neither Unix-domain
            // sockets nor shared memory; they retire at ADR-0023 step 7.
            104..=116 => "the unix-socket and shared-memory core (net.unix_* / net.shm_*)",
            _ => "unknown operation",
        })
}

#[cfg(test)]
mod tests {
    use super::{embedded_runs_component, serves, COMPONENT_ONLY_OPS, P3_EXCLUDED_P1_OPS};

    /// A component-only op is one the p3 shim serves and the p1 shim does
    /// not (the embedded core host serves the p1 surface and more, never
    /// less); the embedded route takes it to the component only when the
    /// shim serves the whole op set.
    #[test]
    fn component_only_ops_are_p3_ops_the_route_sends_to_the_component() {
        for op in COMPONENT_ONLY_OPS {
            assert!(serves(*op, true), "component-only op {op} is not served on p3");
            assert!(!crate::wasi::P1_SERVED_OPS.contains(op), "component-only op {op} is p1-served");
            assert!(embedded_runs_component(&[1, *op]), "op {op} with fs.read_text stays on the core route");
            assert!(!embedded_runs_component(&[53, *op]), "op {op} with the call handle left the core route");
        }
        assert!(!embedded_runs_component(&[43, 48]), "a program the core host serves left the core route");
    }

    /// #3140's exit gate: a program that builds as a p1 core module also
    /// builds as a p3 component — the p3 served set covers every p1-served op
    /// but the declared exclusions, and each exclusion carries a reason.
    #[test]
    fn p3_serves_every_p1_op_but_the_declared_exclusions() {
        for op in crate::wasi::P1_SERVED_OPS {
            let excluded = P3_EXCLUDED_P1_OPS.iter().any(|(o, _)| o == op);
            assert!(serves(*op, true) || excluded, "p1 serves host op {op} and p3 neither serves nor excludes it");
        }
    }

    /// The other direction: an exclusion is a p1 op p3 really refuses, with
    /// a reason — a served or never-p1 op listed here is stale.
    #[test]
    fn every_p3_exclusion_is_a_refused_p1_op_with_a_reason() {
        for (op, why) in P3_EXCLUDED_P1_OPS {
            assert!(crate::wasi::P1_SERVED_OPS.contains(op), "exclusion {op} is not a p1-served op");
            assert!(!serves(*op, true), "exclusion {op} is served on p3: drop it");
            assert!(!why.trim().is_empty(), "exclusion {op} has no reason");
        }
    }
}
