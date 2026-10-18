// fs extern — Rust native implementations (platform layer: native only, no WASM)

use std::path::Path;

// #2090 — a host call that fails at the syscall names ITSELF and the operand
// that identifies it, then the platform's own text verbatim as a SUFFIX:
//
//     fs.read_text("/nope/missing.txt"): No such file or directory (os error 2)
//
// Before this the path was in scope at every one of these sites and discarded at
// every one, so a missing FILE and a missing EXECUTABLE printed the same bytes.
//
// The suffix is the load-bearing half: C-215's errno classification and #1368's
// errno-carrying fix both read that tail, and a reader who recognises
// `(os error 2)` keeps recognising it.
//
// `crates/almide-wasm-run/src/host.rs` carries the twin of this pair, and its
// comment promises this formatting VERBATIM — the structural wasm leg crosses to
// that host for every `fs.*` call, so the two move together or C-215 breaks.
//
// The suffix is the platform's Display only where every compared host spells
// it alike. ENOTEMPTY, ENAMETOOLONG and EXDEV differ in number or text, so they
// answer the classified errno-free text of almide-base's one table (C-273):
// `fs.remove("d"): Directory not empty` on every host and every leg.
fn io_err(call: &str, args: &str, e: std::io::Error) -> String {
    format!("{call}({args}): {}", fs_io_error_text(&e))
}
include!("../../../crates/almide-base/src/fs_errno_classified.rs");
// Source-shaped quoting, so the operand reads back as the writer spelled it.
//
// DELIBERATELY not Rust's `{:?}`. Four independent implementations have to emit
// these bytes identically — this file, the embedded host, the interp VFS and the
// v0 leg's generated WAT — and `{:?}`'s escape table (`\"`, `\\`, `\n`,
// `\u{..}`) is a much larger surface to reproduce in hand-written WAT than a
// pair of quotes. For any path that is not pathological the two are byte-equal;
// for one that is, every leg still renders the SAME bytes, which is what C-215
// promises. Readability of a filename containing a quote is not worth a
// divergence none of the gates would catch.
// NAME-SPACED per runtime file: every `runtime/rs/src/*.rs` chunk is emitted
// WHOLE into one generated crate, so a program importing both `fs` and
// `process` gets both files' helpers side by side. A bare `q` in each is
// `error[E0428]: the name `q` is defined multiple times` —
// native_runtime_name_collision_pin_test exists for exactly this.
fn fs_q(s: &str) -> String { format!("\"{s}\"") }

// The runtime-side twin of stdlib/fs.almd's `type FileStat = { size, is_dir,
// is_file, modified }`: the bundled decl types the surface at check time, this
// struct is the value at run time. The emitter spells the type under this
// reserved name and skips the bundled decl (walker/runtime_owned.rs, #1821), so
// a user's own `type FileStat` keeps the bare spelling. The repr the emitted
// decl used to carry lives here and prints the Almide-level literal form.
#[derive(Clone, Debug, PartialEq)]
pub struct AlmideFileStat {
    pub size: i64,
    pub is_dir: bool,
    pub is_file: bool,
    pub modified: i64,
}
impl AlmideRepr for AlmideFileStat {
    fn almide_repr(&self) -> String {
        format!(
            "FileStat {{ size: {}, is_dir: {}, is_file: {}, modified: {} }}",
            self.size.almide_repr(), self.is_dir.almide_repr(), self.is_file.almide_repr(), self.modified.almide_repr()
        )
    }
}

// Read
pub fn almide_rt_fs_read_text(path: &str) -> Result<AlmideString, AlmideString> { let __almide_v: Result<String, AlmideString> = { let __almide_r: Result<String, String> = (|| { std::fs::read_to_string(path).map_err(|e| io_err("fs.read_text", &fs_q(path), e)) })(); __almide_r.map_err(Into::into) }; __almide_v.into_almide() }
pub fn almide_rt_fs_read_bytes(path: &str) -> Result<Vec<i64>, AlmideString> { let __almide_r: Result<Vec<i64>, String> = (|| { std::fs::read(path).map(|b| b.into_iter().map(|x| x as i64).collect()).map_err(|e| io_err("fs.read_bytes", &fs_q(path), e)) })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_fs_read_lines(path: &str) -> Result<Vec<AlmideString>, AlmideString> { let __almide_v: Result<Vec<String>, AlmideString> = { let __almide_r: Result<Vec<String>, String> = (|| { std::fs::read_to_string(path).map(|s| s.lines().map(|l| l.to_string()).collect()).map_err(|e| io_err("fs.read_lines", &fs_q(path), e)) })(); __almide_r.map_err(Into::into) }; __almide_v.into_almide() }

// Streaming line readers: fold/each over a BufReader, never materializing the
// file — peak memory is O(longest line), not O(file). Line semantics MUST
// byte-match read_lines' `.lines()`: split on \n, strip one trailing \r, no
// phantom empty line after a trailing newline, a final \n-less line still
// yielded. One deliberate divergence from read_lines: a mid-file read error
// (e.g. invalid UTF-8) surfaces AFTER the callbacks for earlier lines have
// already run — inherent to streaming.
pub fn almide_rt_fs_fold_lines<A>(path: &str, init: A, f: std::rc::Rc<dyn Fn(A, AlmideString) -> A>) -> Result<A, AlmideString> {
    let __almide_r: Result<A, String> = (|| {
        use std::io::BufRead;
        // The `open` and the per-line `read_line` are INNER steps of one call, so
        // both report `fs.fold_lines` — the walker the writer actually invoked.
        // Naming `read_line`, which they never called, would be worse than errno.
        let mut reader = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| io_err("fs.fold_lines", &fs_q(path), e))?);
        let mut acc = init;
        let mut buf = String::new();
        loop {
            buf.clear();
            if reader.read_line(&mut buf).map_err(|e| io_err("fs.fold_lines", &fs_q(path), e))? == 0 {
                return Ok(acc);
            }
            if buf.ends_with('\n') {
                buf.pop();
                if buf.ends_with('\r') {
                    buf.pop();
                }
            }
            // MEASURED (#1232, 2026-08-14): `clone` here beats `std::mem::take`, which
            // the ledger proposed as a free memcpy saving. `take` steals the String's
            // CAPACITY, so `read_line` regrows the buffer from zero every line (several
            // doublings for an ~80-byte line) where `clone` allocates one right-sized
            // copy and leaves `buf`'s capacity intact to reuse. 2M lines / 162 MB,
            // best-of-15, interleaved: 137 ms clone vs 143-146 ms take — the "saving"
            // is a 4-6% REGRESSION. Do not re-apply without re-measuring.
            acc = f(acc, AlmideString::from(buf.as_str()));
        }
    })();
    __almide_r.map_err(Into::into)
}
// Range-scoped fold: the chunk-parallel worker's walker. A line is owned by
// the chunk containing the byte BEFORE its first byte (line 0 by the chunk
// containing byte 0), so a partition of [0, filesize) processes every line
// exactly once: start>0 seeks to start-1 and discards through the first \n
// (if byte start-1 IS \n the discard consumes just it), then lines fold while
// their start offset is < end — the final line may read past end to finish.
fn fold_lines_range_impl<A, F: Fn(A, AlmideString) -> A + ?Sized>(path: &str, start: i64, end: i64, init: A, f: &F) -> Result<A, String> {
    use std::io::{BufRead, Seek};
    let mut file = std::fs::File::open(path).map_err(|e| io_err("fs.fold_lines_range", &fs_q(path), e))?;
    // #3652: a directory opens but fails its first read, and the empty range
    // (start 0, end <= 0) never reads — it answered ok(init) where every other
    // range, `fs.fold_lines` and the wasm leg give the read's err. Take that
    // same read's err up front, so the message is the one the other ranges give.
    if file.metadata().map(|m| m.is_dir()).unwrap_or(false) {
        use std::io::Read;
        file.read(&mut [0u8; 1]).map_err(|e| io_err("fs.fold_lines_range", &fs_q(path), e))?;
    }
    let mut pos: u64 = if start > 0 { (start - 1) as u64 } else { 0 };
    file.seek(std::io::SeekFrom::Start(pos)).map_err(|e| io_err("fs.fold_lines_range", &fs_q(path), e))?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = String::new();
    if start > 0 {
        let n = reader.read_line(&mut buf).map_err(|e| io_err("fs.fold_lines_range", &fs_q(path), e))?;
        if n == 0 {
            return Ok(init);
        }
        pos += n as u64;
    }
    let mut acc = init;
    while (pos as i64) < end {
        buf.clear();
        let n = reader.read_line(&mut buf).map_err(|e| io_err("fs.fold_lines_range", &fs_q(path), e))?;
        if n == 0 {
            break;
        }
        pos += n as u64;
        if buf.ends_with('\n') {
            buf.pop();
            if buf.ends_with('\r') {
                buf.pop();
            }
        }
        // MEASURED (#1232, 2026-08-14): `clone` here beats `std::mem::take`, which
        // the ledger proposed as a free memcpy saving. `take` steals the String's
        // CAPACITY, so `read_line` regrows the buffer from zero every line (several
        // doublings for an ~80-byte line) where `clone` allocates one right-sized
        // copy and leaves `buf`'s capacity intact to reuse. 2M lines / 162 MB,
        // best-of-15, interleaved: 137 ms clone vs 143-146 ms take — the "saving"
        // is a 4-6% REGRESSION. Do not re-apply without re-measuring.
        acc = f(acc, AlmideString::from(buf.as_str()));
    }
    Ok(acc)
}
// Raw `F: Fn` last arg (NOT `Rc<dyn Fn>`): build.rs derives the
// takes_raw_fn_last_arg set from this signature, so callbacks render unboxed.
pub fn almide_rt_fs_fold_lines_range<A, F: Fn(A, AlmideString) -> A>(path: &str, start: i64, end: i64, init: A, f: F) -> Result<A, AlmideString> { let __almide_r: Result<A, String> = (|| { fold_lines_range_impl(path, start, end, init, &f) })(); __almide_r.map_err(Into::into) }
// Chunk-parallel fold: the threads live HERE, inside the runtime — the fan
// machinery's purity gate keeps effectful bodies sequential, so file
// parallelism is the fs module's own intrinsic instead. Workers each fold
// their byte-range partition (fold_lines_range_impl's ownership rule makes
// the partition exact) on a scoped thread; the per-chunk partials return in
// CHUNK ORDER and merging stays with the caller, so results are
// deterministic whatever the thread schedule. The Send + Sync bounds hold
// only for an accumulator and callback with no `Rc` inside: RustLowering
// routes every other call to the `_seq` twin below (#3649).
pub fn almide_rt_fs_fold_lines_chunked<A: Clone + Send, F: Fn(A, AlmideString) -> A + Send + Sync>(path: &str, workers: i64, init: A, f: F) -> Result<Vec<A>, AlmideString> {
    init.almide_share();
    let __almide_r: Result<Vec<A>, String> = (|| {
        let ranges = fold_lines_chunk_ranges(path, workers)?;
        // Flush first: a worker's abort cannot reach this thread's buffer (C-197).
        almide_stdout_flush();
        // One thread per chunk was one thread per BYTE once `workers` passed the
        // file size, and the OS refusing the 4097th spawn crashed the run where
        // wasm answered ok. The chunks stay as they were; at most one thread per
        // core folds them, thread t taking chunks t, t+T, …, and each partial
        // lands back in its chunk's slot.
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        let threads = ranges.len().min(cores);
        let ranges = &ranges;
        std::thread::scope(|scope| {
            let f = &f;
            let mut handles = Vec::with_capacity(threads);
            for t in 0..threads {
                let init = init.clone();
                let spawned = std::thread::Builder::new().spawn_scoped(scope, move || {
                    almide_stack_guard_register();
                    ranges.iter().skip(t).step_by(threads).map(|&(s, e)| fold_lines_range_impl(path, s, e, init.clone(), f)).collect::<Vec<_>>()
                });
                handles.push(spawned.map_err(|e| format!("fold_lines_chunked: cannot start a worker: {e}"))?);
            }
            let mut slots: Vec<Option<Result<A, String>>> = (0..ranges.len()).map(|_| None).collect();
            for (t, h) in handles.into_iter().enumerate() {
                let parts = h.join().map_err(|_| "fold_lines_chunked: worker panicked".to_string())?;
                for (k, part) in parts.into_iter().enumerate() {
                    slots[t + k * threads] = Some(part);
                }
            }
            slots.into_iter().map(|p| p.expect("every chunk folded")).collect()
        })
    })();
    __almide_r.map_err(Into::into)
}
// The sequential twin (#3649): the same chunk partition, folded chunk by chunk
// on the calling thread. The stdlib signature `fold_lines_chunked[A]` bounds
// nothing, so a Matrix or Bytes accumulator, or a callback that captures one or
// is a closure VALUE (`Rc<dyn Fn>`), is a legal program that cannot cross a
// thread. Each chunk still folds from its own `init` and the partials return
// in chunk order, so the result is the one the threaded form gives. The
// callback is the uniform `Rc<dyn Fn>`, which a closure value already is.
// build.rs pairs a `Send`-bounded runtime fn with its `_seq` twin by name.
pub fn almide_rt_fs_fold_lines_chunked_seq<A: Clone>(path: &str, workers: i64, init: A, f: std::rc::Rc<dyn Fn(A, AlmideString) -> A>) -> Result<Vec<A>, AlmideString> {
    let __almide_r: Result<Vec<A>, String> = (|| {
        let ranges = fold_lines_chunk_ranges(path, workers)?;
        ranges.iter().map(|&(s, e)| fold_lines_range_impl(path, s, e, init.clone(), &*f)).collect()
    })();
    __almide_r.map_err(Into::into)
}
// The byte-range partition both chunked forms walk: `workers` (clamped to 1)
// equal chunks of [0, size), the empty tail chunks dropped. The non-empty
// count is computed, not filtered out of all `workers` candidates (2^31 of
// them for `workers = int.MAX`).
fn fold_lines_chunk_ranges(path: &str, workers: i64) -> Result<Vec<(i64, i64)>, String> {
    let size = std::fs::metadata(path).map_err(|e| io_err("fs.fold_lines_chunked", &fs_q(path), e))?.len() as i64;
    let w = workers.max(1);
    let chunk = size / w + 1;
    let count = (size + chunk - 1) / chunk;
    Ok((0..count).map(|i| i * chunk).map(|s| (s, (s + chunk).min(size))).collect())
}
pub fn almide_rt_fs_for_each_line(path: &str, f: std::rc::Rc<dyn Fn(AlmideString)>) -> Result<(), AlmideString> {
    let __almide_r: Result<(), String> = (|| {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| io_err("fs.for_each_line", &fs_q(path), e))?);
        let mut buf = String::new();
        loop {
            buf.clear();
            if reader.read_line(&mut buf).map_err(|e| io_err("fs.for_each_line", &fs_q(path), e))? == 0 {
                return Ok(());
            }
            if buf.ends_with('\n') {
                buf.pop();
                if buf.ends_with('\r') {
                    buf.pop();
                }
            }
            // MEASURED (#1232, 2026-08-14): `clone` here beats `std::mem::take`, which
            // the ledger proposed as a free memcpy saving. `take` steals the String's
            // CAPACITY, so `read_line` regrows the buffer from zero every line (several
            // doublings for an ~80-byte line) where `clone` allocates one right-sized
            // copy and leaves `buf`'s capacity intact to reuse. 2M lines / 162 MB,
            // best-of-15, interleaved: 137 ms clone vs 143-146 ms take — the "saving"
            // is a 4-6% REGRESSION. Do not re-apply without re-measuring.
            f(AlmideString::from(buf.as_str()));
        }
    })();
    __almide_r.map_err(Into::into)
}

// The ADR-0006 FALLIBLE forms of the two callback-driven walkers (#1144, the
// C-220 tracked cell). Same reader, same line semantics — the only difference
// is that the callback answers `Result` and the FIRST err ends the walk: the
// `?` returns before the next `read_line`, so the BufReader is dropped with
// the rest of the file unread. The err-stop point is therefore observable
// twice over: the callback is not invoked again, AND the reader consumed
// exactly through the failing line (plus its buffered readahead, which is not
// an observable). The checker routes `fs.fold_lines(p, z, (a, l) => g(a, l)!)`
// here by rewriting the callee to `fs.__fallible_fold_lines`.
pub fn almide_rt_fs_fold_lines_effect<A>(path: &str, init: A, f: std::rc::Rc<dyn Fn(A, AlmideString) -> Result<A, AlmideString>>) -> Result<A, AlmideString> {
    let __almide_r: Result<A, String> = (|| {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| io_err("fs.fold_lines", &fs_q(path), e))?);
        let mut acc = init;
        let mut buf = String::new();
        loop {
            buf.clear();
            if reader.read_line(&mut buf).map_err(|e| io_err("fs.fold_lines", &fs_q(path), e))? == 0 {
                return Ok(acc);
            }
            if buf.ends_with('\n') {
                buf.pop();
                if buf.ends_with('\r') {
                    buf.pop();
                }
            }
            // MEASURED (#1232, 2026-08-14): `clone` here beats `std::mem::take`, which
            // the ledger proposed as a free memcpy saving. `take` steals the String's
            // CAPACITY, so `read_line` regrows the buffer from zero every line (several
            // doublings for an ~80-byte line) where `clone` allocates one right-sized
            // copy and leaves `buf`'s capacity intact to reuse. 2M lines / 162 MB,
            // best-of-15, interleaved: 137 ms clone vs 143-146 ms take — the "saving"
            // is a 4-6% REGRESSION. Do not re-apply without re-measuring.
            acc = f(acc, AlmideString::from(buf.as_str()))?;
        }
    })();
    __almide_r.map_err(Into::into)
}
pub fn almide_rt_fs_for_each_line_effect(path: &str, f: std::rc::Rc<dyn Fn(AlmideString) -> Result<(), AlmideString>>) -> Result<(), AlmideString> {
    let __almide_r: Result<(), String> = (|| {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| io_err("fs.for_each_line", &fs_q(path), e))?);
        let mut buf = String::new();
        loop {
            buf.clear();
            if reader.read_line(&mut buf).map_err(|e| io_err("fs.for_each_line", &fs_q(path), e))? == 0 {
                return Ok(());
            }
            if buf.ends_with('\n') {
                buf.pop();
                if buf.ends_with('\r') {
                    buf.pop();
                }
            }
            // MEASURED (#1232, 2026-08-14): `clone` here beats `std::mem::take`, which
            // the ledger proposed as a free memcpy saving. `take` steals the String's
            // CAPACITY, so `read_line` regrows the buffer from zero every line (several
            // doublings for an ~80-byte line) where `clone` allocates one right-sized
            // copy and leaves `buf`'s capacity intact to reuse. 2M lines / 162 MB,
            // best-of-15, interleaved: 137 ms clone vs 143-146 ms take — the "saving"
            // is a 4-6% REGRESSION. Do not re-apply without re-measuring.
            f(AlmideString::from(buf.as_str()))?;
        }
    })();
    __almide_r.map_err(Into::into)
}

// Absence-as-Option content readers (#1106 / ADR-0004 D4): `Ok(None)` ⇔ the
// path (or a parent) does not exist; every other failure (permission, a
// directory at the path, IO) keeps the err path with the same message the
// plain reader produces. Only the runtime can classify — the String error
// has already erased ErrorKind, and a `fs.exists` pre-check races (TOCTOU).
pub fn almide_rt_fs_read_text_if_exists(path: &str) -> Result<Option<AlmideString>, AlmideString> {
    let __almide_v: Result<Option<String>, AlmideString> = {
        let __almide_r: Result<Option<String>, String> = (|| {
            match std::fs::read_to_string(path) {
                Ok(s) => Ok(Some(s)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(io_err("fs.read_text_if_exists", &fs_q(path), e)),
            }
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}
pub fn almide_rt_fs_read_bytes_if_exists(path: &str) -> Result<Option<Vec<i64>>, AlmideString> {
    let __almide_r: Result<Option<Vec<i64>>, String> = (|| {
        match std::fs::read(path) {
            Ok(b) => Ok(Some(b.into_iter().map(|x| x as i64).collect())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err("fs.read_bytes_if_exists", &fs_q(path), e)),
        }
    })();
    __almide_r.map_err(Into::into)
}
pub fn almide_rt_fs_read_lines_if_exists(path: &str) -> Result<Option<Vec<AlmideString>>, AlmideString> {
    let __almide_v: Result<Option<Vec<String>>, AlmideString> = {
        let __almide_r: Result<Option<Vec<String>>, String> = (|| {
            match std::fs::read_to_string(path) {
                Ok(s) => Ok(Some(s.lines().map(|l| l.to_string()).collect())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(io_err("fs.read_lines_if_exists", &fs_q(path), e)),
            }
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}
pub fn almide_rt_fs_read_bytes_raw_if_exists(path: &str) -> Result<Option<Vec<u8>>, AlmideString> {
    let __almide_r: Result<Option<Vec<u8>>, String> = (|| {
        match std::fs::read(path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err("fs.read_bytes_raw_if_exists", &fs_q(path), e)),
        }
    })();
    __almide_r.map_err(Into::into)
}

// Write
pub fn almide_rt_fs_write(path: &str, content: &str) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { std::fs::write(path, content).map_err(|e| io_err("fs.write", &fs_q(path), e)) })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_fs_write_bytes(path: &str, bytes: &[i64]) -> Result<(), AlmideString> {
    let __almide_r: Result<(), String> = (|| {
        let data: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
        std::fs::write(path, &data).map_err(|e| io_err("fs.write_bytes", &fs_q(path), e))
    })();
    __almide_r.map_err(Into::into)
}
pub fn almide_rt_fs_append(path: &str, content: &str) -> Result<(), AlmideString> {
    let __almide_r: Result<(), String> = (|| {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).create(true).open(path).map_err(|e| io_err("fs.append", &fs_q(path), e))?;
        f.write_all(content.as_bytes()).map_err(|e| io_err("fs.append", &fs_q(path), e))
    })();
    __almide_r.map_err(Into::into)
}

// Directory
pub fn almide_rt_fs_mkdir_p(path: &str) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { std::fs::create_dir_all(path).map_err(|e| io_err("fs.mkdir_p", &fs_q(path), e)) })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_fs_list_dir(path: &str) -> Result<Vec<AlmideString>, AlmideString> {
    let __almide_v: Result<Vec<String>, AlmideString> = {
        let __almide_r: Result<Vec<String>, String> = (|| {
            let entries = std::fs::read_dir(path).map_err(|e| io_err("fs.list_dir", &fs_q(path), e))?;
            let mut names = Vec::new();
            for entry in entries {
                let e = entry.map_err(|e| io_err("fs.list_dir", &fs_q(path), e))?;
                names.push(e.file_name().to_string_lossy().to_string());
            }
            // One directory's names are unique, so the unstable sort gives
            // the same byte order; it swaps in place, where the stable
            // sort's scratch merges moved bytes faster than n log n
            // (x2.2 per doubling of the entry count, GCE 2026-10-18).
            names.sort_unstable();
            Ok(names)
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

// Delete
pub fn almide_rt_fs_remove(path: &str) -> Result<(), AlmideString> {
    let __almide_r: Result<(), String> = (|| {
        let p = Path::new(path);
        if p.is_dir() { std::fs::remove_dir(path).map_err(|e| io_err("fs.remove", &fs_q(path), e)) }
        else { std::fs::remove_file(path).map_err(|e| io_err("fs.remove", &fs_q(path), e)) }
    })();
    __almide_r.map_err(Into::into)
}
pub fn almide_rt_fs_remove_all(path: &str) -> Result<(), AlmideString> {
    let __almide_r: Result<(), String> = (|| {
        let p = Path::new(path);
        if p.is_dir() { std::fs::remove_dir_all(path).map_err(|e| io_err("fs.remove_all", &fs_q(path), e)) }
        else { std::fs::remove_file(path).map_err(|e| io_err("fs.remove_all", &fs_q(path), e)) }
    })();
    __almide_r.map_err(Into::into)
}

// Copy / Rename
fn fs_same_file(a: &str, b: &str) -> bool {
    matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

pub fn almide_rt_fs_copy(src: &str, dst: &str) -> Result<(), AlmideString> {
    let __almide_r: Result<(), String> = (|| {
        let args = format!("{}, {}", fs_q(src), fs_q(dst));
        // A directory source fails as READING it does (EISDIR, C-215): the stock
        // wasm leg's self-hosted copy reads its source first, and std::fs::copy's
        // own InvalidInput text carries no errno. The embedded host's twin is
        // host_fs.rs `fs_copy_or_rename`.
        if Path::new(src).is_dir() {
            if let Err(e) = std::fs::read(src) {
                return Err(io_err("fs.copy", &args, e));
            }
        }
        // Onto the same file (under any spelling) the copy is already done:
        // std::fs::copy truncates its destination before reading the source,
        // which would empty the file. The host twin is `fs_copy_or_rename`.
        if fs_same_file(src, dst) {
            return Ok(());
        }
        std::fs::copy(src, dst).map(|_| ()).map_err(|e| io_err("fs.copy", &args, e))
    })();
    __almide_r.map_err(Into::into)
}
pub fn almide_rt_fs_rename(src: &str, dst: &str) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { std::fs::rename(src, dst).map_err(|e| io_err("fs.rename", &format!("{}, {}", fs_q(src), fs_q(dst)), e)) })(); __almide_r.map_err(Into::into) }

// Predicates
pub fn almide_rt_fs_exists(path: &str) -> bool { Path::new(path).exists() }
pub fn almide_rt_fs_is_dir(path: &str) -> bool { Path::new(path).is_dir() }
pub fn almide_rt_fs_is_file(path: &str) -> bool { Path::new(path).is_file() }
pub fn almide_rt_fs_is_symlink(path: &str) -> bool { Path::new(path).is_symlink() }

// Metadata
pub fn almide_rt_fs_file_size(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { std::fs::metadata(path).map(|m| m.len() as i64).map_err(|e| io_err("fs.file_size", &fs_q(path), e)) })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_fs_modified_at(path: &str) -> Result<i64, AlmideString> {
    let __almide_r: Result<i64, String> = (|| {
        let meta = std::fs::metadata(path).map_err(|e| io_err("fs.modified_at", &fs_q(path), e))?;
        let modified = meta.modified().map_err(|e| io_err("fs.modified_at", &fs_q(path), e))?;
        Ok(modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
    })();
    __almide_r.map_err(Into::into)
}
pub fn almide_rt_fs_stat(path: &str) -> Result<AlmideFileStat, AlmideString> {
    let __almide_r: Result<AlmideFileStat, String> = (|| {
        let meta = std::fs::metadata(path).map_err(|e| io_err("fs.stat", &fs_q(path), e))?;
        let size = meta.len() as i64;
        let is_dir = meta.is_dir();
        let is_file = meta.is_file();
        let modified = meta.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64).unwrap_or(0);
        Ok(AlmideFileStat { size, is_dir, is_file, modified })
    })();
    __almide_r.map_err(Into::into)
}

// Temp
pub fn almide_rt_fs_temp_dir() -> AlmideString { let __almide_out: String = std::env::temp_dir().to_string_lossy().replace('\\', "/"); __almide_out.into() }
pub fn almide_rt_fs_create_temp_file(prefix: &str) -> Result<AlmideString, AlmideString> {
    let __almide_v: Result<String, AlmideString> = {
        let __almide_r: Result<String, String> = (|| {
            let dir = std::env::temp_dir();
            let name = format!("{}{}", prefix, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
            let path = dir.join(&name);
            std::fs::write(&path, "").map_err(|e| io_err("fs.create_temp_file", &fs_q(&path.to_string_lossy()), e))?;
            Ok(path.to_string_lossy().replace('\\', "/"))
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}
pub fn almide_rt_fs_create_temp_dir(prefix: &str) -> Result<AlmideString, AlmideString> {
    let __almide_v: Result<String, AlmideString> = {
        let __almide_r: Result<String, String> = (|| {
            let dir = std::env::temp_dir();
            let name = format!("{}{}", prefix, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
            let path = dir.join(&name);
            std::fs::create_dir_all(&path).map_err(|e| io_err("fs.create_temp_dir", &fs_q(&path.to_string_lossy()), e))?;
            Ok(path.to_string_lossy().replace('\\', "/"))
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

// Walk (recursive)
pub fn almide_rt_fs_walk(dir: &str) -> Result<Vec<AlmideString>, AlmideString> {
    let __almide_v: Result<Vec<String>, AlmideString> = {
        let __almide_r: Result<Vec<String>, String> = (|| {
            let mut results = Vec::new();
            walk_recursive(Path::new(dir), &mut results)?;
            results.sort();
            Ok(results)
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

fn walk_recursive(dir: &Path, results: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| io_err("fs.walk", &fs_q(&dir.to_string_lossy()), e))? {
        let entry = entry.map_err(|e| io_err("fs.walk", &fs_q(&dir.to_string_lossy()), e))?;
        let path = entry.path();
        results.push(path.to_string_lossy().replace('\\', "/"));
        if path.is_dir() { walk_recursive(&path, results)?; }
    }
    Ok(())
}

// Glob (segment-wise, #1805). The pattern is split on '/' — empty segments (a
// trailing or doubled slash) are dropped, a leading '/' makes it absolute — and
// the leading run of wildcard-free segments is the BASE: the directory the walk
// starts from, re-spelled from its segments in front of every result (empty =
// the cwd, walked as "."). The remaining segments match one-to-one against the
// base-relative path of every entry the recursive walk visits (files AND
// directories, fs.walk's visit set): `*` matches any run of characters within
// ONE segment (empty included, a leading dot included), a segment that is
// exactly `**` matches zero or more whole segments, everything else is literal.
// A base that is not a directory answers [] (never an error); a pattern without
// wildcards is a literal path and answers [path] iff it exists; a `**`-only
// remainder also admits the base itself. Results are cwd-relative (no `./`)
// when the base is empty, and byte-order sorted. The walk is depth-bounded to
// the pattern's segment count when it holds no `**`. The wasm twin
// (stdlib/fs_walk.almd `fs_glob`) transcribes this algorithm step for step.
pub fn almide_rt_fs_glob(pattern: &str) -> Result<Vec<AlmideString>, AlmideString> {
    let __almide_v: Result<Vec<String>, AlmideString> = {
        let __almide_r: Result<Vec<String>, String> = (|| {
            let absolute = pattern.starts_with('/');
            let segs: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
            let k = segs.iter().take_while(|s| !s.contains('*')).count();
            let pats = &segs[k..];
            let base = format!("{}{}", if absolute { "/" } else { "" }, segs[..k].join("/"));
            if pats.is_empty() {
                let hit = !base.is_empty() && Path::new(&base).exists();
                return Ok(if hit { vec![base] } else { Vec::new() });
            }
            if !base.is_empty() && !Path::new(&base).is_dir() { return Ok(Vec::new()); }
            let root = if base.is_empty() { "." } else { base.as_str() };
            let prefix = if base.is_empty() || base == "/" { base.clone() } else { format!("{}/", base) };
            let depth = if pats.iter().any(|p| *p == "**") { None } else { Some(pats.len()) };
            let mut results = Vec::new();
            if !base.is_empty() && glob_segs_match(pats, &[]) { results.push(base.clone()); }
            let mut rels = Vec::new();
            glob_walk(Path::new(root), "", depth, &mut rels)?;
            for rel in rels {
                let rsegs: Vec<&str> = rel.split('/').collect();
                if glob_segs_match(pats, &rsegs) { results.push(format!("{}{}", prefix, rel)); }
            }
            results.sort();
            Ok(results)
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

// Collects every entry below `dir` as a `/`-joined path relative to the walk
// root; `depth` is the count of levels still to visit (`Some(1)` = list `dir`
// only), `None` unbounded.
fn glob_walk(dir: &Path, rel_prefix: &str, depth: Option<usize>, out: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| io_err("fs.glob", &fs_q(&dir.to_string_lossy()), e))? {
        let entry = entry.map_err(|e| io_err("fs.glob", &fs_q(&dir.to_string_lossy()), e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if rel_prefix.is_empty() { name } else { format!("{}/{}", rel_prefix, name) };
        let path = entry.path();
        let descend = depth != Some(1) && path.is_dir();
        out.push(rel.clone());
        if descend { glob_walk(&path, &rel, depth.map(|d| d - 1), out)?; }
    }
    Ok(())
}

// `pats` against `segs`, segment by segment; `**` spans zero or more whole segments.
fn glob_segs_match(pats: &[&str], segs: &[&str]) -> bool {
    match pats.first() {
        None => segs.is_empty(),
        Some(&"**") => (0..=segs.len()).any(|i| glob_segs_match(&pats[1..], &segs[i..])),
        Some(pat) => !segs.is_empty() && glob_star_match(pat, segs[0]) && glob_segs_match(&pats[1..], &segs[1..]),
    }
}

// One segment: `*` matches any run of characters (empty included); the rest is literal.
fn glob_star_match(pat: &str, seg: &str) -> bool {
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 { return pat == seg; }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if seg.len() < first.len() + last.len() || !seg.starts_with(first) || !seg.ends_with(last) { return false; }
    let region = &seg[first.len()..seg.len() - last.len()];
    let mut pos = 0;
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() { continue; }
        match region[pos..].find(part) {
            Some(i) => pos += i + part.len(),
            None => return false,
        }
    }
    true
}

pub fn almide_rt_fs_read_bytes_raw(path: &str) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { std::fs::read(path).map_err(|e| io_err("fs.read_bytes_raw", &fs_q(path), e)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_fs_write_bytes_raw(path: &str, data: &Vec<u8>) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { std::fs::write(path, data).map_err(|e| io_err("fs.write_bytes_raw", &fs_q(path), e)) })(); __almide_r.map_err(Into::into) }
