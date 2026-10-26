//! The module interface carries the `mut` parameter modifier. Before it did,
//! `almide compile <module> --json` dropped it, so every surface rendered from
//! that JSON — the docs/stdlib signature indexes, the stdlib README, the
//! release interface diff — showed `bytes.push(b: Bytes, …)` for a
//! function whose caller must pass a `var` (E032 otherwise), and a reader of
//! the docs could not tell an in-place writer from a copying one.
//!
//! Derived from the declarations, never from a list of names: for every public
//! function of every stdlib module, the parameters the interface marks `mut`
//! are exactly the ones its `.almd` declaration spells `mut`.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn interface_json(module: &str) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["compile", module, "--json"])
        .output()
        .expect("run almide compile --json");
    assert!(out.status.success(), "compile {module} --json failed:\n{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("interface JSON parses")
}

/// The `mut` parameter names of `fn <name>` as declared in the module's
/// sources (`stdlib/<module>.almd` and its `<module>_*.almd` parts); `None`
/// when no declaration line is found.
fn declared_mut(module: &str, name: &str) -> Option<BTreeSet<String>> {
    let head = format!("fn {name}");
    for entry in std::fs::read_dir(root().join("stdlib")).unwrap().flatten() {
        let file = entry.file_name().to_string_lossy().to_string();
        let Some(stem) = file.strip_suffix(".almd") else { continue };
        if stem != module && !stem.starts_with(&format!("{module}_")) {
            continue;
        }
        let src = std::fs::read_to_string(entry.path()).unwrap();
        for line in src.lines() {
            let decl = line.strip_prefix("effect ").unwrap_or(line);
            let Some(rest) = decl.strip_prefix(&head) else { continue };
            if !(rest.starts_with('(') || rest.starts_with('[')) {
                continue;
            }
            let params = &rest[rest.find('(').unwrap() + 1..];
            return Some(
                params
                    .split(['(', ','])
                    .filter_map(|p| p.trim_start().strip_prefix("mut ")?.split(':').next().map(|n| n.trim().to_string()))
                    .collect(),
            );
        }
    }
    None
}

#[test]
fn every_stdlib_mut_parameter_is_marked_in_the_interface() {
    let mut modules: Vec<String> = std::fs::read_dir(root().join("docs/stdlib"))
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".md").map(str::to_string))
        .filter(|m| m != "README")
        // A page with no `.almd` source (the clock constructors, which the
        // checker resolves from time_units.rs) has no declaration to compare.
        .filter(|m| root().join(format!("stdlib/{m}.almd")).exists())
        .collect();
    modules.sort();
    let (mut checked, mut marked, mut wrong) = (0, 0, Vec::new());
    for module in &modules {
        let iface = interface_json(module);
        for f in iface["functions"].as_array().into_iter().flatten() {
            let name = f["name"].as_str().unwrap();
            let Some(declared) = declared_mut(module, name) else { continue };
            let in_iface: BTreeSet<String> = f["params"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["mut"].as_bool() == Some(true))
                .map(|p| p["name"].as_str().unwrap().to_string())
                .collect();
            checked += 1;
            marked += in_iface.len();
            if in_iface != declared {
                wrong.push(format!("{module}.{name}: declared mut {declared:?}, interface mut {in_iface:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "interface `mut` disagrees with the declaration:\n{}", wrong.join("\n"));
    assert!(checked > 500, "only {checked} declarations found — the source scan lost its corpus");
    assert!(marked > 0, "no `mut` parameter in any interface — the modifier is not exported");
}
