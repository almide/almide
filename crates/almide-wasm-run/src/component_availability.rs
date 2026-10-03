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
    let common = matches!(op, 30..=32 | 34..=35 | 60 | 73);
    let extra = p3
        && (matches!(op, 26 | 29 | 36 | 37 | 40..=50)
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
];

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

/// `operation_name`, with the subprocess family (#2589) named as one.
fn operation_label(op: i32) -> &'static str {
    if (80..=89).contains(&op) { "the subprocess family (process.exec / exec_status / spawn / kill / …)" } else { operation_name(op) }
}

fn operation_name(op: i32) -> &'static str {
    const NAMES: &[&str] = &[
        "unknown operation", "fs.read_text", "fs.write", "fs.write_bytes",
        "fs.exists", "fs.is_dir", "fs.is_file", "fs.mkdir_p", "fs.remove",
        "fs.remove_all", "fs.create_temp_dir", "fs.list_dir", "fs.read_lines",
        "fs.read_text_if_exists", "fs.read_bytes", "fs.write_bytes", "fs.append",
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
            48..=50 => "http.request framed response",
            51 => "fs.fold_lines", 52 => "fs.for_each_line",
            53..=59 => "the http call handle (http.start / poll / read_new / wait / cancel)",
            60 => "datetime.monotonic_ns",
            61 => "fs.fold_lines_range", 62 => "fs.fold_lines_chunked",
            63 => "fs.read_bytes_raw", 64 => "fs.read_bytes_raw_if_exists",
            73 => "panic",
            _ => "unknown operation",
        })
}

#[cfg(test)]
mod tests {
    use super::{serves, P3_EXCLUDED_P1_OPS};

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
