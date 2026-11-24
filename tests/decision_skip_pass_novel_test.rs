//! Row `env.skip-pass`: `ALMIDE_SKIP_PASS` (an Ablation switch, read only off
//! the release lock) skips the named codegen passes. The names are read
//! against the passes every target's pipeline DECLARES
//! (`almide_codegen::target::every_pass_name`): a name that is no declared
//! pass would skip nothing while the hunt reads "output unchanged", so it is
//! refused. Driven through the CLI so the switch never enters this process.
#[path = "support/novel.rs"]
mod novel;

use std::process::Command;

#[test]
fn novel_a_skip_pass_naming_no_declared_pass_is_refused() {
    let td = tempfile::TempDir::new().unwrap();
    let f = novel::ident("f");
    std::fs::write(td.path().join("main.almd"), format!("fn {f}() -> Int = 1\n\neffect fn main() -> Unit = println(int.to_string({f}()))\n")).unwrap();
    let emit = |skip: Option<&str>| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.current_dir(td.path()).args(["main.almd", "--target", "rust"]).env_remove("ALMIDE_SKIP_PASS");
        if let Some(s) = skip {
            c.env("ALMIDE_SKIP_PASS", s);
        }
        c.output().unwrap()
    };
    let text = |o: &std::process::Output| format!("exit {:?}\n{}{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    // Control: without the switch the program emits.
    let o = emit(None);
    assert!(o.status.success(), "control: the program does not emit:\n{}", text(&o));
    // A fresh pass name no pipeline declares, alone and after an empty item.
    let pass = novel::ident("NovelPass");
    for skip in [pass.clone(), format!(",{pass}")] {
        let o = emit(Some(&skip));
        let t = text(&o);
        assert!(!o.status.success(), "ALMIDE_SKIP_PASS={skip} skipped nothing and emitted:\n{t}");
        assert!(t.contains(&format!("ALMIDE_SKIP_PASS names `{pass}`, which is no codegen pass")), "{t}");
    }
    // Every declared name is known to the check (none is refused as unknown).
    let known = almide::codegen::target::every_pass_name();
    assert!(known.len() > 10, "{known:?}");
}
