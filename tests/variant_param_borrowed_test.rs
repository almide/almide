//! A variant-typed param whose every `match` only READS its payloads is
//! passed by reference (`s: &Shape`): the arms bind `&T` payloads and no
//! caller clones the value to pass it. A match that moves a payload out
//! (returns it, builds it into a value) keeps the param owned — matching by
//! value moves the payload for free where a borrowed match would clone it.
//! The ownership certifier's C4 reads the same rule: an owned variant param
//! whose matches only read is a defect it names.
use std::process::Command;

const PROGRAM: &str = r#"type Shape = Circle(Float) | Rect(Float, Float) | Named(String, Shape)

fn area(s: Shape) -> Float = match s {
  Circle(r) => 3.0 * r * r,
  Rect(w, h) => w * h,
  Named(_, inner) => 1.0 + area(inner),
}

fn depth(s: Shape) -> Int = match s {
  Named(n, inner) => string.len(n) + depth(inner),
  _ => 0,
}

fn label(s: Shape) -> String = match s {
  Named(n, _) => n,
  _ => "anon",
}

fn main() -> Unit = {
  let s = Named("box", Rect(2.0, 3.0))
  println(float.to_string(area(s)))
  println(float.to_string(area(s)))
  println(int.to_string(depth(s)))
  println(label(s))
}
"#;

const RADIUS: &str = "type Shape = Circle(Float) | Rect(Float, Float)\n\nfn radius(s: Shape) -> Float = match s {\n  Circle(r) => r,\n  Rect(w, h) => w + h,\n}\n\nfn main() -> Unit = println(float.to_string(radius(Circle(1.5))))\n";

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn fn_sig_and_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    rust[start..].split("\n}").next().unwrap()
}

#[test]
fn a_variant_param_the_matches_only_read_is_borrowed() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let out = Command::new(almide_bin()).arg(&file).arg("--target").arg("rust").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).into_owned();
    // Scalar payloads and a payload handed to a borrowed slot: by reference.
    assert!(fn_sig_and_body(&rust, "area").starts_with("pub fn area(s: &Shape)"), "{}", fn_sig_and_body(&rust, "area"));
    assert!(fn_sig_and_body(&rust, "depth").starts_with("pub fn depth(s: &Shape)"), "{}", fn_sig_and_body(&rust, "depth"));
    // A payload returned: owned, the match moves it out.
    assert!(fn_sig_and_body(&rust, "label").starts_with("pub fn label(s: Shape)"), "{}", fn_sig_and_body(&rust, "label"));
    let m = fn_sig_and_body(&rust, "__almide_main");
    assert!(m.matches("area(&s)").count() == 2 && m.contains("depth(&s)"), "callers pass a borrow, never a clone:\n{m}");
    assert!(!m.contains("area(s.clone())"), "{m}");
    for target in ["rust", "wasm"] {
        let run = Command::new(almide_bin()).args(["run", file.to_str().unwrap(), "--target", target]).output().unwrap();
        assert!(run.status.success(), "{target}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "7.0\n7.0\n3\nbox", "{target}");
    }
}

#[test]
fn the_certifier_names_an_owned_variant_param_whose_matches_only_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, RADIUS).unwrap();
    let out = Command::new(almide_bin()).arg(&file).arg("--target").arg("rust").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("pub fn radius(s: &Shape)"));
    // Force every param owned: the certifier's C4 names the variant param.
    let out = Command::new(almide_bin())
        .env("ALMIDE_BORROW_OWN_ALL", "1")
        .env("ALMIDE_CERTIFY_OWNERSHIP", "report")
        .arg(&file).arg("--target").arg("rust")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[C4 owned-never-consumed] radius: param `s"), "{stderr}");
}
