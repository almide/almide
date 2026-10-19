// ── eval_test.rs, part 6: a user fn spelled like a stdlib impl fn ──
//
// include!-spliced into `tests/eval_test.rs`. #3825: a program fn whose name
// equals a self-hosted stdlib IMPL name (`list_len` is how the registry spells
// `list.len`) ran as that stdlib fn — the named-call tier recognised the
// spelling and took the stdlib route, dropping the user's body. A user fn and
// a stdlib impl fn share one identifier space only by spelling; which one a
// call means is decided by which definition the call resolved to. Covered:
// registry rows (`list_len`, `map_len`, `string_len`, `int_to_string`), a HOF
// row (`list_map`), and an `@implements`-declared body
// (`process_stdin_lines`), each called by name AND alongside the stdlib
// surface it shadows by spelling. The expected lines are native's and wasm's
// (both measured: `almide run` with and without `--target wasm`).

const IMPL_NAME_COLLISION_SRC: &str = r#"fn list_len(xs: List[String], i: Int) -> Int = list.len(xs) + i
fn map_len(m: Map[String, Int], i: Int) -> Int = map.len(m) * 100 + i
fn string_len(s: String) -> Int = string.len(s) + 1000
fn int_to_string(n: Int) -> String = "user:" + int.to_string(n)
fn list_map(xs: List[Int], k: Int) -> List[Int] = list.map(xs, (x) => x * k)
fn process_stdin_lines() -> String = "user stdin"

fn main() -> Unit = {
  println(int.to_string(list_len(["a", "b"], 10)))
  println(int.to_string(map_len(["a": 1, "b": 2], 7)))
  println(int.to_string(string_len("abc")))
  println(int_to_string(5))
  println("${list_map([1, 2], 3)}")
  println(process_stdin_lines())
  println(int.to_string(list.len([1, 2, 3])))
  println(int.to_string(string.len("hello")))
  println(int.to_string(map.len(["x": 1])))
  println("${list.map([1, 2], (x) => x + 1)}")
}
"#;

const IMPL_NAME_COLLISION_OUT: &str =
    "12\n207\n1003\nuser:5\n[3, 6]\nuser stdin\n3\n5\n1\n[2, 3]\n";

#[test]
fn a_user_fn_spelled_like_a_stdlib_impl_runs_the_users_body() {
    expect_out(IMPL_NAME_COLLISION_SRC, IMPL_NAME_COLLISION_OUT);
}

// The other direction: a self-hosted body's OWN bare call of a sibling impl
// (`zlib_deflate_level` calls `zlib_deflate(data)!`) means the pool's
// sibling, not the program's fn of that spelling. The expected lines are
// native's (`almide run`).
#[test]
fn a_self_hosted_body_calls_its_own_sibling_not_the_users_namesake() {
    expect_out(
        r#"import zlib

fn zlib_deflate(n: Int) -> Int = n + 1

effect fn main() -> Unit = {
  let packed = zlib.deflate_level(bytes.from_list([1, 2, 3]), 6)!
  let back = zlib.inflate(packed)!
  println("${bytes.to_list(back)}")
  println(int.to_string(zlib_deflate(1)))
}
"#,
        "[1, 2, 3]\n2\n",
    );
}
