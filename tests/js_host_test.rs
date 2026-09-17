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
    // A String parameter goes in through the module's allocator; the
    // incumbent's callee borrows it, so the host releases after the call.
    assert!(js.contains("const h0 = allocString(s);") && js.contains("instance.exports.__release(h0);"), "{js}");
    assert!(dts.contains("export function greet(n: number): number;"), "{dts}");
    assert!(dts.contains("export function shout(s: string): string;"), "{dts}");
    assert!(dts.contains("js_log: (msg: string) => void;"), "{dts}");
    assert!(dts.contains("export function run(): void;"), "{dts}");
    // The two host-called exports exist only under the switch.
    let has = |needle: &[u8]| wasm.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"__alloc") && has(b"__release"), "the host's allocator/release exports are missing");
    let (ok, stderr) = build(dir.path(), PROGRAM, &["--target", "wasm", "-o", "dist/plain.wasm"]);
    assert!(ok, "{stderr}");
    let plain = std::fs::read(dir.path().join("dist/plain.wasm")).unwrap();
    let has_plain = |needle: &[u8]| plain.windows(needle.len()).any(|w| w == needle);
    assert!(!has_plain(b"__alloc") && !has_plain(b"__release"), "a build without --host js must not carry the host exports");
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

#[test]
fn host_is_a_wasm_option() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, stderr) = build(dir.path(), PROGRAM, &["--host", "js"]);
    assert!(!ok && stderr.contains("--host is a wasm option"), "{stderr}");
    let (ok, stderr) = build(dir.path(), PROGRAM, &["--target", "wasm", "--host", "wasi"]);
    assert!(!ok && stderr.contains("accepts only `js`"), "{stderr}");
}
