//! The module interface keeps a transparent alias's name where the source
//! wrote it. The checker expands `type Handler = (Int) -> String`, so the IR
//! type of a parameter declared `Handler` is the bare fn type, and the
//! interface — and every surface rendered from it: the docs/stdlib signature
//! indexes, the stdlib README, the release interface diff — printed the
//! expansion (`http.serve(port: Int, handler: (HttpRequest) -> Result[…])`)
//! instead of the name the declaration used.
//!
//! The spelling comes from the declaration, never from a list of alias names:
//! a parameter written as the expanded type stays expanded even when an alias
//! of the same shape exists (the negative control below).

use std::path::PathBuf;
use std::process::Command;

fn interface_json(args: &[&str]) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["compile"])
        .args(args)
        .arg("--json")
        .output()
        .expect("run almide compile --json");
    assert!(out.status.success(), "compile {args:?} --json failed:\n{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("interface JSON parses")
}

fn function<'a>(iface: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    iface["functions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap_or_else(|| panic!("no fn {name} in the interface"))
}

fn named(name: &str) -> serde_json::Value {
    serde_json::json!({ "kind": "named", "name": name })
}

const SRC: &str = r#"
type Handler = (Int) -> String
type Step = effect (Int) -> String
type Layer = (Handler) -> Handler
type Route = { path: String, run: Handler }

fn route(path: String, run: Handler) -> Route = { path: path, run: run }

fn chain(h: Handler, layers: List[Layer]) -> Result[Handler, String] = ok(h)

effect fn first(s: Step, steps: List[Step]) -> Step = s

fn expanded(f: (Int) -> String) -> (Int) -> String = f
"#;

#[test]
fn a_written_alias_keeps_its_name_and_an_expanded_type_stays_expanded() {
    let dir: PathBuf = std::env::temp_dir().join(format!("almide-alias-spelling-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("aliases.almd");
    std::fs::write(&file, SRC).unwrap();
    let iface = interface_json(&[file.to_str().unwrap()]);

    let route = function(&iface, "route");
    assert_eq!(route["params"][1]["type"], named("Handler"), "{route:#}");

    let chain = function(&iface, "chain");
    assert_eq!(chain["params"][0]["type"], named("Handler"), "{chain:#}");
    assert_eq!(chain["params"][1]["type"], serde_json::json!({ "kind": "list", "inner": named("Layer") }), "{chain:#}");
    assert_eq!(chain["return"]["kind"], "result", "{chain:#}");
    assert_eq!(chain["return"]["ok"], named("Handler"), "{chain:#}");

    // An effect fn's declared return is its success type.
    let first = function(&iface, "first");
    assert_eq!(first["return"], named("Step"), "{first:#}");
    assert_eq!(first["params"][0]["type"], named("Step"), "{first:#}");
    assert_eq!(first["params"][1]["type"]["inner"], named("Step"), "{first:#}");

    // Negative control: the same shape written out is not renamed.
    let expanded = function(&iface, "expanded");
    assert_eq!(expanded["params"][0]["type"]["kind"], "fn", "{expanded:#}");
    assert_eq!(expanded["return"]["kind"], "fn", "{expanded:#}");

    // The type index spells nested aliases and record fields too.
    let types = iface["types"].as_array().unwrap();
    let ty = |n: &str| types.iter().find(|t| t["name"] == n).unwrap_or_else(|| panic!("no type {n}"));
    assert_eq!(ty("Layer")["kind"]["target"]["params"][0], named("Handler"), "{:#}", ty("Layer"));
    assert_eq!(ty("Layer")["kind"]["target"]["return"], named("Handler"), "{:#}", ty("Layer"));
    let run = ty("Route")["kind"]["fields"].as_array().unwrap().iter().find(|f| f["name"] == "run").unwrap().clone();
    assert_eq!(run["type"], named("Handler"), "{run:#}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_http_server_surface_is_spelled_with_its_aliases() {
    let iface = interface_json(&["http"]);
    let serve = function(&iface, "serve");
    assert_eq!(serve["params"][1]["type"], named("HttpHandler"), "{serve:#}");
    let wrap = function(&iface, "wrap");
    assert_eq!(wrap["params"][0]["type"], named("HttpHandler"), "{wrap:#}");
    assert_eq!(wrap["params"][1]["type"]["inner"], named("HttpMiddleware"), "{wrap:#}");
    assert_eq!(wrap["return"], named("HttpHandler"), "{wrap:#}");
    let router = function(&iface, "router");
    assert_eq!(router["return"]["ok"], named("HttpHandler"), "{router:#}");
}
