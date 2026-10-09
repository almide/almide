//! #2157: the keyed-aggregation shapes on the native leg — what the emitted
//! Rust looks like and how much it allocates.
//!
//! The word count `let w = vocab[i]; m[w] = map.get_or(m, w, 0) + 1` against
//! the hand-written Rust reference (`HashMap<String, i64>`, one owned key
//! clone and one `entry` per draw) cost three things the reference does not:
//!
//! - `vocab[i]` copied the String TWICE: the clone pass wraps an index read
//!   in `Clone` to take an owned value, and `almide_index!` already returns
//!   one, so the walker emitted `almide_index!(vocab, i).clone()` — one
//!   allocation per draw made and freed on the spot (`m[k]` likewise ended
//!   in `.cloned().clone()`).
//! - the update probed the map twice (`get_or`, then `insert` of the same
//!   key). It is now one `upsert_with` when the new value is plain
//!   arithmetic over the one `get_or` read.
//! - the idiomatic twin `list.group_by(ws, f) |> map.map((g) => list.len(g))`
//!   built every group — each element copied once more for its key, pushed,
//!   then copied AGAIN by `map.map` (which borrowed a map nothing else read)
//!   and freed. It is now one counting pass (`almide_rt_list_group_count`),
//!   and a whole-map read at its source's last use moves the map instead of
//!   copying it (`*_owned` twins, the #3398 mechanism).
//!
//! Each cell below fails on the compiler before the change.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

const WORDS: &str = r#"
fn word_of(i: Int) -> String = "w${(i * 7919 + 17) % 1000}"

fn draw(i: Int) -> Int = (i * 2654435761 + 1) % 500
"#;

fn imperative(n: usize) -> String {
    format!(
        "{WORDS}
effect fn main() -> Unit = {{
  let vocab = list.range(0, 500) |> list.map(word_of)
  var counts: Map[String, Int] = [:]
  for i in list.range(0, {n}) {{
    let w = vocab[draw(i)]
    counts[w] = map.get_or(counts, w, 0) + 1
  }}
  let top = map.entries(counts) |> list.sort_by(((w, c)) => (0 - c, w)) |> list.take(3)
  for (w, c) in top {{
    println(\"${{w}} ${{c}}\")
  }}
}}
"
    )
}

fn grouped(n: usize) -> String {
    format!(
        "{WORDS}
effect fn main() -> Unit = {{
  let vocab = list.range(0, 500) |> list.map(word_of)
  let counts = list.range(0, {n})
    |> list.map((i) => vocab[draw(i)])
    |> list.group_by((w) => w)
    |> map.map((ws) => list.len(ws))
  let top = map.entries(counts) |> list.sort_by(((w, c)) => (0 - c, w)) |> list.take(3)
  for (w, c) in top {{
    println(\"${{w}} ${{c}}\")
  }}
}}
"
    )
}

fn write(name: &str, src: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, src).unwrap();
    (dir, path)
}

fn emit(name: &str, src: &str) -> String {
    let (_dir, path) = write(name, src);
    let out = Command::new(almide()).arg(&path).args(["--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The emitted program body (`__almide_main`), without the runtime.
fn main_body(rust: &str) -> &str {
    let i = rust.find("pub fn __almide_main(").expect("no __almide_main");
    rust[i..].split("\n}\n").next().unwrap()
}

/// (stdout, allocations) of a native run under the counting allocator.
fn run_counted(name: &str, src: &str) -> (String, u64) {
    let (_dir, path) = write(name, src);
    let out = Command::new(almide()).arg("run").arg(&path).env("ALMIDE_ALLOC_COUNT", "1").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "native run failed:\n{stderr}");
    let line = stderr
        .lines()
        .find(|l| l.starts_with("__ALMD_ALLOC"))
        .unwrap_or_else(|| panic!("no `__ALMD_ALLOC` line — the allocation lane did not arm:\n{stderr}"));
    let allocs = line
        .split_whitespace()
        .find_map(|f| f.strip_prefix("allocs="))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("unparsable allocation report: {line}"));
    (String::from_utf8_lossy(&out.stdout).to_string(), allocs)
}

#[test]
fn an_index_read_is_copied_once() {
    let rust = emit("index_once.almd", &imperative(10));
    let body = main_body(&rust);
    assert!(body.contains("almide_index!(vocab, draw(i))"), "the index read is gone:\n{body}");
    assert!(!body.contains("almide_index!(vocab, draw(i)).clone()"), "`vocab[i]` is copied twice:\n{body}");
    let rust = emit(
        "map_read_once.almd",
        "effect fn main() -> Unit = {\n  let m = [\"a\": \"x\", \"b\": \"y\"]\n  let v = m[\"a\"]\n  println(v ?? \"-\")\n  println(int.to_string(map.len(m)))\n}\n",
    );
    let body = main_body(&rust);
    assert!(!body.contains(".cloned().clone()"), "`m[k]` is copied twice:\n{body}");
}

#[test]
fn a_counting_update_probes_the_map_once() {
    let rust = emit("update_once.almd", &imperative(10));
    let body = main_body(&rust);
    assert!(
        body.contains("counts.upsert_with(w, 0i64, |__almide_old|"),
        "`m[w] = map.get_or(m, w, 0) + 1` is not one probe:\n{body}"
    );
    assert!(!body.contains("almide_rt_map_get_or"), "the update still reads the map separately:\n{body}");
}

/// `list.range(a, b)` walked once — a `for` head, an owned chain source —
/// counts instead of building a `Vec<i64>` to iterate.
#[test]
fn a_range_walked_once_is_not_built() {
    let body_rust = emit("range_head.almd", &imperative(10));
    let body = main_body(&body_rust);
    assert!(body.contains("for i in 0i64..10i64 {"), "the `for` head builds its range:\n{body}");
    let body_rust = emit("range_chain.almd", &grouped(10));
    let body = main_body(&body_rust);
    assert!(body.contains("(0i64..10i64).map("), "the chain source builds its range:\n{body}");
    assert!(!body.contains("almide_rt_list_range("), "a range is still materialized:\n{body}");
}

/// Shapes that must keep the two-step form: the new value calls something,
/// reads the map again, or reads the key elsewhere.
#[test]
fn an_update_that_is_not_plain_arithmetic_stays_two_steps() {
    let src = "fn bump(x: Int) -> Int = x + 1\n\
        effect fn main() -> Unit = {\n\
        \x20 var m: Map[String, Int] = [:]\n\
        \x20 for w in [\"a\", \"b\", \"a\"] {\n\
        \x20   m[w] = bump(map.get_or(m, w, 0))\n\
        \x20   m[w] = map.get_or(m, w, 0) + map.get_or(m, \"a\", 0)\n\
        \x20   m[w] = map.get_or(m, w, 0) + string.len(w)\n\
        \x20 }\n\
        \x20 println(\"${map.get_or(m, \"a\", 0)} ${map.get_or(m, \"b\", 0)}\")\n\
        }\n";
    let body_rust = emit("two_step.almd", src);
    let body = main_body(&body_rust);
    assert!(!body.contains("upsert_with"), "a non-arithmetic update was fused:\n{body}");
    let (_dir, path) = write("two_step.almd", src);
    let out = Command::new(almide()).arg("run").arg(&path).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "9 5\n", "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn group_by_then_len_counts_without_building_the_groups() {
    let rust = emit("group_count.almd", &grouped(10));
    let body = main_body(&rust);
    assert!(body.contains("almide_rt_list_group_count("), "group-and-count is not fused:\n{body}");
    assert!(!body.contains("almide_rt_list_group_by("), "the groups are still built:\n{body}");
    // A group used for anything but its length keeps the groups.
    let rust = emit(
        "group_kept.almd",
        "effect fn main() -> Unit = {\n  let g = list.group_by([\"a\", \"bb\", \"a\"], (w) => string.len(w)) |> map.map((ws) => list.join(ws, \"\"))\n  println(map.get(g, 1) ?? \"-\")\n}\n",
    );
    assert!(main_body(&rust).contains("almide_rt_list_group_by("), "a non-counting map.map lost its groups");
}

/// `a` is read once (moved), `b` twice (borrowed, then moved), `c` twice.
const MAP_OWNED: &str = "effect fn main() -> Unit = {\n  let a = [\"x\": 1, \"y\": 2]\n  let b = map.map(a, (v) => v * 10)\n  let c = map.map(b, (v) => v + 1)\n  println(\"${map.values(b)} ${map.values(c)} ${map.keys(c)}\")\n}\n";

#[test]
fn a_whole_map_read_at_its_last_use_moves_the_map() {
    let rust = emit("entries_owned.almd", &imperative(10));
    let body = main_body(&rust);
    assert!(body.contains("almide_rt_map_entries_owned(counts)"), "`map.entries` at the map's last use copies it:\n{body}");
    let rust = emit("map_owned.almd", MAP_OWNED);
    let body = main_body(&rust);
    assert!(body.contains("almide_rt_map_map_values_owned(a,"), "`map.map` at its source's last use copies it:\n{body}");
    assert!(body.contains("almide_rt_map_map_values(&b,"), "a source read again must stay borrowed:\n{body}");
    assert!(body.contains("almide_rt_map_keys_owned(c)"), "the last read of `c` must move it:\n{body}");
    let (_dir, path) = write("map_owned_run.almd", MAP_OWNED);
    let out = Command::new(almide()).arg("run").arg(&path).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "[10, 20] [11, 21] [\"x\", \"y\"]\n");
}

#[test]
fn the_word_count_allocates_once_per_draw() {
    let n = 20_000;
    let (out, allocs) = run_counted("count_alloc.almd", &imperative(n));
    let (out_g, allocs_g) = run_counted("group_alloc.almd", &grouped(n));
    assert_eq!(out, out_g, "the two spellings disagree");
    assert_eq!(out.lines().count(), 3, "unexpected output: {out}");
    // One owned key per draw (the reference's `vocab[i].clone()`) plus the
    // vocabulary and the map. Before: 2 per draw imperative (~42k), ~4.5 per
    // draw grouped (~90k) plus ~20k reallocs.
    assert!(allocs < (n as u64) * 23 / 20, "imperative count made {allocs} allocations for {n} draws");
    assert!(allocs_g < (n as u64) * 23 / 20, "grouped count made {allocs_g} allocations for {n} draws");
}

/// The String-key probe's two per-lookup costs that were not hashing: the
/// 1..=7-byte tail of `almide_rt_map_hash_bytes` was copied into a zeroed
/// buffer with a variable length (a `memcpy` call per short key), and every
/// probed slot re-read `hashes[p]` from a second array before rejecting it.
/// The tail is now read with fixed-width loads, and each slot carries the
/// hash's upper half. Behaviour (every length, shared prefixes and tails) is
/// pinned by spec/stdlib/map_keyed_count_test.almd on both legs.
#[test]
fn the_string_key_probe_reads_no_variable_length_tail_and_no_side_array() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/rs/src/map.rs")).unwrap();
    let section = |head: &str| -> String {
        let i = src.find(head).unwrap_or_else(|| panic!("runtime/rs/src/map.rs lost `{head}`"));
        src[i..].split("\n}\n").next().unwrap().to_string()
    };
    let hash = section("pub fn almide_rt_map_hash_bytes(");
    assert!(!hash.contains("..rest.len()]"), "the hash tail is a variable-length copy again:\n{hash}");
    let find = section("    pub fn find(");
    let find = find.split("\n    }\n").next().unwrap();
    assert!(!find.contains("self.hashes["), "a probe reads the side hash array per slot again:\n{find}");
    assert!(find.contains("ALMIDE_MAP_SLOT_TAG"), "a probe no longer rejects a slot on its tag:\n{find}");
}
