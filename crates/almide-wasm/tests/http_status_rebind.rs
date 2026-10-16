//! `x = http.status(x, code)` is O(1) per step on the wasm leg.
//!
//! The wasm response is a flat `List[String]` (status, body, then name/value
//! pairs). `http_status` rebuilt it as `[st] + list.drop(resp, 1)`: a fresh
//! list of every header on each call, so a loop that only sets the status
//! copied all n headers per step. The stdlib inspection measured that rebind
//! at O(n) per step on wasm against O(1) on native (in place).
//!
//! The steps must request a small share of what building the response did.

mod harness;
use harness::run_wasm;

fn bytes_requested(src: &str, want: &str) -> u64 {
    let ir = almide_spine::s5::lower_to_ir("http_status_rebind.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    assert_eq!(r.stdout, want);
    assert_eq!(c.allocs, c.frees, "leaked {} block(s)", c.allocs.abs_diff(c.frees));
    c.bytes
}

fn program(headers: u32, steps: u32) -> String {
    format!(
        "import http

effect fn main() -> Unit = {{
  var x = http.with_headers(200, \"b\", list.range(0, {headers}) |> list.fold(map.new(), (m, i) => map.set(m, \"k${{i}}\", \"v\")))
  var i = 0
  while i < {steps} {{
    x = http.status(x, 200 + i % 2)
    i = i + 1
  }}
  println(\"${{http.status_code(x)}} ${{map.len(http.headers(x))}}\")
}}
"
    )
}

#[test]
fn setting_the_status_in_a_loop_does_not_copy_the_headers() {
    let headers = 2000;
    let steps = 200;
    let code = if steps % 2 == 0 { 201 } else { 200 };
    let want = format!("{code} {headers}\n");
    let base = bytes_requested(&program(headers, 0), &format!("200 {headers}\n"));
    let looped = bytes_requested(&program(headers, steps), &want);
    let per_step = looped.saturating_sub(base) / u64::from(steps);
    // One header pair is two 4-byte slots plus their strings; a copy of the
    // list is thousands of bytes. Allow a fresh status string and a little.
    assert!(per_step < 256, "{per_step} bytes per status step over {headers} headers ({base} to build, {looped} with {steps} steps) — the step copies the headers");
}
