//! #2157: the keyed-aggregation shapes on the native leg — what the emitted
//! Rust looks like and how much it allocates.
//!
//! The word count `let w = vocab[i]; m[w] = (map.get(m, w) ?? 0) + 1` against
//! the hand-written Rust reference (`HashMap<String, i64>`, one owned key
//! clone and one `entry` per draw) cost three things the reference does not:
//!
//! - `vocab[i]` copied the String TWICE: the clone pass wraps an index read
//!   in `Clone` to take an owned value, and `almide_index!` already returns
//!   one, so the walker emitted `almide_index!(vocab, i).clone()` — one
//!   allocation per draw made and freed on the spot (`m[k]` likewise ended
//!   in `.cloned().clone()`).
//! - the update probed the map twice (the read, then `insert` of the same
//!   key). It is now one `upsert_with` when the new value is plain
//!   arithmetic over the one defaulted read.
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
    counts[w] = (map.get(counts, w) ?? 0) + 1
  }}
  let top = map.to_list(counts) |> list.sort_by(((w, c)) => (0 - c, w)) |> list.take(3)
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
  let top = map.to_list(counts) |> list.sort_by(((w, c)) => (0 - c, w)) |> list.take(3)
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
        "`m[w] = (map.get(m, w) ?? 0) + 1` is not one probe:\n{body}"
    );
    assert!(!body.contains("almide_rt_map_get"), "the update still reads the map separately:\n{body}");
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
    // A collecting chain counts into one vector reserved for the span (#3604).
    assert!(
        body.contains("let __almide_re: i64 = 10i64;") && body.contains("(__almide_rs..__almide_re).map("),
        "the chain source builds its range:\n{body}"
    );
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
        \x20   m[w] = bump(map.get(m, w) ?? 0)\n\
        \x20   m[w] = (map.get(m, w) ?? 0) + (map.get(m, \"a\") ?? 0)\n\
        \x20   m[w] = (map.get(m, w) ?? 0) + string.len(w)\n\
        \x20 }\n\
        \x20 println(\"${map.get(m, \"a\") ?? 0} ${map.get(m, \"b\") ?? 0}\")\n\
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

/// The verified native render counts the same chain in one pass
/// (`native_group_count`). What the program prints is the two-step form's
/// answer: keys in first-seen order, a String and an Int key, an empty list,
/// a source list read again afterwards, and — refused by the rule — a
/// callback that is more than the group's length.
#[test]
fn a_counted_group_by_prints_what_the_two_steps_print() {
    let src = "fn show[K](m: Map[K, Int]) -> String =\n\
        \x20 map.to_list(m) |> list.map(((k, v)) => \"${k}=${v}\") |> list.join(\",\")\n\n\
        fn tally(xs: List[String]) -> Int = {\n\
        \x20 let c = list.group_by(xs, (w) => w) |> map.map((g) => list.len(g))\n\
        \x20 map.len(c) * 100 + list.len(xs)\n\
        }\n\n\
        effect fn main() -> Unit = {\n\
        \x20 let ws = [\"b\", \"a\", \"b\", \"c\", \"a\", \"b\"]\n\
        \x20 let nothing: List[String] = []\n\
        \x20 println(show(list.group_by(ws, (w) => w) |> map.map((g) => list.len(g))))\n\
        \x20 println(show(list.group_by([\"aa\", \"b\", \"cc\", \"d\", \"e\"], (w) => string.len(w)) |> map.map((g) => list.len(g))))\n\
        \x20 println(show(list.group_by(ws, (w) => w) |> map.map((g) => list.len(g) * 10)))\n\
        \x20 println(show(list.group_by(nothing, (w) => w) |> map.map((g) => list.len(g))))\n\
        \x20 println(int.to_string(tally(ws) + list.len(ws)))\n\
        }\n";
    let (_dir, path) = write("counted.almd", src);
    let out = Command::new(almide()).arg("run").arg(&path).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "b=3,a=2,c=1\n2=2,1=3\nb=30,a=20,c=10\n\n312\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `a` is read once (moved), `b` twice (borrowed, then moved), `c` twice.
const MAP_OWNED: &str = "effect fn main() -> Unit = {\n  let a = [\"x\": 1, \"y\": 2]\n  let b = map.map(a, (v) => v * 10)\n  let c = map.map(b, (v) => v + 1)\n  println(\"${map.values(b)} ${map.values(c)} ${map.keys(c)}\")\n}\n";

#[test]
fn a_whole_map_read_at_its_last_use_moves_the_map() {
    let rust = emit("entries_owned.almd", &imperative(10));
    let body = main_body(&rust);
    assert!(body.contains("almide_rt_map_entries_owned(counts)"), "`map.to_list` at the map's last use copies it:\n{body}");
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
    // The probe expression, not the bare constant name: a quoted `"ALMIDE_…"`
    // literal reads as an environment switch to scripts/check-env-switches.sh.
    assert!(find.contains("s & ALMIDE_MAP_SLOT_TAG == tag"), "a probe no longer rejects a slot on its tag:\n{find}");
}

/// The verified native render fuses the same update into one `map.upsert`
/// (`native_map_upsert`), whose `init` runs on every call. What the program
/// prints is the two-step form's answer for the fused shapes — wrapping Int
/// arithmetic, Float arithmetic, a present key kept in place — and for the
/// ones the rule refuses: an Int `/` whose default would divide by zero does
/// not abort while the key is present.
#[test]
fn a_fused_update_prints_what_the_two_steps_print() {
    let src = "fn show(m: Map[String, Int]) -> String =\n\
        \x20 map.to_list(m) |> list.map(((k, v)) => \"${k}=${v}\") |> list.join(\",\")\n\n\
        effect fn main() -> Unit = {\n\
        \x20 let step = 2\n\
        \x20 var a: Map[String, Int] = [:]\n\
        \x20 var b: Map[String, Int] = [:]\n\
        \x20 var c: Map[String, Int] = [:]\n\
        \x20 var h: Map[String, Int] = [\"b\": 9223372036854775807]\n\
        \x20 var f: Map[Int, Float] = [:]\n\
        \x20 for w in [\"b\", \"a\", \"b\", \"c\", \"a\", \"b\"] {\n\
        \x20   a[w] = (map.get(a, w) ?? 0) + 1\n\
        \x20   b[w] = step * (map.get(b, w) ?? 1) - 1\n\
        \x20   c[w] = 100 / ((map.get(c, w) ?? 4) + 1)\n\
        \x20   h[w] = (map.get(h, w) ?? 0) + 1\n\
        \x20 }\n\
        \x20 for n in [3, 1, 3, 2, 3] {\n\
        \x20   f[n] = -(map.get(f, n) ?? 1.5) * 2.0 / 4.0\n\
        \x20 }\n\
        \x20 println(show(a))\n\
        \x20 println(show(b))\n\
        \x20 println(show(c))\n\
        \x20 println(show(h))\n\
        \x20 println(map.to_list(f) |> list.map(((k, v)) => \"${k}=${v}\") |> list.join(\",\"))\n\
        \x20 var z: Map[String, Int] = [\"k\": 4]\n\
        \x20 let k = \"k\"\n\
        \x20 z[k] = 8 / (map.get(z, k) ?? 0)\n\
        \x20 println(show(z))\n\
        }\n";
    let (_dir, path) = write("fused.almd", src);
    let out = Command::new(almide()).arg("run").arg(&path).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "b=3,a=2,c=1\nb=1,a=1,c=1\nb=20,a=4,c=20\nb=-9223372036854775806,a=2,c=1\n3=-0.1875,1=-0.75,2=-0.75\nk=2\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
