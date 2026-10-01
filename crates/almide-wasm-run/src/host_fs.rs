// The `almide.fs_call` op dispatch of the embedded host: the fs, env, http
// and random ops a structural wasm guest crosses to, each answering
// `(status<<32 | len, buffer)`. `include!`d by host.rs (split out for the
// 800-line file budget); everything here is host.rs's module scope.

/// io_err — VERBATIM the native runtime's formatting, so error strings
/// (`fs.read_text("/nope/x"): No such file or directory (os error 2)`) match by
/// construction. The twin lives in `runtime/rs/src/fs.rs`; #2090 gave both the
/// call name and the operand, and they move together or C-215 breaks.
///
/// The structural wasm leg crosses `almide.fs_call` to THIS host for every
/// `fs.*` call, so this file — not the generated WAT — is where the default
/// `--target wasm` message is built.
fn io_err(call: &str, args: &str, e: impl std::fmt::Display) -> String {
    format!("{call}({args}): {e}")
}
/// Source-shaped quoting — the twin of `fs.rs`'s `q`. Deliberately not `{:?}`;
/// see that file for why the escape table is not reproduced.
fn q(s: &str) -> String {
    format!("\"{s}\"")
}
/// The Almide call an fs op came from — ONE table with the p1 fs service
/// (`almide_wasi::fs_op_name`), so the stock-runtime artifact and this host
/// cannot spell a call two ways (#2090, #2742).
use crate::wasi::fs_op_name;

/// Length-prefixed string frames (u32 LE + bytes) — the list-of-strings
/// result encoding the guest decoder walks.
fn frames(names: &[String]) -> Vec<u8> {
    let mut b = Vec::new();
    for n in names {
        b.extend_from_slice(&(n.len() as u32).to_le_bytes());
        b.extend_from_slice(n.as_bytes());
    }
    b
}

/// status<<32 | len: 0 = ok, 1 = err (buffer holds the message), 2 =
/// ok-none (the *_if_exists shapes). `flag` rides len for bool ops.
/// Parse the http_framed cell frame (#1710 increment 3): decimal
/// CHAR-count lengths (string.len semantics — the guest counts chars,
/// so this parser walks chars, not bytes), `<len>\n<payload>` cells:
/// method, body, then key/value pairs until the frame ends.
type HttpFrame = (String, String, Vec<(String, String)>);

fn parse_http_frame(frame: &str) -> Result<HttpFrame, String> {
    fn cell(rest: &str) -> Result<(String, &str), String> {
        let nl = rest.find('\n').ok_or_else(|| "malformed http frame (missing length)".to_string())?;
        let n: usize = rest[..nl].parse().map_err(|_| "malformed http frame (bad length)".to_string())?;
        let tail = &rest[nl + 1..];
        if tail.chars().count() < n {
            return Err("malformed http frame (short cell)".to_string());
        }
        let byte_end = tail.char_indices().nth(n).map(|(i, _)| i).unwrap_or(tail.len());
        Ok((tail[..byte_end].to_string(), &tail[byte_end..]))
    }
    let (method, rest) = cell(frame)?;
    let (body, mut rest) = cell(rest)?;
    let mut headers = Vec::new();
    while !rest.is_empty() {
        let (k, r1) = cell(rest)?;
        let (v, r2) = cell(r1)?;
        headers.push((k, v));
        rest = r2;
    }
    Ok((method, body, headers))
}

fn pack(status: i64, len: usize) -> i64 {
    (status << 32) | (len as i64 & 0xFFFF_FFFF)
}

/// The WRITE-side ops — split from fs_dispatch for the complexity
/// budget. Bodies verbatim from the native runtime (io_err = Display).
fn fs_dispatch_w(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    use std::path::Path;
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    let unit = |r: Result<(), String>| match r {
        Ok(()) => (pack(0, 0), Vec::new()),
        Err(m) => err_s(m),
    };
    match op {
        2 | 15 => unit(std::fs::write(a, b).map_err(|e| io_err(fs_op_name(op), &q(a), e))),
        // write_bytes: b is the guest List[Int] payload — i64 LE slots,
        // low byte each (native `x as u8`).
        3 => {
            let (slots, _) = b.as_chunks::<8>();
            let data: Vec<u8> = slots.iter().map(|c| i64::from_le_bytes(*c) as u8).collect();
            unit(std::fs::write(a, &data).map_err(|e| io_err(fs_op_name(op), &q(a), e)))
        }
        7 => unit(std::fs::create_dir_all(a).map_err(|e| io_err(fs_op_name(op), &q(a), e))),
        8 => {
            let p = Path::new(a);
            unit(if p.is_dir() {
                std::fs::remove_dir(a).map_err(|e| io_err(fs_op_name(op), &q(a), e))
            } else {
                std::fs::remove_file(a).map_err(|e| io_err(fs_op_name(op), &q(a), e))
            })
        }
        9 => {
            let p = Path::new(a);
            unit(if p.is_dir() {
                std::fs::remove_dir_all(a).map_err(|e| io_err(fs_op_name(op), &q(a), e))
            } else {
                std::fs::remove_file(a).map_err(|e| io_err(fs_op_name(op), &q(a), e))
            })
        }
        _ => unit(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(a)
                .and_then(|mut f| std::io::Write::write_all(&mut f, b))
                .map_err(|e| io_err(fs_op_name(op), &q(a), e)),
        ),
    }
}

/// The structured READ ops (temp dir / list_dir / read_lines /
/// if-exists / read_bytes) — split for the complexity budget.
fn fs_dispatch_r2(op: i32, a: &str) -> (i64, Vec<u8>) {
    use std::path::Path;
    let ok_text = |t: String| (pack(0, t.len()), t.into_bytes());
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    match op {
        10 => {
            let dir = std::env::temp_dir();
            let name = format!(
                "{}{}",
                a,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let path = dir.join(&name);
            match std::fs::create_dir_all(&path).map_err(|e| io_err(fs_op_name(op), &q(&path.to_string_lossy()), e)) {
                Ok(()) => ok_text(path.to_string_lossy().replace('\\', "/")),
                Err(m) => err_s(m),
            }
        }
        11 => match std::fs::read_dir(a) {
            Ok(entries) => {
                let mut names = Vec::new();
                for entry in entries {
                    match entry {
                        Ok(e) => names.push(e.file_name().to_string_lossy().to_string()),
                        Err(e) => return err_s(io_err(fs_op_name(op), &q(a), e)),
                    }
                }
                names.sort();
                let buf = frames(&names);
                (pack(0, buf.len()), buf)
            }
            Err(e) => err_s(io_err(fs_op_name(op), &q(a), e)),
        },
        // 51/52 (fold_lines / for_each_line) are op 12's body under their own
        // name (#2090). They MUST share this arm: the `_` fallthrough below is the
        // raw-BYTES reader, and routing them there fed the guest bytes where it
        // decodes length-prefixed frames — `fs.fold_lines` then summed an empty
        // walk and answered 0 instead of 6, silently.
        12 | 51 | 52 => match std::fs::read_to_string(a) {
            Ok(t) => {
                let lines: Vec<String> = t.lines().map(str::to_string).collect();
                let buf = frames(&lines);
                (pack(0, buf.len()), buf)
            }
            Err(e) => err_s(io_err(fs_op_name(op), &q(a), e)),
        },
        13 => {
            if Path::new(a).exists() {
                match std::fs::read_to_string(a) {
                    Ok(t) => ok_text(t),
                    Err(e) => err_s(io_err(fs_op_name(op), &q(a), e)),
                }
            } else {
                (pack(2, 0), Vec::new())
            }
        }
        _ => match std::fs::read(a) {
            Ok(bytes) => (pack(0, bytes.len()), bytes),
            Err(e) => err_s(io_err(fs_op_name(op), &q(a), e)),
        },
    }
}

/// The metadata / host-environment ops (17+) — split for the
/// complexity budget. Bodies verbatim from the native runtime.
fn fs_dispatch_meta(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    use std::path::Path;
    match op {
        // 62 is fold_lines_chunked's size probe: op 17's body under the
        // chunked call's name (#2744), native's `metadata` before the workers.
        17 | 62 => match std::fs::metadata(a) {
            Ok(m) => fs_ok_i64(m.len() as i64),
            Err(e) => fs_err(io_err(fs_op_name(op), &q(a), e)),
        },
        18 => fs_mtime(op, a),
        19 | 20 => fs_copy_or_rename(op, a, b),
        21 => fs_temp_file(a),
        22 => (pack(0, usize::from(Path::new(a).is_symlink())), Vec::new()),
        23 => fs_walk_sorted(a),
        38 => fs_stat(op, a),
        39 => fs_glob(a),
        24 => fs_read_lines(a),
        // 64 is `read_bytes_raw_if_exists`: op 25's body under its own name
        // (#2890).
        25 | 64 => match std::fs::read(a) {
            Ok(bytes) => (pack(0, bytes.len()), bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (pack(2, 0), Vec::new()),
            Err(e) => fs_err(io_err(fs_op_name(op), &q(a), e)),
        },
        _ => fs_dispatch_host(op, a, b),
    }
}

/// An err answer: the buffer holds the message.
fn fs_err(m: String) -> (i64, Vec<u8>) {
    (pack(1, m.len()), m.into_bytes())
}

/// An ok answer carrying one i64 LE.
fn fs_ok_i64(v: i64) -> (i64, Vec<u8>) {
    (pack(0, 8), v.to_le_bytes().to_vec())
}

/// op 18: the modification time in Unix seconds.
fn fs_mtime(op: i32, a: &str) -> (i64, Vec<u8>) {
    match std::fs::metadata(a)
        .map_err(|e| io_err(fs_op_name(op), &q(a), e))
        .and_then(|m| m.modified().map_err(|e| io_err(fs_op_name(op), &q(a), e)))
    {
        Ok(t) => fs_ok_i64(t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64),
        Err(m) => fs_err(m),
    }
}

/// ops 19 / 20: copy / rename `a` to the path in `b`.
fn fs_copy_or_rename(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    let to = String::from_utf8_lossy(b);
    let r = if op == 19 { std::fs::copy(a, to.as_ref()).map(|_| ()) } else { std::fs::rename(a, to.as_ref()) };
    match r {
        Ok(()) => (pack(0, 0), Vec::new()),
        Err(e) => fs_err(io_err(fs_op_name(op), &format!("{}, {}", q(a), q(&to)), e)),
    }
}

/// fs.stat (#1423 stage 4): the four FileStat fields as i64 LE — native's
/// almide_rt_fs_stat field for field (modified = Unix seconds, 0 when the
/// host cannot say).
fn fs_stat(op: i32, a: &str) -> (i64, Vec<u8>) {
    match std::fs::metadata(a) {
        Ok(m) => {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let mut buf = Vec::with_capacity(32);
            for v in [m.len() as i64, i64::from(m.is_dir()), i64::from(m.is_file()), modified] {
                buf.extend_from_slice(&v.to_le_bytes());
            }
            (pack(0, buf.len()), buf)
        }
        Err(e) => fs_err(io_err(fs_op_name(op), &q(a), e)),
    }
}

/// op 24: read a text file as framed lines (none-class on NotFound).
fn fs_read_lines(path: &str) -> (i64, Vec<u8>) {
    match std::fs::read_to_string(path) {
        Ok(t) => {
            let lines: Vec<String> = t.lines().map(str::to_string).collect();
            let buf = frames(&lines);
            (pack(0, buf.len()), buf)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (pack(2, 0), Vec::new()),
        Err(e) => {
            let m = io_err("fs.read_lines_if_exists", &q(path), e);
            (pack(1, m.len()), m.into_bytes())
        }
    }
}

/// op 21: create an empty uniquely-named temp file, return its path.
fn fs_temp_file(prefix: &str) -> (i64, Vec<u8>) {
    let name = format!(
        "{}{}",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let path = std::env::temp_dir().join(&name);
    match std::fs::write(&path, "")
        .map_err(|e| io_err("fs.create_temp_file", &q(&path.to_string_lossy()), e))
    {
        Ok(()) => {
            let t = path.to_string_lossy().replace('\\', "/");
            (pack(0, t.len()), t.into_bytes())
        }
        Err(m) => (pack(1, m.len()), m.into_bytes()),
    }
}

/// op 23: recursive directory listing, sorted, framed.
/// fs.glob (#1423 stage 4): the SEGMENT-WISE matcher of C-228, transcribed
/// from runtime/rs/src/fs.rs (almide_rt_fs_glob / glob_walk /
/// glob_segs_match / glob_star_match) so the embedded host answers the
/// same list, in the same order, with the same walk errors as native.
fn fs_glob(pattern: &str) -> (i64, Vec<u8>) {
    use std::path::Path;
    fn walk(dir: &Path, rel_prefix: &str, depth: Option<usize>, out: &mut Vec<String>) -> Result<(), String> {
        for entry in
            std::fs::read_dir(dir).map_err(|e| io_err("fs.glob", &q(&dir.to_string_lossy()), e))?
        {
            let entry = entry.map_err(|e| io_err("fs.glob", &q(&dir.to_string_lossy()), e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if rel_prefix.is_empty() { name } else { format!("{rel_prefix}/{name}") };
            let path = entry.path();
            let descend = depth != Some(1) && path.is_dir();
            out.push(rel.clone());
            if descend {
                walk(&path, &rel, depth.map(|d| d - 1), out)?;
            }
        }
        Ok(())
    }
    fn segs_match(pats: &[&str], segs: &[&str]) -> bool {
        match pats.first() {
            None => segs.is_empty(),
            Some(&"**") => (0..=segs.len()).any(|i| segs_match(&pats[1..], &segs[i..])),
            Some(pat) => {
                !segs.is_empty() && star_match(pat, segs[0]) && segs_match(&pats[1..], &segs[1..])
            }
        }
    }
    fn star_match(pat: &str, seg: &str) -> bool {
        let parts: Vec<&str> = pat.split('*').collect();
        if parts.len() == 1 {
            return pat == seg;
        }
        let (first, last) = (parts[0], parts[parts.len() - 1]);
        if seg.len() < first.len() + last.len() || !seg.starts_with(first) || !seg.ends_with(last) {
            return false;
        }
        let region = &seg[first.len()..seg.len() - last.len()];
        let mut pos = 0;
        for part in &parts[1..parts.len() - 1] {
            if part.is_empty() {
                continue;
            }
            match region[pos..].find(part) {
                Some(i) => pos += i + part.len(),
                None => return false,
            }
        }
        true
    }
    let ok_list = |results: Vec<String>| {
        let buf = frames(&results);
        (pack(0, buf.len()), buf)
    };
    let absolute = pattern.starts_with('/');
    let segs: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let k = segs.iter().take_while(|s| !s.contains('*')).count();
    let pats = &segs[k..];
    let base = format!("{}{}", if absolute { "/" } else { "" }, segs[..k].join("/"));
    if pats.is_empty() {
        let hit = !base.is_empty() && Path::new(&base).exists();
        return ok_list(if hit { vec![base] } else { Vec::new() });
    }
    if !base.is_empty() && !Path::new(&base).is_dir() {
        return ok_list(Vec::new());
    }
    let root = if base.is_empty() { "." } else { base.as_str() };
    let prefix = if base.is_empty() || base == "/" { base.clone() } else { format!("{base}/") };
    let depth = if pats.contains(&"**") { None } else { Some(pats.len()) };
    let mut results = Vec::new();
    if !base.is_empty() && segs_match(pats, &[]) {
        results.push(base.clone());
    }
    let mut rels = Vec::new();
    if let Err(m) = walk(Path::new(root), "", depth, &mut rels) {
        return (pack(1, m.len()), m.into_bytes());
    }
    for rel in rels {
        let rsegs: Vec<&str> = rel.split('/').collect();
        if segs_match(pats, &rsegs) {
            results.push(format!("{prefix}{rel}"));
        }
    }
    results.sort();
    ok_list(results)
}

fn fs_walk_sorted(root: &str) -> (i64, Vec<u8>) {
    use std::path::Path;
    fn walk(dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
        for entry in
            std::fs::read_dir(dir).map_err(|e| io_err("fs.walk", &q(&dir.to_string_lossy()), e))?
        {
            let entry =
                entry.map_err(|e| io_err("fs.walk", &q(&dir.to_string_lossy()), e))?;
            let path = entry.path();
            out.push(path.to_string_lossy().replace('\\', "/"));
            if path.is_dir() {
                walk(&path, out)?;
            }
        }
        Ok(())
    }
    let mut results = Vec::new();
    match walk(Path::new(root), &mut results) {
        Ok(()) => {
            results.sort();
            let buf = frames(&results);
            (pack(0, buf.len()), buf)
        }
        Err(m) => (pack(1, m.len()), m.into_bytes()),
    }
}

/// The env.set overlay (op 37), scoped to ONE run: a native program's
/// `setenv` lives as long as its process, and every native test file is its
/// own process, so a run is the unit here too. It is thread-local because a
/// run executes on the thread that called `run_wasm_*` (the guest call is
/// synchronous), and `run_wasm_src` clears it before the module starts —
/// `almide test` runs many files in one process, concurrently on worker
/// threads and one after another on each (#3046), and a process-wide map let
/// one file's `env.set` leak into every later file's `env.get`.
fn with_env_overlay<R>(f: impl FnOnce(&mut std::collections::HashMap<String, String>) -> R) -> R {
    thread_local! {
        static OVERLAY: std::cell::RefCell<std::collections::HashMap<String, String>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    OVERLAY.with(|o| f(&mut o.borrow_mut()))
}

/// fs_dispatch_meta for the complexity budget.
fn fs_dispatch_host(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    match op {
        // Decomposed by op family (codopsy cc 38 -> per-family fns): the
        // http and env arms live in `fs_dispatch_http` / `fs_dispatch_env`.
        43..=50 => fs_dispatch_http(op, a, b),
        26 | 27 | 28 | 29 | 33 | 37 => fs_dispatch_env(op, a, b),
        // incremental stdin (op 35) — same empty answer in the harness.
        35 => (pack(0, 0), Vec::new()),
        32 => {
            let n = b.len();
            let mut seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
                | 1;
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                out.push(seed as u8);
            }
            (pack(0, out.len()), out)
        }
        _ => err_s(format!("unknown fs op {op}")),
    }
}

/// The http op family (43..=50) of `fs_dispatch_host`, verbatim.
fn fs_dispatch_http(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    match op {
        48..=50 => fs_http_framed(op, a, b),
        43..=47 => fs_http_plain(op, a, b),
        // env.set (#1423 bucket C ruling): key in a, value in b.
        _ => fs_err(format!("unknown fs op {op}")),
    }
}

fn fs_text_or_err(r: Result<String, String>) -> (i64, Vec<u8>) {
    match r {
        Ok(t) => (pack(0, t.len()), t.into_bytes()),
        Err(m) => fs_err(m),
    }
}

/// http string client (#1710 increment 1, ops 43..=47): url in a, body
/// (POST/PUT/PATCH) in b — THE native client transcribed (http_client.rs),
/// so error texts and framing match native byte-for-byte. Stock artifacts
/// never reach here (the build-path op audit refuses unserved ops); the
/// embedded lane serves them.
fn fs_http_plain(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    let method = match op {
        43 => "GET",
        44 => "POST",
        45 => "PUT",
        46 => "PATCH",
        _ => "DELETE",
    };
    let body = String::from_utf8_lossy(b).to_string();
    fs_text_or_err(almide_rt_core::http_client_core::request(method, a, &body, &[]))
}

/// The framed request family (#1710 increment 3, ops 48..=50): url in a, the
/// decimal CHAR-length frame in b — method cell, body cell, then header
/// key/value cells, `<len>\n<payload>` each, exactly as
/// stdlib/http_framed.almd builds it. 48 answers the body text, 49 answers
/// `<status>\n<body>`, 50 raw bytes.
fn fs_http_framed(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    use almide_rt_core::http_client_core as client;
    let frame = String::from_utf8_lossy(b).to_string();
    let (method, body, headers) = match parse_http_frame(&frame) {
        Ok(parts) => parts,
        Err(m) => return fs_err(m),
    };
    match op {
        48 => fs_text_or_err(client::request(&method, a, &body, &headers)),
        49 => fs_text_or_err(client::request_status(&method, a, &body, &headers).map(|(code, text)| format!("{code}\n{text}"))),
        _ => match client::request_bytes(&method, a, &body, &headers) {
            Ok(bytes) => (pack(0, bytes.len()), bytes),
            Err(m) => fs_err(m),
        },
    }
}

/// The env op family (26/27/28/29/33/37) of `fs_dispatch_host`, verbatim.
fn fs_dispatch_env(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    let ok_text = |t: String| (pack(0, t.len()), t.into_bytes());
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    match op {
        // env.get consults the env.set overlay FIRST (op 37): native's
        // process-level setenv makes a later get observe the set, and the
        // overlay reproduces that observable without std::env::set_var
        // (unsafe under threads — the wasmtime host runs multi-threaded).
        26 => match with_env_overlay(|o| o.get(a).cloned()) {
            Some(v) => ok_text(v),
            None => match std::env::var(a) {
                Ok(v) => ok_text(v),
                Err(_) => (pack(2, 0), Vec::new()),
            },
        },
        // http string client (#1710 increment 1, ops 43..=47): url in a,
        // body (POST/PUT/PATCH) in b — THE native client transcribed
        // (http_client.rs), so error texts and framing match native
        // byte-for-byte. Stock artifacts never reach here (the build-path
        // op audit refuses unserved ops); the embedded lane serves them.
        // The framed request family (#1710 increment 3, ops 48..=50):
        // url in a, the decimal CHAR-length frame in b — method cell,
        // body cell, then header key/value cells, `<len>\n<payload>`
        // each, exactly as stdlib/http_framed.almd builds it. 48 answers
        // the body text, 49 answers `<status>\n<body>`, 50 raw bytes.
        // env.set (#1423 bucket C ruling): key in a, value in b.
        37 => {
            let v = String::from_utf8_lossy(b).to_string();
            with_env_overlay(|o| o.insert(a.to_string(), v));
            (pack(0, 0), Vec::new())
        }
        27 => ok_text(std::env::consts::OS.to_string()),
        28 => ok_text(std::env::temp_dir().to_string_lossy().replace('\\', "/")),
        // args without a run context (#1716): [argv0] only — the fs_call
        // closure answers op 29 with the run's real args before dispatch
        // reaches here, so this arm is the no-Host fallback.
        // args without a run context (#1716): [argv0] only — the fs_call
        // closure answers op 29 with the run's real args before dispatch
        // reaches here, so this arm is the no-Host fallback.
        29 => {
            let buf = frames(&["wasm-harness".to_string()]);
            (pack(0, buf.len()), buf)
        }
        // stdin read (up to n = a bytes) — the harness has no stdin.
        // cwd — the same std::env the native runtime reads.
        33 => match std::env::current_dir() {
            Ok(p) => ok_text(p.to_string_lossy().replace('\\', "/")),
            Err(e) => err_s(io_err("env.cwd", "", e)),
        },
        // host entropy: n = b_len bytes from a seeded-by-time xorshift
        // (the range property is the only observable, C-112).
        _ => err_s(format!("unknown fs op {op}")),
    }
}

fn fs_dispatch(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    use std::path::Path;
    // The fan prefetch pair (#1628 increment 2b). The guest re-sends the
    // path at await, so the sequential host needs NO slot state: start is
    // a no-op and await IS the read — byte-identical to sequential fan.
    // (The p3 component's shim is where start actually goes async.)
    if op == 40 {
        return (pack(0, 0), Vec::new());
    }
    if op == 41 {
        return fs_dispatch(1, a, b);
    }
    if op == 42 {
        return (pack(0, 0), Vec::new());
    }
    if matches!(op, 2 | 3 | 7..=9 | 15 | 16) {
        return fs_dispatch_w(op, a, b);
    }
    // 51/52 are `fold_lines` / `for_each_line`: the SAME framed-lines body as
    // op 12, carrying their own name so the message matches native (#2090).
    // 63 is `read_bytes_raw`: op 14's bytes reader under its own name (#2890).
    if matches!(op, 10..=14 | 51 | 52 | 63) {
        return fs_dispatch_r2(op, a);
    }
    if op >= 17 && op != 61 {
        return fs_dispatch_meta(op, a, b);
    }
    let ok_text = |t: String| (pack(0, t.len()), t.into_bytes());
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    match op {
        // 61 is fold_lines_range (and fold_lines_chunked's worker read): op
        // 1's body under the range's name (#2744) — the guest walks the text's
        // byte ranges.
        1 | 61 => match std::fs::read_to_string(a) {
            Ok(t) => ok_text(t),
            Err(e) => err_s(io_err(fs_op_name(op), &q(a), e)),
        },
        4 => (pack(0, usize::from(Path::new(a).exists())), Vec::new()),
        5 => (pack(0, usize::from(Path::new(a).is_dir())), Vec::new()),
        6 => (pack(0, usize::from(Path::new(a).is_file())), Vec::new()),
        _ => err_s(format!("unknown fs op {op}")),
    }
}
