//! ADR-0033 D1 risk: the rc-word READER INVENTORY. An interior block's rc
//! word is `0x8000_0000 | distance-to-root`, so every place that loads or
//! stores an rc word must be one of:
//!
//! - `redirect`  — increments / decrements: credits the root instead
//!   (crates/almide-wasm family.rs), emitted in modules that link a
//!   `@family_producer`;
//! - `shared`    — a uniqueness judge comparing the word UNSIGNED against 1
//!   (`== 1`, `!= 1`, `<= 1` unsigned, `> 1` unsigned): an interior word
//!   reads as shared, so the block is copied, never written in place;
//! - `unseen`    — the site can never meet an interior block (the reason
//!   says why: a birth store on a fresh block, a free-list word, a payload
//!   slot that is not a header, a static);
//! - `copyout`   — a host or export boundary that copies the bytes out and
//!   releases through the redirecting `$dec_flat`.
//!
//! The sites are found by spelling (`RC.offset`, a raw offset-0 word access,
//! a stdlib `prim.load32(h)` / `prim.store32(h, …)` with no offset, the JS
//! host's `getUint32(h, true)`), counted per file, and every count is
//! classified below. A new site — or a moved count — fails this test until
//! it is classified here. Spelling is the limit of the method: a header
//! access spelled some other way is not seen, which is why the redirect
//! itself is also tested by running it (family_tests.rs).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Kind {
    /// `RC.offset`
    Rc,
    /// `word(0)` — a raw offset-0 word access
    Word0,
    /// `offset: 0, align: 2` — a raw offset-0 MemArg
    Off0,
    /// stdlib `prim.load32(x)` / `prim.store32(x, …)` with a bare identifier
    Header,
    /// JS host `getUint32(h, true)`
    Js,
}

/// (path, kind, lines, class, reason)
const INVENTORY: &[(&str, Kind, usize, &str, &str)] = &[
    // ── the emitter (crates/almide-wasm) ──
    ("crates/almide-wasm/src/runtime_alloc.rs", Kind::Rc, 17, "redirect+unseen",
     "$alloc's two birth stores (rc = 1 on a fresh block: unseen); $inc load+store and $dec_flat load+store \
      (retarget first: redirect); $drop_list, $drop_<shape>, $drop_http_call, $drop_map, $drop_entries load+store \
      each (forward first: redirect); $cow_elems' interior test (family modules: copy, take credits, then release)"),
    ("crates/almide-wasm/src/runtime_alloc.rs", Kind::Word0, 5, "unseen",
     "$alloc / $free free-list head words (FREELIST_BASE slots, not headers) and the element slots of \
      $drop_list / $inc_elems (payload handles, released through the redirecting decs)"),
    ("crates/almide-wasm/src/runtime_alloc_cow.rs", Kind::Rc, 2, "shared",
     "$map_reserve's `rc > 1` unsigned precondition (an interior word traps: defined abort, never a write) and \
      $cow's `rc <= 1` unsigned (interior reads as shared: copied, then released through the redirect)"),
    ("crates/almide-wasm/src/deep_walk.rs", Kind::Rc, 2, "redirect",
     "$drop_<shape> of a recursive type: load+store after the forward"),
    ("crates/almide-wasm/src/dying_move.rs", Kind::Rc, 1, "shared",
     "@reuse / dying spread judge `rc == 1`: an interior source is copied"),
    ("crates/almide-wasm/src/writeback_move.rs", Kind::Rc, 1, "shared",
     "release_written_slots `rc == 1`: an interior tuple keeps its slots"),
    ("crates/almide-wasm/src/map_inplace.rs", Kind::Rc, 1, "shared", "map.set in-place judge `rc == 1`"),
    ("crates/almide-wasm/src/map_inplace.rs", Kind::Off0, 1, "unseen", "loads an entry slot handle to release it (payload, not a header)"),
    ("crates/almide-wasm/src/map_remove.rs", Kind::Rc, 2, "shared", "map.remove / set.remove in-place judges `rc == 1`"),
    ("crates/almide-wasm/src/set_insert.rs", Kind::Rc, 1, "shared", "set.insert in-place judge `rc == 1`"),
    ("crates/almide-wasm/src/value_set_path.rs", Kind::Rc, 1, "shared",
     "value_set_path's `unique` (`== 1` / `!= 1`): an interior Value / object list is rebuilt, never written"),
    ("crates/almide-wasm/src/runtime_str.rs", Kind::Rc, 2, "shared",
     "$str_append in-place windows `rc == 1` (with the static floor test): an interior String is copied"),
    ("crates/almide-wasm/src/runtime_push.rs", Kind::Rc, 1, "shared",
     "$list_push `rc != 1` takes the copying path for an interior list"),
    ("crates/almide-wasm/src/runtime_large.rs", Kind::Rc, 1, "unseen", "$ltake's birth store on a node it just took"),
    ("crates/almide-wasm/src/runtime_large.rs", Kind::Word0, 2, "unseen", "large free-list node words of free blocks"),
    ("crates/almide-wasm/src/alloc_inline.rs", Kind::Rc, 1, "unseen", "inlined $alloc's birth store"),
    ("crates/almide-wasm/src/carve.rs", Kind::Rc, 1, "unseen", "prim.carve's birth store: it WRITES the interior word of a block it just carved, before the family is published"),
    ("crates/almide-wasm/src/carve.rs", Kind::Word0, 1, "unseen", "the carve's countdown zero-fill of payload words, not a header"),
    ("crates/almide-wasm/src/alloc_inline.rs", Kind::Word0, 1, "unseen", "the inlined $alloc's free-list slot test"),
    ("crates/almide-wasm/src/func.rs", Kind::Rc, 2, "unseen",
     "pool statics' headers (rc = 1, below the line end: the floor guard skips them before any rc read)"),
    ("crates/almide-wasm/src/checked_alloc.rs", Kind::Word0, 1, "unseen",
     "the poison check over a free-list block's payload (a freed block is never interior: interior blocks are never freed)"),
    ("crates/almide-wasm/src/canonical_alloc.rs", Kind::Off0, 2, "unseen", "free-list slot words"),
    ("crates/almide-wasm/src/collections.rs", Kind::Off0, 3, "unseen", "typed slot load/store at an absolute slot address (payload)"),
    ("crates/almide-wasm/src/display_helpers.rs", Kind::Off0, 1, "unseen", "a payload slot of the displayed block"),
    ("crates/almide-wasm/src/matrix.rs", Kind::Off0, 2, "unseen", "f64 payload cells of a matrix"),
    ("crates/almide-wasm/src/runtime.rs", Kind::Off0, 4, "unseen", "$scan_* element slot cursors (payload)"),
    ("crates/almide-wasm/src/family.rs", Kind::Rc, 1, "redirect", "the redirect itself (retarget and forward)"),
    ("crates/almide-wasm/src/family_tests.rs", Kind::Rc, 1, "unseen", "test code"),
    ("crates/almide-wasm/src/runtime_alloc_cap_test.rs", Kind::Rc, 1, "unseen", "test code"),
    // ── the host (crates/almide-wasm-run) ──
    ("crates/almide-wasm-run/src/host_fan.rs", Kind::Rc, 1, "copyout",
     "fan placement writes PINNED_RC (2^30, positive) on a block it built in a worker from the parent's COPIED bytes; \
      the parent's blocks are only read (len + payload), never counted"),
    ("crates/almide-wasm-run/src/host.rs", Kind::Off0, 1, "unseen", "an out-of-bounds probe at address -1 (a test)"),
    // ── the JS host ──
    ("src/cli/js_host_values.js", Kind::Js, 1, "shared+copyout",
     "drop(): `getUint32(h) === 1` (unsigned) before walking children — an interior block's children are not walked; \
      the credit goes to __release = $dec_flat, which redirects"),
    // ── the self-host stdlib ──
    ("stdlib/list_concat.almd", Kind::Header, 2, "shared", "`prim.load32(ha) == 1` (load32 zero-extends): in-place append window"),
    ("stdlib/string_concat.almd", Kind::Header, 1, "shared", "`prim.load32(ha) == 1`: in-place append window"),
    ("stdlib/string_to_upper.almd", Kind::Header, 1, "shared", "`prim.load32(h) == 1`: in-place rewrite window"),
    ("stdlib/float_to_string.almd", Kind::Header, 12, "unseen", "bignum len words in a scratch block's payload (p = handle + 12 + …)"),
    ("stdlib/float_parse.almd", Kind::Header, 2, "unseen", "the decimal scratch's count word in its payload"),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            files(&p, ext, out);
        } else if p.extension().is_some_and(|e| e == ext) {
            out.push(p);
        }
    }
}

/// A stdlib `prim.load32(x)` / `prim.store32(x, …)` whose address is one
/// bare identifier — a header word, unless classified otherwise.
fn header_access(line: &str) -> bool {
    for op in ["prim.load32(", "prim.store32("] {
        let mut rest = line;
        while let Some(at) = rest.find(op) {
            let tail = &rest[at + op.len()..];
            let ident: String = tail.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let next = tail[ident.len()..].chars().next();
            if ident.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c == '_') && matches!(next, Some(',' | ')')) {
                return true;
            }
            rest = &rest[at + op.len()..];
        }
    }
    false
}

fn count(path: &Path, kind: Kind) -> usize {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .filter(|l| match kind {
            Kind::Rc => l.contains("RC.offset"),
            Kind::Word0 => l.contains("word(0)"),
            Kind::Off0 => l.contains("offset: 0, align: 2"),
            Kind::Header => header_access(l),
            Kind::Js => l.contains("getUint32(h, true)"),
        })
        .count()
}

#[test]
fn every_rc_word_site_is_classified() {
    let root = root();
    let mut found: BTreeMap<(String, Kind), usize> = BTreeMap::new();
    let scan = |dir: &str, ext: &str, kinds: &[Kind], found: &mut BTreeMap<(String, Kind), usize>| {
        let mut fs = Vec::new();
        files(&root.join(dir), ext, &mut fs);
        assert!(!fs.is_empty(), "no .{ext} files under {dir}: the scan is broken, not clean");
        for f in fs {
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            for &k in kinds {
                let n = count(&f, k);
                if n > 0 {
                    found.insert((rel.clone(), k), n);
                }
            }
        }
    };
    scan("crates/almide-wasm/src", "rs", &[Kind::Rc, Kind::Word0, Kind::Off0], &mut found);
    scan("crates/almide-wasm-run/src", "rs", &[Kind::Rc, Kind::Word0, Kind::Off0], &mut found);
    scan("src/cli", "js", &[Kind::Js], &mut found);
    scan("stdlib", "almd", &[Kind::Header], &mut found);
    let classified: BTreeMap<(String, Kind), (usize, &str, &str)> =
        INVENTORY.iter().map(|&(p, k, n, c, r)| ((p.to_string(), k), (n, c, r))).collect();
    assert_eq!(classified.len(), INVENTORY.len(), "an inventory row is duplicated");
    let mut problems = Vec::new();
    for ((p, k), n) in &found {
        match classified.get(&(p.clone(), *k)) {
            None => problems.push(format!("UNCLASSIFIED: {p} has {n} {k:?} site(s) — classify each (redirect / shared / unseen / copyout)")),
            Some((m, _, _)) if m != n => problems.push(format!("MOVED: {p} {k:?}: {n} site(s), the inventory says {m} — re-classify")),
            _ => {}
        }
    }
    for ((p, k), (m, _, _)) in &classified {
        if !found.contains_key(&(p.clone(), *k)) {
            problems.push(format!("STALE: {p} {k:?} ({m}) is no longer found — drop the row"));
        }
    }
    for (_, _, _, c, r) in INVENTORY {
        assert!(!r.is_empty(), "every row names its reason");
        assert!(c.split('+').all(|c| matches!(c, "redirect" | "shared" | "unseen" | "copyout")), "unknown class {c}");
    }
    assert!(problems.is_empty(), "rc-word inventory (ADR-0033 D1):\n{}", problems.join("\n"));
}

#[test]
fn the_header_scan_sees_what_it_must() {
    assert!(header_access("  if prim.load32(ha) == 1 and"));
    assert!(header_access("prim.store32(h, 1)"));
    assert!(!header_access("prim.load32(ha + 8)"));
    assert!(!header_access("prim.store32(8, data)"));
}
