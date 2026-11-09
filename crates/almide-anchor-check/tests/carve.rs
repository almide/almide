//! ADR-0033 D7: the family records (`k` / `c` / `e`) against hand-written
//! copies of the shapes crates/almide-wasm/src/carve.rs emits — each
//! accepted, and each one-instruction drift refused.

use almide_anchor_check::{check_init, decode, Report};

mod common;
use common::module;

/// Locals: 0 root, 1 cursor, 2 base, 3 the root's `let`, 4 len, 5 word
/// count, 6 size, 7 fill cursor, 8 the i64 length.
const LOCALS: &str = "(local i32 i32 i32 i32 i32 i32 i32 i32 i64)";

/// One instruction per line; a line ending `;;@` is an anchor. Answers the
/// body and the anchors of each `|`-separated record, `;`-joined.
fn shape(src: &str) -> (String, String) {
    let mut groups = vec![Vec::new()];
    let mut body = Vec::new();
    for line in src.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line == "|" {
            groups.push(Vec::new());
            continue;
        }
        if line.ends_with(";;@") {
            groups.last_mut().expect("a group").push(format!("1:{}", body.len()));
        }
        body.push(line.trim_end_matches(";;@").to_string());
    }
    let anchors = groups.iter().map(|g| g.join(" ")).collect::<Vec<_>>().join(";");
    (body.join("\n"), anchors)
}

fn check(src: &str, witness: &str) -> Result<Report, String> {
    let (body, anchors) = shape(src);
    let bytes = module(LOCALS, &body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_init(&bodies, 0, witness, &anchors, &mut r).map(|()| r)
}

const ABORT: &str = "if
  i32.const 0
  call $alloc
  i32.const 1
  call $alloc
  unreachable
  end ;;@";

fn family() -> String {
    "i64.const 44
     i32.wrap_i64
     i32.const 4
     i32.add
     call $alloc
     local.set 0 ;;@
     local.get 0
     i32.const 16
     i32.store offset=12 ;;@
     local.get 0
     local.set 3"
        .to_string()
}

/// The common head after the size is known: `size_i64` / `size_i32` push it.
fn head(size_i64: &str, size_i32: &str) -> String {
    format!(
        "local.get 0
         i32.load offset=12 ;;@
         local.set 1
         local.get 1
         i64.extend_i32_u
         {size_i64}
         i64.add
         local.get 0
         i32.load offset=4 ;;@
         i64.extend_i32_u
         i64.const 12
         i64.add
         i64.gt_u
         {ABORT}
         local.get 0
         local.get 1
         {size_i32}
         i32.add
         i32.store offset=12 ;;@
         local.get 0
         local.get 1
         i32.add
         local.set 2 ;;@
         local.get 2
         local.get 1
         i32.const -2147483648
         i32.or
         i32.store ;;@"
    )
}

/// `prim.carve(fam, 16)`: four payload words.
fn literal() -> String {
    let mut s = format!("local.get 3\nlocal.set 0\n{}", head("i64.const 28", "i32.const 28"));
    s.push_str(
        "
         local.get 2
         i32.const 16
         i32.store offset=4 ;;@
         local.get 2
         i32.const 16
         i32.store offset=8 ;;@",
    );
    for off in [12, 16, 20, 24] {
        s.push_str(&format!("\nlocal.get 2\ni32.const 0\ni32.store offset={off} ;;@"));
    }
    s.push_str("\nlocal.get 2\ni64.extend_i32_u\ndrop");
    s
}

/// `prim.carve(fam, len)` with a computed len: the countdown zero fill.
fn computed(step: i32) -> String {
    format!(
        "local.get 3
         local.set 0
         i64.const 5
         local.tee 8
         i64.const 2147418112
         i64.gt_u
         {ABORT}
         local.get 8
         i32.wrap_i64
         local.set 4
         local.get 4
         i32.const 3
         i32.add
         i32.const 2
         i32.shr_u
         local.set 5
         local.get 5
         i32.const 1
         local.get 5
         select
         local.set 5
         local.get 5
         i32.const 2
         i32.shl
         i32.const 12
         i32.add
         local.set 6
         {}
         local.get 2
         local.get 4
         i32.store offset=4 ;;@
         local.get 2
         local.get 5
         i32.const 2
         i32.shl
         i32.store offset=8 ;;@
         local.get 2
         i32.const 12
         i32.add
         local.set 7
         block
         loop
         local.get 5
         i32.eqz
         br_if 1
         local.get 7
         i32.const 0
         i32.store ;;@
         local.get 7
         i32.const {step}
         i32.add
         local.set 7
         local.get 5
         i32.const 1
         i32.sub
         local.set 5
         br 0
         end
         end
         local.get 2 ;;@
         i64.extend_i32_u
         drop",
        head("local.get 6\ni64.extend_i32_u", "local.get 6")
    )
}

fn seal(local: u32) -> String {
    format!(
        "local.get {local}
         local.set 0
         local.get 0
         i32.load offset=12 ;;@
         local.get 0
         i32.load offset=4 ;;@
         i32.const 12
         i32.add
         i32.ne
         {ABORT}"
    )
}

fn whole(carve: &str) -> String {
    format!("{}\n|\n{carve}\n|\n{}", family(), seal(3))
}

const LITERAL_W: &str = "k16;c12 w4 n4 s0 s1 s2 s3;e12";
const COMPUTED_W: &str = "k16;c12 w4 v5 f5 t4;e12";

#[test]
fn a_family_its_literal_carve_and_its_seal_are_accepted() -> Result<(), String> {
    let r = check(&whole(&literal()), LITERAL_W)?;
    assert_eq!((r.families, r.carves, r.carve_fills, r.seals), (1, 1, 0, 1));
    Ok(())
}

#[test]
fn a_computed_carve_is_accepted() -> Result<(), String> {
    let r = check(&whole(&computed(4)), COMPUTED_W)?;
    assert_eq!((r.families, r.carves, r.carve_fills, r.seals), (1, 0, 1, 1));
    Ok(())
}

#[test]
fn a_room_test_loosened_by_one_is_refused() {
    let src = whole(&literal()).replacen("i64.const 12", "i64.const 13", 1);
    assert!(check(&src, LITERAL_W).is_err());
}

#[test]
fn a_signed_room_test_is_refused() {
    let src = whole(&literal()).replace("i64.gt_u", "i64.gt_s");
    assert!(check(&src, LITERAL_W).is_err());
}

#[test]
fn a_misplaced_carve_store_is_refused() {
    let src = whole(&literal()).replace("i32.store offset=20 ;;@", "i32.store offset=28 ;;@");
    assert!(check(&src, LITERAL_W).is_err());
}

#[test]
fn a_cursor_advanced_by_less_than_the_size_is_refused() {
    let src = whole(&literal()).replacen("i32.const 28", "i32.const 24", 1);
    assert!(check(&src, LITERAL_W).is_err());
}

#[test]
fn a_record_that_drops_a_payload_word_is_refused() {
    assert!(check(&whole(&literal()), "k16;c12 w4 n4 s0 s1 s3;e12").is_err());
}

#[test]
fn a_zero_loop_that_skips_a_word_is_refused() {
    assert!(check(&whole(&computed(8)), COMPUTED_W).is_err());
}

#[test]
fn a_loop_that_counts_another_local_is_refused() {
    assert!(check(&whole(&computed(4)), "k16;c12 w4 v6 f6 t4;e12").is_err());
}

#[test]
fn an_unsealed_family_is_refused() {
    let src = format!("{}\n|\n{}", family(), literal());
    assert!(check(&src, "k16;c12 w4 n4 s0 s1 s2 s3").unwrap_err().contains("no seal"));
}

#[test]
fn a_seal_of_another_local_is_refused() {
    let src = format!("{}\n|\n{}\n|\n{}", family(), literal(), seal(4));
    assert!(check(&src, LITERAL_W).unwrap_err().contains("no seal"));
}
