// fs extern — Rust native implementations (platform layer: native only, no WASM)

use std::path::Path;

fn io_err(e: impl std::fmt::Display) -> String { format!("{}", e) }

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
pub fn almide_rt_fs_read_text(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(io_err)
}
pub fn almide_rt_fs_read_bytes(path: &str) -> Result<Vec<i64>, String> {
    std::fs::read(path).map(|b| b.into_iter().map(|x| x as i64).collect()).map_err(io_err)
}
pub fn almide_rt_fs_read_lines(path: &str) -> Result<Vec<String>, String> {
    std::fs::read_to_string(path).map(|s| s.lines().map(|l| l.to_string()).collect()).map_err(io_err)
}

// Streaming line readers: fold/each over a BufReader, never materializing the
// file — peak memory is O(longest line), not O(file). Line semantics MUST
// byte-match read_lines' `.lines()`: split on \n, strip one trailing \r, no
// phantom empty line after a trailing newline, a final \n-less line still
// yielded. One deliberate divergence from read_lines: a mid-file read error
// (e.g. invalid UTF-8) surfaces AFTER the callbacks for earlier lines have
// already run — inherent to streaming.
pub fn almide_rt_fs_fold_lines<A>(path: &str, init: A, f: std::rc::Rc<dyn Fn(A, String) -> A>) -> Result<A, String> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(std::fs::File::open(path).map_err(io_err)?);
    let mut acc = init;
    let mut buf = String::new();
    loop {
        buf.clear();
        if reader.read_line(&mut buf).map_err(io_err)? == 0 {
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
        acc = f(acc, buf.clone());
    }
}
// Range-scoped fold: the chunk-parallel worker's walker. A line is owned by
// the chunk containing the byte BEFORE its first byte (line 0 by the chunk
// containing byte 0), so a partition of [0, filesize) processes every line
// exactly once: start>0 seeks to start-1 and discards through the first \n
// (if byte start-1 IS \n the discard consumes just it), then lines fold while
// their start offset is < end — the final line may read past end to finish.
fn fold_lines_range_impl<A, F: Fn(A, String) -> A>(path: &str, start: i64, end: i64, init: A, f: &F) -> Result<A, String> {
    use std::io::{BufRead, Seek};
    let mut file = std::fs::File::open(path).map_err(io_err)?;
    let mut pos: u64 = if start > 0 { (start - 1) as u64 } else { 0 };
    file.seek(std::io::SeekFrom::Start(pos)).map_err(io_err)?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = String::new();
    if start > 0 {
        let n = reader.read_line(&mut buf).map_err(io_err)?;
        if n == 0 {
            return Ok(init);
        }
        pos += n as u64;
    }
    let mut acc = init;
    while (pos as i64) < end {
        buf.clear();
        let n = reader.read_line(&mut buf).map_err(io_err)?;
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
        acc = f(acc, buf.clone());
    }
    Ok(acc)
}
// Raw `F: Fn` last arg (NOT `Rc<dyn Fn>`): build.rs derives the
// takes_raw_fn_last_arg set from this signature, so callbacks render unboxed.
pub fn almide_rt_fs_fold_lines_range<A, F: Fn(A, String) -> A>(path: &str, start: i64, end: i64, init: A, f: F) -> Result<A, String> {
    fold_lines_range_impl(path, start, end, init, &f)
}
// Chunk-parallel fold: the threads live HERE, inside the runtime — the fan
// machinery's purity gate keeps effectful bodies sequential, so file
// parallelism is the fs module's own intrinsic instead. Workers each fold
// their byte-range partition (fold_lines_range_impl's ownership rule makes
// the partition exact) on a scoped thread; the per-chunk partials return in
// CHUNK ORDER and merging stays with the caller, so results are
// deterministic whatever the thread schedule. The Send + Sync bounds are
// enforced by rustc on the emitted program: a callback capturing non-Send
// state is a loud compile error, never a data race.
pub fn almide_rt_fs_fold_lines_chunked<A: Clone + Send, F: Fn(A, String) -> A + Send + Sync>(path: &str, workers: i64, init: A, f: F) -> Result<Vec<A>, String> {
    let size = std::fs::metadata(path).map_err(io_err)?.len() as i64;
    let w = workers.max(1);
    let chunk = size / w + 1;
    let mut ranges: Vec<(i64, i64)> = Vec::new();
    for i in 0..w {
        let s = i * chunk;
        if s < size {
            ranges.push((s, (s + chunk).min(size)));
        }
    }
    std::thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = ranges
            .iter()
            .map(|&(s, e)| {
                let init = init.clone();
                scope.spawn(move || fold_lines_range_impl(path, s, e, init, f))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().map_err(|_| "fold_lines_chunked: worker panicked".to_string())?)
            .collect()
    })
}
pub fn almide_rt_fs_for_each_line(path: &str, f: std::rc::Rc<dyn Fn(String)>) -> Result<(), String> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(std::fs::File::open(path).map_err(io_err)?);
    let mut buf = String::new();
    loop {
        buf.clear();
        if reader.read_line(&mut buf).map_err(io_err)? == 0 {
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
        f(buf.clone());
    }
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
pub fn almide_rt_fs_fold_lines_effect<A>(path: &str, init: A, f: std::rc::Rc<dyn Fn(A, String) -> Result<A, String>>) -> Result<A, String> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(std::fs::File::open(path).map_err(io_err)?);
    let mut acc = init;
    let mut buf = String::new();
    loop {
        buf.clear();
        if reader.read_line(&mut buf).map_err(io_err)? == 0 {
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
        acc = f(acc, buf.clone())?;
    }
}
pub fn almide_rt_fs_for_each_line_effect(path: &str, f: std::rc::Rc<dyn Fn(String) -> Result<(), String>>) -> Result<(), String> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(std::fs::File::open(path).map_err(io_err)?);
    let mut buf = String::new();
    loop {
        buf.clear();
        if reader.read_line(&mut buf).map_err(io_err)? == 0 {
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
        f(buf.clone())?;
    }
}

// Absence-as-Option content readers (#1106 / ADR-0004 D4): `Ok(None)` ⇔ the
// path (or a parent) does not exist; every other failure (permission, a
// directory at the path, IO) keeps the err path with the same message the
// plain reader produces. Only the runtime can classify — the String error
// has already erased ErrorKind, and a `fs.exists` pre-check races (TOCTOU).
pub fn almide_rt_fs_read_text_if_exists(path: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(e)),
    }
}
pub fn almide_rt_fs_read_bytes_if_exists(path: &str) -> Result<Option<Vec<i64>>, String> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b.into_iter().map(|x| x as i64).collect())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(e)),
    }
}
pub fn almide_rt_fs_read_lines_if_exists(path: &str) -> Result<Option<Vec<String>>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s.lines().map(|l| l.to_string()).collect())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(e)),
    }
}
pub fn almide_rt_fs_read_bytes_raw_if_exists(path: &str) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(e)),
    }
}

// Write
pub fn almide_rt_fs_write(path: &str, content: &str) -> Result<(), String> {
    std::fs::write(path, content).map_err(io_err)
}
pub fn almide_rt_fs_write_bytes(path: &str, bytes: &[i64]) -> Result<(), String> {
    let data: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
    std::fs::write(path, &data).map_err(io_err)
}
pub fn almide_rt_fs_append(path: &str, content: &str) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).create(true).open(path).map_err(io_err)?;
    f.write_all(content.as_bytes()).map_err(io_err)
}

// Directory
pub fn almide_rt_fs_mkdir_p(path: &str) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(io_err)
}
pub fn almide_rt_fs_list_dir(path: &str) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(path).map_err(io_err)?;
    let mut names = Vec::new();
    for entry in entries {
        let e = entry.map_err(io_err)?;
        names.push(e.file_name().to_string_lossy().to_string());
    }
    names.sort();
    Ok(names)
}

// Delete
pub fn almide_rt_fs_remove(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if p.is_dir() { std::fs::remove_dir(path).map_err(io_err) }
    else { std::fs::remove_file(path).map_err(io_err) }
}
pub fn almide_rt_fs_remove_all(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if p.is_dir() { std::fs::remove_dir_all(path).map_err(io_err) }
    else { std::fs::remove_file(path).map_err(io_err) }
}

// Copy / Rename
pub fn almide_rt_fs_copy(src: &str, dst: &str) -> Result<(), String> {
    std::fs::copy(src, dst).map(|_| ()).map_err(io_err)
}
pub fn almide_rt_fs_rename(src: &str, dst: &str) -> Result<(), String> {
    std::fs::rename(src, dst).map_err(io_err)
}

// Predicates
pub fn almide_rt_fs_exists(path: &str) -> bool { Path::new(path).exists() }
pub fn almide_rt_fs_is_dir(path: &str) -> bool { Path::new(path).is_dir() }
pub fn almide_rt_fs_is_file(path: &str) -> bool { Path::new(path).is_file() }
pub fn almide_rt_fs_is_symlink(path: &str) -> bool { Path::new(path).is_symlink() }

// Metadata
pub fn almide_rt_fs_file_size(path: &str) -> Result<i64, String> {
    std::fs::metadata(path).map(|m| m.len() as i64).map_err(io_err)
}
pub fn almide_rt_fs_modified_at(path: &str) -> Result<i64, String> {
    let meta = std::fs::metadata(path).map_err(io_err)?;
    let modified = meta.modified().map_err(io_err)?;
    Ok(modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
}
pub fn almide_rt_fs_stat(path: &str) -> Result<AlmideFileStat, String> {
    let meta = std::fs::metadata(path).map_err(io_err)?;
    let size = meta.len() as i64;
    let is_dir = meta.is_dir();
    let is_file = meta.is_file();
    let modified = meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64).unwrap_or(0);
    Ok(AlmideFileStat { size, is_dir, is_file, modified })
}

// Temp
pub fn almide_rt_fs_temp_dir() -> String {
    std::env::temp_dir().to_string_lossy().replace('\\', "/")
}
pub fn almide_rt_fs_create_temp_file(prefix: &str) -> Result<String, String> {
    let dir = std::env::temp_dir();
    let name = format!("{}{}", prefix, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let path = dir.join(&name);
    std::fs::write(&path, "").map_err(io_err)?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}
pub fn almide_rt_fs_create_temp_dir(prefix: &str) -> Result<String, String> {
    let dir = std::env::temp_dir();
    let name = format!("{}{}", prefix, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let path = dir.join(&name);
    std::fs::create_dir_all(&path).map_err(io_err)?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}

// Walk (recursive)
pub fn almide_rt_fs_walk(dir: &str) -> Result<Vec<String>, String> {
    let mut results = Vec::new();
    walk_recursive(Path::new(dir), &mut results)?;
    results.sort();
    Ok(results)
}

fn walk_recursive(dir: &Path, results: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
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
pub fn almide_rt_fs_glob(pattern: &str) -> Result<Vec<String>, String> {
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
}

// Collects every entry below `dir` as a `/`-joined path relative to the walk
// root; `depth` is the count of levels still to visit (`Some(1)` = list `dir`
// only), `None` unbounded.
fn glob_walk(dir: &Path, rel_prefix: &str, depth: Option<usize>, out: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
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

pub fn almide_rt_fs_read_bytes_raw(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(io_err)
}

pub fn almide_rt_fs_write_bytes_raw(path: &str, data: &Vec<u8>) -> Result<(), String> {
    std::fs::write(path, data).map_err(io_err)
}
