//! `list.fold(xs, f, (acc, a) => App(acc, a))`: a fold whose callback body is
//! one constructor of the accumulator's own variant type. A constructor is
//! spelled as a named call, and the fold's direct-call route emitted a call
//! to a function of that name, which does not exist ("call to `App` — not a
//! lowered user fn"). The route declines a constructor and the general fold
//! body builds it.
use std::process::Command;

/// A positional constructor over the accumulator and the element, and over
/// the accumulator and a string, folded from a seed, over an empty list, and in a loop.
const SHAPE: &str = r##"type Raw = | RVar(String) | RApp(Raw, Raw) | RField(Raw, String)

fn apply(f: Raw, args: List[Raw]) -> Raw = list.fold(args, f, (acc, a) => RApp(acc, a))

fn fields(f: Raw, path: String) -> Raw = list.fold(string.split(path, "/"), f, (acc, g) => RField(acc, g))

fn show(r: Raw) -> String = match r {
  RVar(s) => s,
  RApp(f, a) => "${show(f)}(${show(a)})",
  RField(f, g) => "${show(f)}.${g}",
}

fn main() -> Unit = {
  println(show(apply(RVar("f"), [RVar("a"), RVar("b")])))
  println(show(apply(RVar("f"), [])))
  println(show(fields(RVar("f"), "x/y")))
  var n = 0
  for i in 0..<20000 {
    n = n + string.len(show(apply(RVar("g"), [RVar("a"), RVar(int.to_string(i))])))
  }
  println(int.to_string(n))
}
"##;

const SHAPE_STDOUT: &str = "f(a)(b)\nf\nf.x.y\n208890\n";

/// Callbacks that never took the route's constructor arm: a call to a user
/// function, and a block that binds the constructed value first.
const CONTROL: &str = r##"type Raw = | RVar(String) | RApp(Raw, Raw)

fn app(x: Raw, y: Raw) -> Raw = RApp(x, y)

fn by_fn(f: Raw, args: List[Raw]) -> Raw = list.fold(args, f, (acc, a) => app(acc, a))

fn by_block(f: Raw, args: List[Raw]) -> Raw = list.fold(args, f, (acc, a) => {
  let r = RApp(acc, a)
  r
})

fn show(r: Raw) -> String = match r {
  RVar(s) => s,
  RApp(f, a) => "${show(f)}(${show(a)})",
}

fn main() -> Unit = {
  println(show(by_fn(RVar("f"), [RVar("a"), RVar("b")])))
  println(show(by_block(RVar("f"), [RVar("a"), RVar("b")])))
}
"##;

const CONTROL_STDOUT: &str = "f(a)(b)\nf(a)(b)\n";

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
}

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
    render(src).unwrap_or_else(|e| panic!("{name} walls: {e}"));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{name} {extra:?}");
    }
}

#[test]
fn a_fold_whose_callback_is_one_constructor_builds_the_variant() {
    runs_unwalled("fold_ctor_callback", SHAPE, SHAPE_STDOUT);
}

#[test]
fn function_and_block_bodied_fold_callbacks_run_as_before() {
    runs_unwalled("fold_ctor_callback_control", CONTROL, CONTROL_STDOUT);
}
