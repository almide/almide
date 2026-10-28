//! #4287 — a generic fn that reaches itself at a growing type is E114 at
//! check time, on the default route and on the wasm build route, instead of
//! an E082 wall or a rustc E0308 after `check` accepted it.
//!
//! The other half: recursion at the SAME type, a cycle that only swaps type
//! parameters, mutual recursion at one type and acyclic growth are all
//! accepted, and run with the same output on the native and the wasm leg.
//!
//! Skips cleanly when the `almide` binary is unavailable; the wasm leg also
//! needs wasmtime.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Write `src` to a fresh file and run `almide <args..> <file>`;
/// (success, stdout, stderr).
fn run(name: &str, src: &str, args: &[&str]) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("almide-polyrec-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide_bin())
        .arg(args[0])
        .arg(&file)
        .args(&args[1..])
        .output()
        .expect("failed to spawn almide");
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

const REFUSED: &[(&str, &str, &str)] = &[
    (
        "issue-4287",
        "fn depth[T: Hash](x: T, n: Int) -> Int =\n  if n <= 0 then 0 else 1 + depth((x, x), n - 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(depth(1, 3)))\n}\n",
        "`depth` calls itself at a growing type: its `T` is `(T, T)`",
    ),
    (
        "unbounded-list",
        "fn nest[T](x: T, n: Int) -> Int =\n  if n <= 0 then 0 else 1 + nest([x], n - 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(nest(\"a\", 3)))\n}\n",
        "`nest` calls itself at a growing type: its `T` is `List[T]`",
    ),
    (
        "pipe-form",
        "fn depth[T](x: T, n: Int) -> Int =\n  if n <= 0 then 0 else 1 + ((x, x) |> depth(n - 1))\n\neffect fn main() -> Unit = {\n  println(int.to_string(depth(1, 3)))\n}\n",
        "`depth` calls itself at a growing type",
    ),
    (
        "mutual",
        "fn wrap[T](x: T, n: Int) -> Int =\n  if n <= 0 then 0 else unwrap([x], n - 1)\n\nfn unwrap[U](y: U, n: Int) -> Int =\n  if n <= 0 then 0 else 1 + wrap(y, n - 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(wrap(\"a\", 4)))\n}\n",
        "`wrap` reaches itself through `unwrap` at a growing type",
    ),
    (
        "three-fn-cycle",
        "fn a[T](x: T, n: Int) -> Int =\n  if n <= 0 then 0 else b(x, n - 1)\n\nfn b[U](y: U, n: Int) -> Int =\n  if n <= 0 then 0 else c(some(y), n - 1)\n\nfn c[V](z: V, n: Int) -> Int =\n  if n <= 0 then 0 else 1 + a(z, n - 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(a(1, 5)))\n}\n",
        "at a growing type: `c`'s `V` is `Option[U]`",
    ),
];

const ACCEPTED: &[(&str, &str, &str)] = &[
    (
        "same-type",
        "fn depth[T: Hash](xs: List[T], n: Int) -> Int =\n  if n <= 0 then list.len(xs) else depth(xs + xs, n - 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(depth([1], 3)))\n}\n",
        "8\n",
    ),
    (
        "swap",
        "fn alt[A, B](a: A, b: B, n: Int) -> Int =\n  if n <= 0 then 0 else 1 + alt(b, a, n - 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(alt(1, \"x\", 5)))\n}\n",
        "5\n",
    ),
    (
        "mutual-same-type",
        "fn ping[T](x: T, n: Int) -> Int =\n  if n <= 0 then 0 else 1 + pong(x, n - 1)\n\nfn pong[U](y: U, n: Int) -> Int =\n  if n <= 0 then 0 else 1 + ping(y, n - 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(ping(\"a\", 6)))\n}\n",
        "6\n",
    ),
    (
        "generic-walk",
        "fn count[T](xs: List[T], i: Int) -> Int =\n  if i >= list.len(xs) then 0 else 1 + count(xs, i + 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(count([\"a\", \"b\", \"c\"], 0)))\n}\n",
        "3\n",
    ),
    (
        "acyclic-growth",
        "fn single[T](x: T) -> List[T] = [x]\n\nfn pair_len[T](x: T, n: Int) -> Int =\n  if n <= 0 then list.len(single((x, x))) else pair_len(x, n - 1)\n\neffect fn main() -> Unit = {\n  println(int.to_string(pair_len(1, 2)))\n}\n",
        "1\n",
    ),
];

#[test]
fn growing_recursion_is_e114_at_check() {
    if !tool_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    for (name, src, msg) in REFUSED {
        for route in [&["check"][..], &["check", "--target", "wasm"][..]] {
            let (ok, out, err) = run(name, src, route);
            let all = format!("{out}{err}");
            assert!(!ok, "{name} ({route:?}): check accepted a growing recursion:\n{all}");
            assert!(all.contains("E114"), "{name} ({route:?}): expected E114:\n{all}");
            assert!(all.contains(msg), "{name} ({route:?}): expected `{msg}`:\n{all}");
            assert!(!all.contains("E082"), "{name} ({route:?}): the wasm wall surfaced:\n{all}");
        }
    }
}

#[test]
fn bounded_recursion_runs_alike_on_both_legs() {
    if !tool_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let wasm = wasmtime_available();
    for (name, src, want) in ACCEPTED {
        let (ok, out, err) = run(name, src, &["check"]);
        assert!(ok && !format!("{out}{err}").contains("E114"), "{name}: check refused:\n{out}{err}");
        let (ok, native, err) = run(name, src, &["run"]);
        assert!(ok, "{name}: native run failed:\n{err}");
        assert_eq!(native, *want, "{name}: native output");
        if wasm {
            let (ok, w, err) = run(name, src, &["run", "--target", "wasm"]);
            assert!(ok, "{name}: wasm run failed:\n{err}");
            assert_eq!(w, native, "{name}: wasm output differs from native");
        }
    }
}
