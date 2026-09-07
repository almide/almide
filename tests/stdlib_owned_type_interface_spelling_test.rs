//! The interface spelling of a STDLIB-OWNED type name (#1828): a user
//! declaration of `Endian` / `FileStat` / … takes the #433 module-qualified
//! identity (`self.Endian` in the entry program) so the stdlib's bare key is
//! never rebound — but the owning module's OWN declaration is that bare key,
//! also when the module is compiled on its own (`almide compile bytes --json`
//! stages the bundled source as the entry program). Both spellings are
//! pinned here because the second one feeds every consumer of the module
//! interface JSON: the wasm reachability sweep keys its argument synthesizer
//! on `"Endian"` (a `self.Endian` made the twelve endian-taking `bytes.*`
//! fns UNPROBEABLE), the committed doc signature indexes, the target
//! availability probe, and the release interface diff (where a `self.`
//! spelling would read as a breaking change).

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// `almide compile <target> --json`, parsed.
fn interface_json(target: &str) -> serde_json::Value {
    let out = Command::new(almide())
        .args(["compile", target, "--json"])
        .output()
        .expect("run almide compile --json");
    assert!(
        out.status.success(),
        "compile {target} --json failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("interface JSON parses")
}

/// Every nominal type name the interface spells: each declared type's name
/// and every `named` type reference (fn params, returns, fields, constants).
fn spelled_type_names(iface: &serde_json::Value) -> Vec<String> {
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(map) => {
                if map.get("kind").and_then(|k| k.as_str()) == Some("named") {
                    if let Some(n) = map.get("name").and_then(|n| n.as_str()) {
                        out.push(n.to_string());
                    }
                }
                for child in map.values() {
                    walk(child, out);
                }
            }
            serde_json::Value::Array(xs) => xs.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for t in iface["types"].as_array().into_iter().flatten() {
        if let Some(n) = t["name"].as_str() {
            out.push(n.to_string());
        }
    }
    walk(iface, &mut out);
    out
}

/// The owning module compiled on its own spells its own types BARE — the
/// identity every stdlib signature carries — for every row of the registry.
#[test]
fn bundled_owner_modules_spell_their_own_types_bare() {
    let mut owners: Vec<&str> = almide_lang::stdlib_info::STDLIB_OWNED_TYPES
        .iter()
        .map(|(m, _)| *m)
        .collect();
    owners.sort_unstable();
    owners.dedup();
    for module in owners {
        let names = spelled_type_names(&interface_json(module));
        let scoped: Vec<&String> = names.iter().filter(|n| n.starts_with("self.")).collect();
        assert!(
            scoped.is_empty(),
            "`almide compile {module} --json` spells a stdlib type under the entry \
             program's shadow scope — the bundled module's own declaration must keep \
             the bare key: {scoped:?}"
        );
        for (_, ty) in almide_lang::stdlib_info::STDLIB_OWNED_TYPES.iter().filter(|(m, _)| *m == module) {
            assert!(
                names.iter().any(|n| n == ty),
                "`almide compile {module} --json` never spells its own `{ty}` bare; spelled: {names:?}"
            );
        }
    }
}

/// A USER entry program declaring a stdlib-owned name keeps the shadow scope
/// (`self.Endian`) — that is #1828's point — while its other types stay bare.
#[test]
fn entry_program_shadow_of_an_owned_name_keeps_its_scope() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("m.almd");
    std::fs::write(
        &path,
        r#"type Endian = { n: Int }
type Pt = { x: Int }

fn mk(n: Int) -> Endian = Endian { n: n }
fn pt(x: Int) -> Pt = Pt { x: x }
"#,
    )
    .expect("write module source");
    let names = spelled_type_names(&interface_json(path.to_str().unwrap()));
    let count = |s: &str| names.iter().filter(|n| n.as_str() == s).count();
    assert_eq!(count("self.Endian"), 2, "the declaration and mk's return: {names:?}");
    assert_eq!(count("Endian"), 0, "the bare spelling is the stdlib's: {names:?}");
    assert_eq!(count("Pt"), 2, "an unowned name keeps its bare identity: {names:?}");
    assert_eq!(count("self.Pt"), 0, "no shadow scope for an unowned name: {names:?}");
}
