//! Validate emitted operations against the selected direct component shim.
//! Preview1 availability does not imply availability in a component world.

/// Does the op set reach `http.serve` (#2659): bind / accept / recv / send.
/// Such a program builds as the WASI 0.3 component — a p1 module has no
/// listening socket.
pub fn serves(host_ops: &[i32]) -> bool {
    crate::wasi::serves(host_ops.iter().copied())
}

/// Reject an artifact before writing it when its direct shim cannot serve it.
/// P3 HTTP imports are selected separately whenever an HTTP operation is emitted;
/// so are the sockets (http.serve) and environment (env.args) imports (#2659).
pub fn check(host_ops: &[i32], p3: bool) -> Result<(), String> {
    let unsupported = host_ops.iter().copied().find(|op| {
        let common = matches!(op, 30..=32 | 34..=35);
        let extra = p3 && matches!(op, 1..=9 | 13..=16 | 29 | 40..=50 | 70 | 73..=75);
        !(common || extra)
    });
    match unsupported {
        Some(op) => Err(format!(
            "error[E081]: {} (host op {op}) is unavailable in the direct WASI {} component. \
             Use a target that serves this operation; `almide run --target wasm` uses the embedded host.",
            operation_name(op), if p3 { "0.3" } else { "0.2" },
        )),
        None => Ok(()),
    }
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
            40..=42 => "filesystem fan prefetch",
            43 => "http.get", 44 => "http.post", 45 => "http.put",
            46 => "http.patch", 47 => "http.delete",
            48..=50 => "http.request framed response",
            53..=59 => "the http call handle (http.start / poll / read_new / wait / cancel)",
            70 | 73..=75 => "http.serve",
            _ => "unknown operation",
        })
}
