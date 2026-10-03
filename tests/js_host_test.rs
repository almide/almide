//! `almide build --target wasm --host js` (#2265) writes the JS host next to
//! the module: `<mod>.js` + `<mod>.d.ts`, the two extra exports the host
//! calls, and a build-time refusal for a boundary type it cannot marshal.
//! Running the glue under node is `scripts/check-js-host.sh`'s job; this file
//! pins what the compiler writes and refuses.
use std::process::Command;

const PROGRAM: &str = "@extern(wasm, \"js\", \"js_log\")\nfn js_log(msg: String) -> Unit\n\npub fn greet(n: Int) -> Int = n + 1\npub fn shout(s: String) -> String = string.to_upper(s)\nfn main() -> Unit = js_log(\"hi ${greet(1)}\")\n";

const UNMARSHALLABLE: &str = "pub fn total(xs: List[Int]) -> Int = list.len(xs)\nfn main() -> Unit = println(int.to_string(total([1])))\n";

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn build(dir: &std::path::Path, src: &str, args: &[&str]) -> (bool, String) {
    std::fs::write(dir.join("main.almd"), src).unwrap();
    let out = Command::new(almide()).current_dir(dir).args(["build", "main.almd"]).args(args).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

#[test]
fn host_js_writes_the_module_the_glue_and_the_typings() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dist")).unwrap();
    let (ok, stderr) = build(dir.path(), PROGRAM, &["--target", "wasm", "--host", "js", "-o", "dist/app.wasm"]);
    assert!(ok, "{stderr}");
    let line = stderr.lines().find(|l| l.starts_with("Built ")).unwrap_or_else(|| panic!("no Built line:\n{stderr}"));
    assert!(line.contains("dist/app.wasm + dist/app.js + dist/app.d.ts"), "{line}");
    let js = std::fs::read_to_string(dir.path().join("dist/app.js")).unwrap();
    let dts = std::fs::read_to_string(dir.path().join("dist/app.d.ts")).unwrap();
    let wasm = std::fs::read(dir.path().join("dist/app.wasm")).unwrap();
    // The glue loads the module by its own file name, exposes main as run(),
    // one wrapper per pub fn, and wires the extern through hooks.js.
    assert!(js.contains("new URL(\"./app.wasm\", import.meta.url)"), "{js}");
    assert!(js.contains("export async function init("), "{js}");
    assert!(js.contains("export function run()"), "{js}");
    assert!(js.contains("export function greet(n)") && js.contains("export function shout(s)"), "{js}");
    assert!(js.contains("jsImports.js_log = (a0) =>") && js.contains("readString(a0)"), "{js}");
    assert!(js.contains("wasiImports.fd_write = wasi.fd_write"), "{js}");
    // A String parameter goes in through the module's allocator. On the
    // structural leg (#2275: the extern import no longer routes the program
    // to the incumbent) ownership is per fn: `shout` hands `s` to
    // `string.to_upper`, so the callee owns the block and the host does not
    // release it after the call.
    assert!(line.contains("structural leg"), "{line}");
    assert!(js.contains("const h0 = allocString(s);"), "{js}");
    assert!(js.contains("takeString(instance.exports.shout(h0))") && !js.contains("__release(h0)"), "{js}");
    assert!(dts.contains("export function greet(n: number): number;"), "{dts}");
    assert!(dts.contains("export function shout(s: string): string;"), "{dts}");
    assert!(dts.contains("js_log: (msg: string) => void;"), "{dts}");
    assert!(dts.contains("export function run(): void;"), "{dts}");
    // The two host-called exports exist only under the switch, and only for a
    // surface that marshals a String (#2276): `shout` and `js_log` do.
    let has = |needle: &[u8]| wasm.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"__alloc") && has(b"__release"), "the host's allocator/release exports are missing");
    let (ok, stderr) = build(dir.path(), PROGRAM, &["--target", "wasm", "-o", "dist/plain.wasm"]);
    assert!(ok, "{stderr}");
    let plain = std::fs::read(dir.path().join("dist/plain.wasm")).unwrap();
    let has_plain = |needle: &[u8]| plain.windows(needle.len()).any(|w| w == needle);
    assert!(!has_plain(b"__alloc") && !has_plain(b"__release"), "a build without --host js must not carry the host exports");
}

const SCALAR_ONLY: &str = "pub fn fib(n: Int) -> Int = if n <= 1 then n else fib(n - 1) + fib(n - 2)\nfn main() -> Unit = println(int.to_string(fib(10)))\n";

/// A scalar-only surface ships nothing for a String (#2276): the module is
/// byte-identical to the build without `--host js`, the glue carries no
/// string helpers, and the `wasi` object holds exactly the shims the shipped
/// module imports.
#[test]
fn a_scalar_only_surface_keeps_the_module_bytes_and_ships_only_its_shims() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, stderr) = build(dir.path(), SCALAR_ONLY, &["--target", "wasm", "--host", "js", "-o", "app.wasm"]);
    assert!(ok, "{stderr}");
    let (ok, stderr) = build(dir.path(), SCALAR_ONLY, &["--target", "wasm", "-o", "plain.wasm"]);
    assert!(ok, "{stderr}");
    assert_eq!(std::fs::read(dir.path().join("app.wasm")).unwrap(), std::fs::read(dir.path().join("plain.wasm")).unwrap(), "--host js must not change a scalar-only module");
    let js = std::fs::read_to_string(dir.path().join("app.js")).unwrap();
    assert!(!js.contains("function allocString(") && !js.contains("function takeString(") && !js.contains("__alloc"), "no String on the surface, no string helpers:\n{js}");
    let start = js.find("const wasi = {\n").expect("the wasi object") + "const wasi = {\n".len();
    let object = &js[start..start + js[start..].find("\n};\n").expect("the object's end")];
    let shims: Vec<&str> = object
        .lines()
        .filter(|l| l.starts_with("  ") && !l.starts_with("   ") && l.as_bytes()[2].is_ascii_lowercase() && l.contains('('))
        .map(|l| l.trim_start().split('(').next().unwrap())
        .collect();
    // The shipped module imports only what its reached code calls (#3136):
    // `fib` prints and can neither trap nor exit, so `fd_write` alone.
    assert_eq!(shims, ["fd_write"], "{shims:?}");
    assert!(!shims.contains(&"path_open") && !shims.contains(&"poll_oneoff") && !shims.contains(&"fd_readdir"), "unlinked shims must not ship: {shims:?}");
    for name in &shims {
        assert!(js.contains(&format!("wasiImports.{name} = wasi.{name};")), "every emitted shim is wired: {name}\n{js}");
    }
    for line in js.lines().filter(|l| l.contains("wasiImports.")) {
        let name = line.trim().trim_start_matches("wasiImports.").split(' ').next().unwrap();
        assert!(shims.contains(&name), "every wired import has its shim: {name}");
    }
}

#[test]
fn a_boundary_type_the_host_cannot_marshal_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, stderr) = build(dir.path(), UNMARSHALLABLE, &["--target", "wasm", "--host", "js", "-o", "app.wasm"]);
    assert!(!ok, "a List on the boundary must be refused:\n{stderr}");
    assert!(stderr.contains("--host js cannot marshal parameter `xs` of `total`"), "{stderr}");
    assert!(stderr.contains("List"), "{stderr}");
    assert!(!dir.path().join("app.js").exists() && !dir.path().join("app.wasm").exists(), "a refused build writes nothing");
    // The same program builds without the switch: the refusal is the host's, not the module's.
    let (ok, stderr) = build(dir.path(), UNMARSHALLABLE, &["--target", "wasm", "-o", "app.wasm"]);
    assert!(ok, "{stderr}");
}

/// `@extern(wasm, module, name)` on the structural leg (#2275): the fn is a
/// declared import of the module (no body, no stub), calls reach it by
/// index, and the JS host serves it from `init({ js: { name } })`.
#[test]
fn a_wasm_extern_is_a_declared_import_of_the_structural_module() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, stderr) = build(dir.path(), PROGRAM, &["--target", "wasm", "-o", "app.wasm"]);
    assert!(ok, "{stderr}");
    assert!(stderr.contains("structural leg"), "{stderr}");
    let wasm = std::fs::read(dir.path().join("app.wasm")).unwrap();
    let mut imports: Vec<(String, String)> = Vec::new();
    let mut defined = 0usize;
    for payload in wasmparser::Parser::new(0).parse_all(&wasm) {
        match payload.unwrap() {
            wasmparser::Payload::ImportSection(r) => {
                for i in r.into_imports() {
                    let i = i.unwrap();
                    imports.push((i.module.to_string(), i.name.to_string()));
                }
            }
            wasmparser::Payload::FunctionSection(r) => defined = r.count() as usize,
            _ => {}
        }
    }
    assert!(imports.contains(&("js".to_string(), "js_log".to_string())), "{imports:?}");
    // The WASI imports come first; the program's own import follows them.
    let pos = imports.iter().position(|(m, _)| m == "js").unwrap();
    assert!(imports[..pos].iter().all(|(m, _)| m == "wasi_snapshot_preview1"), "{imports:?}");
    assert!(defined > 0);
    // `almide run --target wasm` has no host for it and says so, instead of
    // wasmtime's "unknown import" at instantiation.
    let out = Command::new(almide()).current_dir(dir.path()).args(["run", "main.almd", "--target", "wasm"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && stderr.contains("imports `js.js_log`") && stderr.contains("--host js"), "{stderr}");
}

const NATIVE_EXTERN: &str = "@extern(rs, \"fast_lib\", \"reverse_it\")\nfn reverse_it(s: String) -> String\n\nfn main() -> Unit = println(reverse_it(\"abc\"))\n";

/// A native (`rs`) extern has no wasm host: the wasm build refuses on both
/// legs instead of emitting a hollow import.
#[test]
fn a_native_extern_is_refused_on_the_wasm_target() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, stderr) = build(dir.path(), NATIVE_EXTERN, &["--target", "wasm", "-o", "app.wasm"]);
    assert!(!ok, "a native extern must not build for wasm:\n{stderr}");
    assert!(!dir.path().join("app.wasm").exists(), "a refused build writes nothing");
    let (ok, stderr) = build(dir.path(), NATIVE_EXTERN, &["--target", "wasm", "-o", "app.wasm"]);
    let _ = (ok, stderr);
    // The structural leg names the wall: ALMIDE_WASM_SKIP_STOCK_AUDIT turns the reroute into the reason.
    let out = Command::new(almide()).current_dir(dir.path()).env("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1").args(["build", "main.almd", "--target", "wasm", "-o", "app.wasm"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && stderr.contains("extern-native:reverse_it"), "{stderr}");
}

#[test]
fn host_is_a_wasm_option() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, stderr) = build(dir.path(), PROGRAM, &["--host", "js"]);
    assert!(!ok && stderr.contains("--host is a wasm option"), "{stderr}");
    let (ok, stderr) = build(dir.path(), PROGRAM, &["--target", "wasm", "--host", "wasi"]);
    assert!(!ok && stderr.contains("accepts only `js`"), "{stderr}");
}

/// The host bindings of a package in a module of their own (#2876, #2878):
/// `host.almd` declares one extern BODYLESS and one `= _`, a sibling fn that
/// calls the extern, and a plain fn. Every call from the entry — to either
/// extern, to the sibling, to the plain fn — lowers on the structural leg
/// (a bodyless decl used to hide the whole module; a module's extern used to
/// be no fn at all), both externs are declared imports of the module, and
/// the JS host serves them from `init({ js })`.
const HOST_MODULE: &str = "@extern(wasm, \"js\", \"js_twice\")\nfn js_twice(x: Int) -> Int\n\n@extern(wasm, \"js\", \"js_inc\")\nfn js_inc(x: Int) -> Int = _\n\nfn quad(x: Int) -> Int = js_twice(js_twice(x))\n\nfn probe() -> Int = 40\n";
const HOST_ENTRY: &str = "import self.host as host\n\nfn main() -> Unit = {\n  println(int.to_string(host.quad(3)))\n  println(int.to_string(host.js_inc(host.probe())))\n  println(int.to_string(host.js_twice(host.probe())))\n}\n";

#[test]
fn an_extern_declared_in_another_module_is_an_import_of_the_structural_module() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("almide.toml"), "[package]\nname = \"hostpkg\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(root.join("src/host.almd"), HOST_MODULE).unwrap();
    std::fs::write(root.join("src/main.almd"), HOST_ENTRY).unwrap();
    // The route-flip probe: a structural decline is the build's error, not a reroute.
    let out = Command::new(almide())
        .current_dir(root)
        .env("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1")
        .args(["build", "src/main.almd", "--target", "wasm", "--host", "js", "-o", "app.wasm"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && stderr.contains("structural leg"), "{stderr}");
    let wasm = std::fs::read(root.join("app.wasm")).unwrap();
    let mut imports: Vec<(String, String)> = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&wasm) {
        if let wasmparser::Payload::ImportSection(r) = payload.unwrap() {
            for i in r.into_imports() {
                let i = i.unwrap();
                imports.push((i.module.to_string(), i.name.to_string()));
            }
        }
    }
    for name in ["js_twice", "js_inc"] {
        assert!(imports.contains(&("js".to_string(), name.to_string())), "{name}: {imports:?}");
    }
    let dts = std::fs::read_to_string(root.join("app.d.ts")).unwrap();
    assert!(dts.contains("js_twice: (x: number) => number;") && dts.contains("js_inc: (x: number) => number;"), "{dts}");
    // Run the glue when node is on PATH (scripts/check-js-host.sh's CI job
    // installs it; locally its absence only skips the run).
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }
    std::fs::write(
        root.join("run.mjs"),
        "import { pathToFileURL } from \"node:url\";\nconst mod = await import(pathToFileURL(process.argv[2]).href);\nawait mod.init(undefined, { js: { js_twice: (x) => x * 2, js_inc: (x) => x + 1 } });\nmod.run();\n",
    )
    .unwrap();
    let run = Command::new("node").current_dir(root).args(["run.mjs", "app.js"]).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "12\n41\n80\n");
}
