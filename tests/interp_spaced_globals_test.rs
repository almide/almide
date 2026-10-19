//! #1602: the interp's global init is SPACED. Separately-lowered modules
//! (the wasm_leg front) each restart `VarId`s at 0, so the interp's old
//! bare-`VarId` global index collided across spaces — `by_var.insert` kept
//! the LAST module's initializer for every colliding id, and one shared
//! frame bound one slot for what are N distinct globals (the #1087
//! wrong-source class, global edition). Init identity is now `(space,
//! VarId)`, each module's top-lets live in their own frame, a lowered fn's
//! hop frame parents off ITS space's frame, and a cross-space read is
//! pre-bound through the alias table at init. The increment-1 boundary: a
//! MUTABLE global aliased across spaces abstains by name (`Unsupported`),
//! never votes wrong.

use std::path::Path;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

/// Lower a multi-module project through the STRUCTURAL front
/// (`wasm_leg::lower_to_ir` — per-module tables, restarting VarIds) and run
/// the interp on the linked IR.
fn run_project(tag: &str, files: &[(&str, &str)]) -> almide_interp::RunOutcome {
    let root = std::env::temp_dir().join(format!("almide-interp-spaced-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    for (name, content) in files {
        write(&root.join(name), content);
    }
    let entry = root.join("main.almd");
    let source = std::fs::read_to_string(&entry).expect("read entry");
    let ir = almide::wasm_leg::lower_to_ir(entry.to_str().unwrap(), &source)
        .expect("front failed");
    almide_interp::Interpreter::new(&ir).run_main()
}

/// Two modules whose top-lets collide on VarId under separate lowering —
/// each module's fn must read ITS OWN global, and the entry's direct
/// cross-module reads must resolve through the alias binds.
#[test]
fn colliding_module_globals_resolve_per_space() {
    let out = run_project("collide", &[
        (
            "alpha.almd",
            "pub let TAG = \"alpha\"\n\npub fn get() -> String = TAG\n",
        ),
        (
            "beta.almd",
            "pub let TAG = \"beta\"\n\npub fn get() -> String = TAG\n",
        ),
        (
            "main.almd",
            "import alpha\nimport beta\n\nfn main() -> Unit = {\n  println(alpha.get())\n  println(beta.get())\n  println(alpha.TAG)\n  println(beta.TAG)\n}\n",
        ),
    ]);
    assert_eq!(
        out.status,
        almide_interp::RunStatus::Ok,
        "run failed: stderr=<{}>",
        out.stderr
    );
    assert_eq!(out.stdout, "alpha\nbeta\nalpha\nbeta\n", "wrong global resolution");
}

/// A module global whose initializer reads ANOTHER module's global — the
/// alias bind must land before the topo-later initializer evaluates.
#[test]
fn cross_module_initializer_reads_through_alias() {
    let out = run_project("xinit", &[
        ("base.almd", "pub let NAME = \"almide\"\n"),
        (
            "banner.almd",
            "import base\n\npub let LINE = \"[\" + base.NAME + \"]\"\n",
        ),
        (
            "main.almd",
            "import banner\n\nfn main() -> Unit = {\n  println(banner.LINE)\n}\n",
        ),
    ]);
    assert_eq!(
        out.status,
        almide_interp::RunStatus::Ok,
        "run failed: stderr=<{}>",
        out.stderr
    );
    assert_eq!(out.stdout, "[almide]\n");
}

/// The increment-1 boundary: a mutable module global read from another
/// space abstains by name — an honest skip, never a stale-value vote.
#[test]
fn mutable_cross_space_alias_abstains() {
    let out = run_project("mutalias", &[
        (
            "counter.almd",
            "pub var count = 0\n\npub fn bump() -> Unit = {\n  count = count + 1\n}\n",
        ),
        (
            "main.almd",
            "import counter\n\nfn main() -> Unit = {\n  counter.bump()\n  println(\"${counter.count}\")\n}\n",
        ),
    ]);
    assert!(
        matches!(out.status, almide_interp::RunStatus::Unsupported(_)),
        "expected the mutable cross-space abstain; got {:?} stdout=<{}> stderr=<{}>",
        out.status,
        out.stdout,
        out.stderr
    );
}

/// Same-space mutability keeps evaluating: a module's own fns mutating the
/// module's own `var` global is in-space and fully modeled.
#[test]
fn same_space_mutable_global_still_evaluates() {
    let out = run_project("mutlocal", &[
        (
            "counter.almd",
            "var count = 10\n\npub fn bump() -> Unit = {\n  count = count + 1\n}\n\npub fn read() -> Int = count\n",
        ),
        (
            "main.almd",
            "import counter\n\nfn main() -> Unit = {\n  counter.bump()\n  counter.bump()\n  println(\"${counter.read()}\")\n}\n",
        ),
    ]);
    assert_eq!(
        out.status,
        almide_interp::RunStatus::Ok,
        "run failed: stderr=<{}>",
        out.stderr
    );
    assert_eq!(out.stdout, "12\n");
}

/// #3825, the module edition: a module body's TAIL call of its own private
/// sibling (`step` inside `walker.count`) means the module's `step`, not the
/// entry program's fn of the same spelling. The ordinary call path resolved
/// the sibling first (#1844); the tail-call spine took the flat table's root
/// `step`. The expected lines are native's (`almide run`).
#[test]
fn a_module_tail_call_reaches_its_own_sibling_not_the_roots_namesake() {
    let out = run_project("tailsibling", &[
        (
            "walker.almd",
            "fn step(n: Int, acc: Int) -> Int = if n == 0 then acc else step(n - 1, acc + 1)\n\npub fn count(n: Int) -> Int = step(n, 0)\n",
        ),
        (
            "main.almd",
            "import walker\n\nfn step(n: Int, acc: Int) -> Int = if n > 100 then step(n - 1, acc) else 999 + n + acc\n\nfn main() -> Unit = {\n  println(int.to_string(walker.count(3)))\n  println(int.to_string(step(1, 2)))\n}\n",
        ),
    ]);
    assert_eq!(out.status, almide_interp::RunStatus::Ok, "run failed: stderr=<{}>", out.stderr);
    assert_eq!(out.stdout, "3\n1002\n");
}
