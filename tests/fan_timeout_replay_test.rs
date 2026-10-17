//! #3781: `ALMIDE_OMEGA` replay of a `fan.timeout` worked on native only —
//! wasm never read it — and the replay test passed vacuously (its fixture
//! prints `-1` for every cut). Here the body PRINTS its progress, so the cut
//! point is observable: each ω must give byte-identical output on native and
//! wasm, and different ωs must cut at different points.
use std::process::Command;

const SOURCE: &str = r#"fn loud(n: Int) -> Int = {
  var i = 0
  while i < n {
    println("t-iter " + int.to_string(i))
    i = i + 1
  }
  i
}

effect fn main() -> Unit = {
  let p = fan.timeout(duration.s(60)) { loud(20) } ?? -1
  println("p=" + int.to_string(p))
}
"#;

fn run(file: &std::path::Path, wasm: bool, omega: Option<&str>) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    c.env_remove("ALMIDE_OMEGA").env_remove("ALMIDE_OMEGA_RECORD");
    if let Some(n) = omega {
        c.env("ALMIDE_OMEGA", n);
    }
    let out = c.output().expect("almide");
    assert!(out.status.success(), "wasm={wasm} omega={omega:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn an_omega_replays_the_same_cut_on_native_and_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("replay.almd");
    std::fs::write(&file, SOURCE).expect("source");
    // live: a 60 s deadline never fires
    let live = run(&file, false, None);
    assert!(live.ends_with("p=20\n"), "{live}");
    assert_eq!(run(&file, true, None), live, "live native vs wasm");
    let mut seen = Vec::new();
    for omega in ["3", "10", "40"] {
        let native = run(&file, false, Some(omega));
        assert!(native.ends_with("p=-1\n"), "omega {omega} must cut: {native}");
        assert_eq!(run(&file, true, Some(omega)), native, "omega {omega}: native vs wasm");
        seen.push(native);
    }
    assert!(seen[0] != seen[1] && seen[1] != seen[2], "different omegas must cut at different points");
}
