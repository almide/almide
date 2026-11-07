mod harness;
use harness::{RunResult, run_wasm};
fn run(src: &str) -> RunResult {
    let ir = almide_spine::s5::lower_to_ir("string_case_reuse.almd", src).expect("front");
    let bytes = {
        let _count = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("emit")
    };
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "{}", r.stderr);
    r
}
#[test]
fn fresh_ascii_upper_reuses_the_input_without_leaking() {
    let src = r#"fn main() -> Unit = {
      var sum = 0
      for i in 0..<1000 {
        let out = string.to_upper("abc${i}xy")
        sum = sum + string.len(out)
      }
      println("${sum}")
    }"#;
    let a = run(src);
    let b = run(&src.replace("string.to_upper(\"abc${i}xy\")", "\"abc${i}xy\""));
    assert_eq!(a.stdout, b.stdout);
    let ac = a.alloc_count.unwrap();
    let bc = b.alloc_count.unwrap();
    assert_eq!(
        ac.allocs, bc.allocs,
        "ASCII upper must not allocate a second string"
    );
    assert_eq!(ac.allocs, ac.frees);
}
#[test]
fn aliases_static_strings_and_unicode_prefixes_keep_value_semantics() {
    let r = run(r#"fn upper(s: String) -> String = string.to_upper(s)
    fn main() -> Unit = {
      let original = "abc${7}"
      let alias = original
      println("${upper(original)} ${original} ${alias}")
      println(upper("abc"))
      println("abc")
      println(upper("abc${9}xy"))
      println(upper("abc${7}éßΣ😀"))
      println(upper("${7}a\u{301}é"))
      println("[${upper(string.trim("  "))}]")
    }"#);
    assert_eq!(
        r.stdout,
        "ABC7 abc7 abc7\nABC\nabc\nABC9XY\nABC7ÉßΣ😀\n7A\u{301}É\n[]\n"
    );
    let c = r.alloc_count.unwrap();
    assert_eq!(c.allocs, c.frees);
}

#[test]
fn pooled_literals_are_immutable_across_calls_and_case_operations() {
    let r = run(r#"fn upper(s: String) -> String = string.to_upper(s)
    fn literal() -> String = "hello"
    fn main() -> Unit = {
      println(upper(literal()))
      println(literal())
      println(string.to_lower(upper(literal())))
      println(string.reverse(literal()))
      println(string.capitalize(literal()))
      println(literal())
    }"#);
    // The oracle lives outside the guest's deduplicated literal pool.
    assert_eq!(r.stdout, "HELLO\nhello\nhello\nolleh\nHello\nhello\n");
    let c = r.alloc_count.unwrap();
    assert_eq!(c.allocs, c.frees);
}
