//! The interface spelling of a STDLIB-OWNED type name (#1828): a user
//! declaration of `IpAddr` / `FileStat` / … takes the #433 module-qualified
//! identity (`self.IpAddr` in the entry program) so the stdlib's bare key is
//! never rebound — but the owning module's OWN declaration is that bare key,
//! also when the module is compiled on its own (`almide query interface net --json`
//! stages the bundled source as the entry program). Both spellings are
//! pinned here because the second one feeds every consumer of the module
//! interface JSON: the wasm reachability sweep keys its argument synthesizer
//! on the bare owned name (a `self.`-spelled owned type made every fn taking
//! it UNPROBEABLE), the committed doc signature indexes, the target
//! availability probe, and the release interface diff (where a `self.`
//! spelling would read as a breaking change).

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// `almide query interface <target> --json`, parsed.
fn interface_json(target: &str) -> serde_json::Value {
    let out = Command::new(almide())
        .args(["query", "interface", target, "--json"])
        .output()
        .expect("run almide query interface --json");
    assert!(
        out.status.success(),
        "query interface {target} --json failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).expect("interface JSON parses")["result"].clone()
}

/// Every nominal type name the interface spells: each declared type's name
/// and every `named` type reference (fn params, returns, fields, constants).
fn spelled_type_names(iface: &serde_json::Value) -> Vec<String> {
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(map) => {
                if map.get("kind").and_then(|k| k.as_str()) == Some("named")
                    && let Some(n) = map.get("name").and_then(|n| n.as_str()) {
                        out.push(n.to_string());
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

/// Does `module`'s bundled source declare `ty` as a public `type` (not a
/// `mod type` or `local type`)? Read from the declaration, as
/// `stdlib_owned_types` reads it; a runtime-backed or builtin row is public.
fn declared_public(module: &str, ty: &str) -> bool {
    let Some(src) = almide_lang::stdlib_info::bundled_source(module) else { return true };
    let private = src.lines().any(|l| {
        ["mod type ", "local type "].iter().any(|p| {
            l.strip_prefix(p).is_some_and(|rest| {
                rest.strip_prefix(ty).is_some_and(|after| !after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_'))
            })
        })
    });
    !private
}

/// The owning module compiled on its own spells its own types BARE — the
/// identity every stdlib signature carries — for every row of the registry.
#[test]
fn bundled_owner_modules_spell_their_own_types_bare() {
    let mut owners: Vec<&str> = almide_lang::stdlib_info::stdlib_owned_types()
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
            "`almide query interface {module} --json` spells a stdlib type under the entry \
             program's shadow scope — the bundled module's own declaration must keep \
             the bare key: {scoped:?}"
        );
        for (_, ty) in almide_lang::stdlib_info::stdlib_owned_types().iter().filter(|(m, _)| *m == module) {
            // A `mod` / `local` type that no public signature names is not part
            // of the interface at all (ffi's `RegionExportView`, which only the
            // compiler-made export wrapper reaches); where any type is spelled,
            // the scoped check above holds it bare.
            if !declared_public(module, ty) && !names.iter().any(|n| n.ends_with(&format!(".{ty}"))) {
                continue;
            }
            assert!(
                names.iter().any(|n| n == ty),
                "`almide query interface {module} --json` never spells its own `{ty}` bare; spelled: {names:?}"
            );
        }
    }
}

/// A USER entry program declaring a stdlib-owned name keeps the shadow scope
/// (`self.IpAddr`) — that is #1828's point — while its other types stay bare.
#[test]
fn entry_program_shadow_of_an_owned_name_keeps_its_scope() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("m.almd");
    std::fs::write(
        &path,
        r#"type IpAddr = { n: Int }
type Pt = { x: Int }

fn mk(n: Int) -> IpAddr = IpAddr { n: n }
fn pt(x: Int) -> Pt = Pt { x: x }
"#,
    )
    .expect("write module source");
    let names = spelled_type_names(&interface_json(path.to_str().unwrap()));
    let count = |s: &str| names.iter().filter(|n| n.as_str() == s).count();
    assert_eq!(count("self.IpAddr"), 2, "the declaration and mk's return: {names:?}");
    assert_eq!(count("IpAddr"), 0, "the bare spelling is the stdlib's: {names:?}");
    assert_eq!(count("Pt"), 2, "an unowned name keeps its bare identity: {names:?}");
    assert_eq!(count("self.Pt"), 0, "no shadow scope for an unowned name: {names:?}");
}
