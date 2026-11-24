//! env.ci-slice (proofs/decision-sites.toml): under `ALMIDE_CI_SLICE=k` a
//! gate judges 1 in k fixtures, and every fixture the change itself touches
//! joins the slice. The fixtures here are fresh paths no list can hold
//! (tests/support/novel.rs); the switch is read once per process, so the
//! selection runs in a child of this test binary with the switch set.
#[path = "../../../tests/support/novel.rs"]
mod novel;

use std::process::{Command, Output};

#[test]
#[ignore = "the child half of novel_a_fresh_fixture_the_change_touches_joins_the_slice"]
fn slice_child() {
    let keys: Vec<String> = std::env::var("NOVEL_KEYS").unwrap_or_default().split(',').map(String::from).collect();
    let kept = almide_corpus::select(keys, "novel", |k| k.clone());
    println!("KEPT={}", kept.join(","));
}

fn child(slice: &str, touched: &std::path::Path, keys: &[String]) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "slice_child", "--ignored", "--nocapture", "--test-threads", "1"])
        .env("ALMIDE_CI_SLICE", slice)
        .env("ALMIDE_CI_TOUCHED", touched)
        .env("NOVEL_KEYS", keys.join(","))
        .env_remove("CI")
        .env_remove("ALMIDE_CI_BASE")
        .env_remove("ALMIDE_CI_SEED")
        .output()
        .expect("the test binary runs")
}

#[test]
fn novel_a_fresh_fixture_the_change_touches_joins_the_slice() {
    let keys: Vec<String> = (0..64).map(|_| format!("spec/{}/{}.almd", novel::path_segment(), novel::ident("fx"))).collect();
    let touched = &keys[..6];
    let td = tempfile::tempdir().unwrap();
    let list = td.path().join("touched.txt");
    std::fs::write(&list, touched.join("\n")).unwrap();
    let o = child("8", &list, &keys);
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "the child failed:\n{out}{}", String::from_utf8_lossy(&o.stderr));
    let kept: Vec<&str> = out.lines().find_map(|l| l.split_once("KEPT=").map(|(_, k)| k)).expect("the child printed its selection").split(',').collect();
    for t in touched {
        assert!(kept.contains(&t.as_str()), "a fixture the change touches was left out of the slice: {t}\nkept: {kept:?}");
    }
    assert!(kept.len() < keys.len(), "the slice judged all {} fixtures", keys.len());
    // A switch value that is no whole number is refused, never read as a full run.
    let o = child(&novel::ident("k"), &list, &keys);
    assert!(!o.status.success(), "a fresh non-number ALMIDE_CI_SLICE was taken:\n{}", String::from_utf8_lossy(&o.stdout));
}
