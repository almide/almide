//! Typed tag constructors and distinct enum payloads preserve every execution route.
use std::process::Command;

// Use the corpus oracle's pre-codegen cut point, including its bundled
// declaration handling. Structural-wasm linking is a different cut point.
#[allow(dead_code)]
mod interp_oracle {
    include!("wasm_runtime_test_parts/interp_leg.rs");
    pub(super) fn lower(source: &str) -> Result<almide_ir::IrProgram, String> {
        lower_for_interp(source)
    }
}

#[test]
fn tagged_values_preserve_all_execution_routes() {
    let cases = [
        ("computed_callee_precedes_argument_snapshots", r#"
fn choose() -> (Int, Int) -> Int = {
 println("choose")
 (a, b) => a * 10 + b
}
fn main() -> Unit = {
 var n = 2
 let r = choose()(n, { n = 7; println("arg"); 1 })
 println("${r} ${n}")
 var f = (a: Int, b: Int) => a + b
 let x = f(1, { f = (a: Int, b: Int) => a * b; 2 })
 println("${x} ${f(2, 4)}")
}
"#, "choose\narg\n21 7\n3 8\n"),

        ("mut_call_nested_read_before", include_str!("../spec/wasm_cross/mut_call_nested_read_before.almd"), "c 3 bump 31 c 30\ntrue false false 3 1.5 1.5\ns x x! x!\nt 1 2 t! 2\nxs [1, 2] 3 [1, 2, 9]\nx!/x!!/x!!\n[\"x!!/x!!!/x!!!\"]\nn x!!!/x!!!!/x!!!! 31 301 300\nnoisy 1\nshow x!!!!\nnoisy 2\nshow x!!!!!\no 1 x!!!! 2 x!!!!! x!!!!!\nnoisy 3\nshow x!!!!!\n3/x!!!!!/x!!!!!!\nh 0 1 1\n"),

        ("hash_hmac_rfc4231", include_str!("../spec/wasm_cross/hash_hmac_rfc4231.almd"), "tc1 sha256 b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7\ntc1 sha512 87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854\ntc2 sha256 5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843\ntc2 sha512 164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737\ntc3 sha256 773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe\ntc3 sha512 fa73b0089d56a284efb0f0756c890be9b1b5dbdd8ee81a3655f83e33b2279d39bf3e848279a722c806b485a47e67c807b946a337bee8942674278859e13292fb\ntc4 sha256 82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b\ntc4 sha512 b0ba465637458c6990e5a8c5f61d4af7e576d97ff94b872de76f8050361ee3dba91ca5c11aa25eb4d679275cc5788063a5f19741120c4f2de2adebeb10a298dd\ntc5 sha256 a3b6167473100ee06e0c796c2955552b\ntc5 sha512 415fad6271580a531d4179bc891d87a6\ntc6 sha256 60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54\ntc6 sha512 80b24263c7c1a3ebb71493c1dd7be8b49b46d1f41b4aeec1121b013783f8f3526b56d037e05f2598bd0fd2215d6a1e5295e64f73f63f0aec8b915a985d786598\ntc7 sha256 9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2\ntc7 sha512 e37b6a775dc87dbaa4dfa9f96e5e3ffddebd71f8867289865df5a32d20cdc944b6022cac3c4982b10d5eeb55c3e4de15134676fb6de0446065c97440fa8c6a58\nvalid256 raw true\nvalid256 hex true\nvalid256 HEX true\nvalid256 wrong-data false\nvalid256 wrong-key false\nvalid256 truncated false\nvalid256 odd-hex false\nvalid256 non-hex false\nvalid256 empty false\nvalid512 raw true\nvalid512 hex true\nvalid512 HEX true\nvalid512 wrong-data false\nvalid512 sha256-tag false\nvalid512 non-hex false\nempty-key b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad\ntag renders Secret(***) [Secret(***)]\ntag == true false true\n"),
        ("closure_rich_list_capture", include_str!("../spec/wasm_cross/closure_rich_list_capture.almd"), "found:b\nmissing\ndone:y\nnot found: zz\nhits=6000 db=2 sdb=2\n"),
        ("guard_value_exit_in_loop", include_str!("../spec/wasm_cross/guard_value_exit_in_loop.almd"), "pure_unit_for 1\npure_unit_for 2\npure_unit_for 5\npure_unit_for done\n600 55\neffect_unit_for 1\neffect_unit_for 7\neffect_unit_for done\neffect_unit_while 1\neffect_unit_while 2\neffect_unit_while 1\neffect_unit_while 2\neffect_unit_while done\n6 -2\n600 55\nab!\nxy\n7 1002\n"),
        ("pipe_chain_scalar_bind", include_str!("../spec/wasm_cross/pipe_chain_scalar_bind.almd"), "30\n"),
        ("arg_rebinds_earlier_arg", include_str!("../spec/wasm_cross/arg_rebinds_earlier_arg.almd"), "contains true 1\nindex_of some(2)\ntake [\"a\", \"b\"]\ndrop [\"b\", \"c\"]\nchunk [[\"a\", \"b\"], [\"c\"]]\nzip [(\"a\", \"x\"), (\"b\", \"y\")]\njoin a-b\nget some(\"b\")\nindex b\nset.contains true 1\nset.union 3\nmap.get some(\"1\") 1\nmap.contains true\nmap.set 2\nstring.contains true z\nstring.replace -bc-bc\nstring.pad_start ..ab\nstr concat ab!\nuser scalar 21 7\nint.max 3\nuser first 3 1\nuser mid 33\nuser mid after rebind 23\nuser str a+b+c\nclosure 3\nlist lit [[\"a\", \"b\"], [\"q\"]]\nrecord [\"a\", \"b\"] 1\ninterp hi/!\n"),
        ("nested_guard_stops_following_effects", r#"
var ticks = 0
fn keep(i: Int) -> Bool = { ticks = ticks + 1; i < 5 }
fn nested() -> Int = {
 var n = 0
 for x in [1, 2, 3] {
  if x == 2 then { guard false else n + 100; n = n + 1000 } else { n = n + x }
  println("step ${x}")
 }
 -1
}
fn checked_while() -> Int = {
 var i = 0
 while keep(i) {
  i = i + 1
  if i == 2 then { guard false else i + 200 } else ()
  println("while ${i}")
 }
 -1
}
fn matched() -> Int = {
 for x in [1, 2, 3] {
  match x { 2 => { guard false else 77 }, _ => () }
  println("match ${x}")
 }
 -1
}
fn two_loops() -> Int = {
 for x in [1, 2] { guard x > 0 else 10 }
 for x in [3, 4] { guard x < 4 else 20 }
 -1
}
fn main() -> Unit = {
 println("nested=${nested()}")
 println("checked=${checked_while()}")
 println("ticks=${ticks}")
 println("matched=${matched()}")
 println("two=${two_loops()}")
}
"#, "step 1\nnested=101\nwhile 1\nchecked=202\nticks=2\nmatch 1\nmatched=77\ntwo=20\n"),

        ("fan_arm_heap_capture", include_str!("../spec/wasm_cross/fan_arm_heap_capture.almd"), "13\n7\n9\n15\n13\n17\n9\n3\n11\n10\n"),
        ("fan_arm_unwrap_or", include_str!("../spec/wasm_cross/fan_arm_unwrap_or.almd"), "sums=3,3,40\ndefaults=7,2\nlens=0,0\n"),
        ("int_pow_operator_negative_base", include_str!("../spec/wasm_cross/int_pow_operator_negative_base.almd"), "9\n9\n-27\n9\n1\n0\n0\n"),
        ("sized_int_overflow_wrap", include_str!("../spec/wasm_cross/sized_int_overflow_wrap.almd"), "-128\n127\n0\n44\n255\n-32768\n0\n-2147483648\n0\n7\n7\n-128\n44\n-32768\n-2147483648\n0\n0\n-733379959\n125\n-40\n144\n24464\n4611686018427387904\n"),
        ("float_equality_signed_zero_nan", r#"
fn verdict(b: Bool) -> String = if b then "eq" else "ne"
fn main() -> Unit = {
 let z = -0.0
 let nan = 0.0 / 0.0
 println(verdict(list.contains([z], 0.0)))
 println(verdict(list.contains([nan], nan)))
 println(int.to_string(list.len(list.unique([z, 0.0, nan, nan]))))
 println(int.to_string(list.len(list.dedup([z, 0.0, nan, nan]))))
 let a: List[(Float, Float)] = [(z, 1.0)]
 let b: List[(Float, Float)] = [(0.0, 1.0)]
 let c: List[(Float, Float)] = [(nan, 1.0)]
 println(verdict(a == b))
 println(verdict(c == c))
}
"#, "eq\nne\n3\n3\neq\nne\n"),
        ("tm_list_tuple_eq", include_str!("../spec/wasm_cross/tm_list_tuple_eq.almd"), "eq\nne\n"),
        ("list_float_equality_family", include_str!("../spec/wasm_cross/list_float_equality_family.almd"), "unique 1\ndedup 1\ncontains t\nindex_of 0\nnan_unique 3\nnan_contains f\nnan_dedup 3\nfuzz_cell 3\n"),
        ("deep_eq_nested_distinct", include_str!("../spec/wasm_cross/deep_eq_nested_distinct.almd"), "eq\nne\neq\neq\nne\n"),
        ("list_of_tuples_equality", include_str!("../spec/wasm_cross/list_of_tuples_equality.almd"), "eq\n"),
        ("ctor_fn_value", include_str!("../spec/wasm_cross/ctor_fn_value.almd"), "some-maps\nok-maps\nerr-maps\nbound-applies\nuser-maps\nmulti-applies\nzero-value\n"),
        ("match_catchall_binder_eq", include_str!("../spec/wasm_cross/match_catchall_binder_eq.almd"), "10\n10\n01\n10\n10\n010\n10\n10\n10\n01\n10\n"),
        ("tm_list_variant_eq", include_str!("../spec/wasm_cross/tm_list_variant_eq.almd"), "eq\nne\n"),
        ("tm_tree_eq", include_str!("../spec/wasm_cross/tm_tree_eq.almd"), "eq\nne\n"),
        ("map_entry_loop", include_str!("../spec/wasm_cross/map_entry_loop.almd"), "(\"x\", 10)\n(\"y\", 20)\n(\"a\", 5)\n[(\"k1\", \"v1\"), (\"b\", \"c\")] 35 [\"k1\": \"v1\", \"b\": \"c\"] [\"x\": 10, \"y\": 20, \"a\": 5]\n"),
        ("leak_free_map_walk_exit", include_str!("../spec/wasm_cross/leak_free_map_walk_exit.almd"), "err big 3 err big 4 1\n241\n"),
        ("for_in_list_value_semantics", include_str!("../spec/wasm_cross/for_in_list_value_semantics.almd"), "A 6 100\nB 3 6\nC 10 0\nD 123 3\n"),
        ("map_set_in_place", include_str!("../spec/wasm_cross/map_set_in_place.almd"), "A.len=2000 sum=138884781\nA.k0=2000 k999=2999 k1999=1999\nA.first=0 last=1999\nB.c=0:72,1:72,2:72,3:71,4:71,5:71,6:71,\nB.r.len=64 first=63 sum=411847536\nC.a=2000 b=2001 m=2001\nC.a.has5000=no a.has5001=no b.has5001=no\nD.d=1:11,2:20,3:30,\nD.held=1:10,2:20, opt=1:10,2:20, pair=1:10,2:20,\nE=1:1,2:2,3:3,11:100,12:200,13:300,\nF=1:111,2:200, a=2 b=1\nG.keys=k0,k1,k2,k3 k2=98\nG.w.len=5 w2=w2\nH.walked=2660 len=40 sum=987739124\n"),
        ("map_entries_reclaim", include_str!("../spec/wasm_cross/map_entries_reclaim.almd"), "174010\n"),
        ("list_with_capacity", include_str!("../spec/wasm_cross/list_with_capacity.almd"), "a=[]\nhuge=[]\nb=[]\nc=[]\na2=[1, 2, 3]\nb2=[\"x\"]\nhuge2=[\"y\"]\n"),
        ("float32_arithmetic_rounds", include_str!("../spec/wasm_cross/float32_arithmetic_rounds.almd"), "1/3: 0.3333333432674408\n0.1+0.2: 0.30000001192092896\n1.1*1.1: 1.2100000381469727\n1-1e-8: 1.0\n2^24+1: 16777216.0\n10%3.3: 0.10000014305114746\n-(1/3): -0.3333333432674408\n(1/3)**1.7: 0.15448768436908722\n3**0.5: 1.7320507764816284\n0.1f32: 0.10000000149011612\nint 2^24+1: 16777216.0\nsum 10x0.1: 1.0000001192092896\n2^24+1 == 2^24: true\n1+1e-8 > 1: false\n0.1+0.2 == 0.3: true\n1/3*3 < 1: false\n"),
        ("self_call_swapped_params", include_str!("../spec/wasm_cross/self_call_swapped_params.almd"), "swap(f(x), y, f(y, x, y~x))\ny/fy\nz~y\na;b;c();a;b,c,a\nw(q)\n"),
        ("tco_bytes_value_arg", include_str!("../spec/wasm_cross/tco_bytes_value_arg.almd"), "nested: 2 1,2\ndeep: 5 1,2,3,4,5\ncall: 4 4,3,2,1\nblock: 3 6,4,2\nif: 2 4,2\nmatch: 6 36,5,5,33,2,2\nboth: 10 9,2,1,9,0,9,0,9,2,0\nlong: 3 32,31,30\n"),
        ("closure_borrowed_arg_guard", include_str!("../spec/wasm_cross/closure_borrowed_arg_guard.almd"), "filter: ok(abc) v=ok(abc)\nreject: err(fallback) w=ok(-0)\noption: true o=opt\nlist: true xs=[\"elem\"]\nfield: true b=field\nparam: true p=ok(param)\nutf8: ok(日本語)\n"),
        ("captured_var_value_stored", include_str!("../spec/wasm_cross/captured_var_value_stored.almd"), "(1,1) (2,1) (3,1)\n(1,1) (3,1) (6,1) (10,1)\n(5,5)#5 (11,6)#6 (18,7)#7\n(3,1) (3,1) (99,99)\n"),
        ("closure_captured_map_key_write", include_str!("../spec/wasm_cross/closure_captured_map_key_write.almd"), "5 23 20 9\n2 9\n"),
        ("closure_captured_record_field_mutation", include_str!("../spec/wasm_cross/closure_captured_record_field_mutation.almd"), "1 [1] ab 1 [0, 7] 1\n2 [1, 10, 20, 30, 99, 50]\n3 [1, 10, 20, 30, 99, 50, 5] [8, 8]\n4 [1, 10, 20, 30, 99, 50, 5, 1, 1] 2 abab [3, 4, 5] [8, 8, 1, 2, 3]\n"),
        ("closure_mut_arg_in_nested_match", include_str!("../spec/wasm_cross/closure_mut_arg_in_nested_match.almd"), "24\n"),
        ("nested_scalar_record_append", include_str!("../spec/wasm_cross/nested_scalar_record_append.almd"), "3\n3\n2\n4\n"),
        ("filter_map_conditional_arm", include_str!("../spec/wasm_cross/filter_map_conditional_arm.almd"), "ab=ab\nxyz=long\n"),
        ("nested_place_assign", include_str!("../spec/wasm_cross/nested_place_assign.almd"), "local xs=[] m=[\"a\": 8, \"b\": 2] leaf=deep/7\nlocal xs=[1, 2, 3] m=[\"a\": 1] leaf=t/0\ninner alias xs=[1, 2, 3] leaf=t\nindexed xs=[1, 20, 21] m=[\"a\": 1] leaf=t/0\nparam xs=[0, 2, 3] m=[:] leaf=reset/0\ng xs=[4, 5] m=[\"k\": 2] leaf=g/1\n"),
        ("mut_param_global_callee_sees_precall", include_str!("../spec/wasm_cross/mut_param_global_callee_sees_precall.almd"), "bare: gl=[0, 1, 2, 3, 4] copy=6\nfield: g.xs=[0, 1, 2, 3, 4] copy=6\nnested: g.inner.ys=[0, 1, 2, 3, 4] copy=6\nhelper: len(gl)=6 copy=7\nclosure: peek=7 copy=8\nafter: [0, 1, 2, 3, 4, 7, 7, 7] [0, 1, 2, 3, 4, 7] [0, 1, 2, 3, 4, 7]\n"),
        ("mut_param_cell_tail_capture_write", include_str!("../spec/wasm_cross/mut_param_cell_tail_capture_write.almd"), "mut_call n=5 xs=1,2\nby_value 1002 n=10 xs=1,2\nmatch_arms 20 n=15 xs=1,2\nif_arms 61 n=37 xs=1,2\nnested n=9 xs=1,2\nloop_tail n=13 xs=1,2\nearly 117 n=110 xs=1,2\n"),
        ("generic_record_anon_field", include_str!("../spec/wasm_cross/generic_record_anon_field.almd"), "42\nhi/w\n41 t s\n{ tag: \"t\", x: 41 }\n"),
        ("generic_record_fn_field", include_str!("../spec/wasm_cross/generic_record_fn_field.almd"), "a=hi\nb!\nc=yo\nbad 7\n"),
        ("nested_hof_lambda_inference", include_str!("../spec/wasm_cross/nested_hof_lambda_inference.almd"), "0.25,0.25\n5.0,25.0\n"),
        ("result_uoe_float", include_str!("../spec/wasm_cross/result_uoe_float.almd"), "r = 0.000001\nf = 100\n"),
        ("result_generic_param_arg", include_str!("../spec/wasm_cross/result_generic_param_arg.almd"), "id=ok 7\nid-err=err neg -1\nfold=ok 5\nmap.get ??=err invalid digit found in string\nget_or=ok 9\nid-effect=err neg -2\nfold-effect=ok 4\n"),
        ("global_record_rebuild", r#"
type C = { n: Int, tag: String }
var g: C = C { n: 0, tag: "start" }
fn bump() -> Unit = { g = C { n: g.n + 1, tag: g.tag + "!" } }
fn main() -> Unit = {
 let old = g
 bump()
 bump()
 println("${g.n} ${g.tag}")
 println("${old.n} ${old.tag}")
}
"#, "2 start!!\n0 start\n"),
        ("effect_function_slots", include_str!("../spec/wasm_cross/effect_fn_value_slot_family.almd"),
         "t:in x\nin y\nin 41 | L:in 41 | n=42\nL:in z\nsvc R:in w\nR:in v\nin a\nA:in b\nc!\nstrict failed\n[IN M]\n[P:IN Q]\n[n=8]\nchain failed\n"),
        ("typed_errors", include_str!("../spec/wasm_cross/match_err_arm_typed_payload.almd"), "ok 7\nbad x\nok 3\nneg -2\nok 4\nmissing\n"),
        ("sized_nested_tuples", include_str!("../spec/wasm_cross/sized_numeric_literals.almd"), "1:100\n127:300\n4\n127:9999\n127:1000:2000\n"),
        ("distinct_records", r#"
type Shape = | Circle({ r: Int }) | Square({ s: Int }) | Rect({ w: Int, h: Int })
fn area(sh: Shape) -> Int = match sh {
 Circle(c) => c.r * c.r * 3,
 Square(c) => c.s * c.s,
 Rect(c) => c.w * c.h,
}
fn main() -> Unit = {
 for sh in [Circle({ r: 2 }), Square({ s: 5 }), Rect({ w: 3, h: 4 })] {
  println("${area(sh)}")
  println("${area(sh)}")
 }
}
"#, "12\n12\n25\n25\n12\n12\n"),
        ("mixed_heap_payloads", r#"
type Item = | Text(String) | Numbers(List[Int]) | Row({ n: Int })
fn show(x: Item) -> String = match x {
 Text(s) => s,
 Numbers(ns) => "${list.len(ns)}",
 Row(r) => "${r.n}",
}
fn main() -> Unit = {
 let xs = [Text("日"), Numbers([1, 2]), Row({ n: 7 }), Text(""), Numbers([])]
 for x in xs { println(show(x)); println(show(x)) }
 println("${list.len(xs)}")
}
"#, "日\n日\n2\n2\n7\n7\n\n\n0\n0\n5\n"),
        ("find_builder", r#"
type Row = { key: String, value: String }
fn found(xs: List[Row], key: String) -> String = {
 let result = list.find(xs, (row) => row.key == key)
 match result { some(row) => row.value, none => "missing" }
}
fn main() -> Unit = {
 let xs = [Row { key: "a", value: "first" }, Row { key: "a", value: "second" }, Row { key: "日", value: "value" }]
 println(found(xs, "a"))
 println(found(xs, "日"))
 println(found(xs, "z"))
 println(found([], "a"))
 println(found(xs, "a"))
 println("${list.len(xs)}")
}
"#, "first\nvalue\nmissing\nmissing\nfirst\n3\n"),
        ("nested_empty_options", r#"
fn wrap(x: String?) -> Result[String?, String] = ok(x)
fn show(x: Result[String?, String]) -> String = match x {
 ok(value) => value ?? "none",
 err(e) => e,
}
fn main() -> Unit = {
 let absent: String? = none
 println(show(wrap(absent)))
 println(show(wrap(some(""))))
 println(show(wrap(some("日"))))
 println(show(err("error")))
 println(show(wrap(absent)))
}
"#, "none\n\n日\nerror\nnone\n"),
        ("global_snapshots", include_str!("../spec/wasm_cross/mutable_global_repeat_writes.almd"), "7\n99\n10\n20\n11\n22\n"),
        ("global_cow_publication", r#"
var buf = bytes.zeros(4)
var xs: List[Int] = [1, 2, 3]
effect fn main() -> Unit = {
 bytes.put_u8(buf, 0, 7)
 let old = buf
 bytes.copy_from(buf, old, 1, 0, 1)
 bytes.copy_from(buf, buf, 2, 1, 1)
 let before = xs
 println("${list.pop(xs) ?? -1}")
 println("${list.len(xs)}:${list.len(before)}")
 println("${bytes.to_list(buf)}")
 println("${bytes.to_list(old)}")
}
"#, "3\n2:3\n[7, 7, 7, 0]\n[7, 0, 0, 0]\n"),
    ];
    let dir = std::env::temp_dir().join(format!("almide-native-tagged-values-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    for (name, source, expected) in cases {
        let result = std::panic::catch_unwind(|| {
            check_routes(&dir, name, source, expected, "", 0);
        });
        if result.is_err() { failures.push(name); }
    }
    assert!(failures.is_empty(), "tagged value cases failed: {failures:?}; artifacts {}", dir.display());
    std::fs::remove_dir_all(dir).unwrap();
}

fn check_routes(dir: &std::path::Path, name: &str, source: &str,
    expected_stdout: &str, expected_stderr: &str, expected_exit: i32) {
    let actual = check_compiled_routes(dir, name, source, expected_stdout, expected_stderr, expected_exit);
    let ir = interp_oracle::lower(source).unwrap();
    let reference = almide_interp::Interpreter::new(&ir).run_main();
    assert!(matches!((&reference.status, expected_exit),
        (almide_interp::RunStatus::Ok, 0) | (almide_interp::RunStatus::Aborted, 1))
        || matches!(&reference.status, almide_interp::RunStatus::Exited(code) if *code == expected_exit),
        "{name}: unexpected interpreter status {:?}", reference.status);
    assert_eq!((&actual.stdout[..], &actual.stderr[..], actual.status.code()),
        (reference.stdout.as_bytes(), reference.stderr.as_bytes(), Some(reference.exit_code())), "{name}: interp");
}

fn check_compiled_routes(dir: &std::path::Path, name: &str, source: &str,
    expected_stdout: &str, expected_stderr: &str, expected_exit: i32) -> std::process::Output {
    let input = dir.join(format!("{name}.almd"));
    let rs = dir.join(format!("{name}.rs"));
    let binary = dir.join(name);
    std::fs::write(&input, source).unwrap();
    let (rust, certs) = almide_mir::pipeline::try_render_rust_source_certified(source)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    for prop in ["ownership", "native-calls", "bounds"] {
        assert!(certs.iter().any(|(p, _, _)| p == prop), "{name}: missing {prop}");
    }
    for (property, function, witness) in &certs {
        let property = almide_verify::Property::from_name(property).expect("known certificate property");
        assert!(almide_verify::check(property, witness.as_bytes()), "{name}: rejected {property:?} for {function}");
    }
    if matches!(name, "fan_arm_heap_capture" | "fan_arm_unwrap_or") {
        assert!(rust.contains("let __almide_fan_h0 = almide_fan_spawn"), "{name}: expected threaded arms");
    }
    std::fs::write(&rs, rust).unwrap();
    let compile = Command::new("rustc").args(["--edition=2021", "-C", "opt-level=1"])
        .arg(&rs).arg("-o").arg(&binary).output().unwrap();
    assert!(compile.status.success(), "{name}: {}", String::from_utf8_lossy(&compile.stderr));
    let actual = Command::new(&binary).output().unwrap();
    assert_eq!(actual.status.code(), Some(expected_exit), "{name}: native execution");
    assert_eq!(actual.stdout, expected_stdout.as_bytes(), "{name}: expected bytes");
    assert_eq!(actual.stderr, expected_stderr.as_bytes(), "{name}: stderr");
    let oracle = dir.join(format!("{name}-v0"));
    let built = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", "--no-verified"]).arg(&input).arg("-o").arg(&oracle)
        .env("ALMIDE_NO_VERIFIED_OK", "1").output().unwrap();
    assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
    let v0 = Command::new(oracle).output().unwrap();
    assert_eq!((&actual.stdout, &actual.stderr, actual.status.code()),
        (&v0.stdout, &v0.stderr, v0.status.code()), "{name}: v0");
    let wasm = almide::wasm_route::route_wasm(input.to_str().unwrap(), source,
        almide::wasm_route::ModuleSource::Disk { dep_paths: &[] }, None,
        Default::default(), &mut |_| {}).unwrap_or_else(|e| panic!("{name}: wasm route {e:?}"));
    let wasm = almide_wasm_run::run_wasm(&wasm.bytes).unwrap();
    assert_eq!((&actual.stdout, &actual.stderr, actual.status.code()),
        (&wasm.stdout_bytes, &wasm.stderr_bytes, Some(wasm.exit)), "{name}: wasm");
    actual
}

#[test]
fn narrow_power_preserves_negative_exponent_abort() {
    let dir = std::env::temp_dir().join(format!("almide-native-narrow-power-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (ty, module) in [("Int8", "int8"), ("Int16", "int16"), ("Int32", "int32")] {
        let source = format!("fn power(n: {ty}) -> {ty} = {{ let base: {ty} = 2; base ^ n }}\nfn main() -> Unit = {{ println({module}.to_string(power(-1))) }}");
        check_routes(&dir, module, &source, "", "Error: negative exponent\n", 1);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unit_aggregates_preserve_mutation_and_execution_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-unit-values-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("unit_aggregate_values", r#"type Mark = { token: Unit, name: String }
fn units(n: Int) -> List[Unit] = list.range(0, n) |> list.map((_) => ())
fn tuple() -> (Unit, String) = ((), "kept")
fn main() -> Unit = {
 let xs = units(4)
 println("${list.len(xs)} ${list.len(list.unique(xs))} ${list.len(list.dedup(xs))}")
 let p = tuple()
 println(p.1)
 let row = Mark { token: (), name: "record" }
 println(row.name)
 println("${list.len([(), (), ()])}")
}
"#, "4 1 1\nkept\nrecord\n3\n"),
        ("mut_param_effect_unpropagated", include_str!("../spec/wasm_cross/mut_param_effect_unpropagated.almd"), "bang: 2 3\nbang list: [7, 8]\ndiscarded: 3 16\nbound: ok 200 20\nmatched: 250 25\nfallback: 310 6\nloop: 34 [1, 2, 3, 4, 5, 6]\n"),
        ("mut_param_pure_result_fn_propagates", include_str!("../spec/wasm_cross/mut_param_pure_result_fn_propagates.almd"), "ok 2 xs=[1, 0]\nerr bad 2 xs=[1, 0, 2]\n?? -1 xs=[1, 0, 2, 3]\nrejected: type mismatch: x vs y\nseen=<y><x>\naccepted y\nseen=<y><x><y><y>\nna some_\n"),
    ];
    for (name, source, expected) in cases { check_routes(&dir, name, source, expected, "", 0); }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn collection_equality_preserves_order_and_index_boundaries() {
    let dir = std::env::temp_dir().join(format!("almide-native-collection-eq-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("set_from_list_every_equatable_element", include_str!("../spec/wasm_cross/set_from_list_every_equatable_element.almd"), "set.from_list of list: len=3 has=true has_not=false same=true\nset.from_list of set: len=3 has=true same=true\nset.from_list of bytes: len=3 has=true has_not=false same=true\n"),
        ("equality_ops_every_equatable_element", include_str!("../spec/wasm_cross/equality_ops_every_equatable_element.almd"), "groups=[\"x: a,c\", \"x,y: b,d\", \": e\"]\nset: unique=2 dedup=2 contains=true index_of=some(2)\nmap: unique=2 dedup=2 contains=true index_of=some(2)\nresult: unique=[ok(1), err(\"e1\"), err(\"e2\")] dedup=[ok(1), err(\"e1\"), ok(1), err(\"e1\"), err(\"e2\")] contains=true index_of=some(4)\nbytes: unique=2 dedup=2 contains=true index_of=some(2)\nunit: unique=1 dedup=1\nunique_by bytes key=[\"ab\", \"b\"]\nset of list: len=2 has=true has_not=false\nset of set: len=2 has=true\nset of bytes: len=2 has=true\nlist key: len=2 get=some(\"y\") miss=none\noption key: len=2 get=some(3) none=some(2)\nresult key: len=2 get=some(\"c\") has=true\n"),
        ("map_insertion_order", include_str!("../spec/wasm_cross/map_insertion_order.almd"), "literal: [zebra=1, apple=2, mango=3, delta=4]\nset-new: [zebra=1, apple=2, mango=3, delta=4, echo=5]\nset-update: [zebra=1, apple=20, mango=3, delta=4, echo=5]\ngrow-len: 30\ngrow-keys: [k0,k1,k2,k3,k4,k5,k6,k7,k8,k9,k10,k11,k12,k13,k14,k15,k16,k17,k18,k19,k20,k21,k22,k23,k24,k25,k26,k27,k28,k29]\ninsert-keys: [i0,i1,i2,i3,i4,i5,i6,i7,i8,i9,i10,i11,i12,i13,i14,i15,i16,i17,i18,i19]\nremove: [zebra=1, apple=2, delta=4, echo=5]\nmerge: [x=1, y=20, z=3, w=40]\nfrom_list: [p=9, q=2, r=3]\nfor-in: zebra/apple/mango/delta/\nfilter-keys: [k0,k2,k4,k6,k8,k10,k12,k14,k16,k18,k20,k22,k24,k26,k28]\nmap: [zebra=100, apple=200, mango=300, delta=400]\nfind: apple=2\nupdate: [zebra=1, apple=1002, mango=3, delta=4]\neq: true\nneq: false\ngroup-keys: [even,odd]\ngroup-even: [10,32,54]\n"),
        ("map_set_eq", include_str!("../spec/wasm_cross/map_set_eq.almd"), "T\nF\nT\nF\nT\nF\nT\nF\n"),
        ("map_set_index_threshold", include_str!("../spec/wasm_cross/map_set_index_threshold.almd"), "A.len=401 hits=401 misses=399 sum=489291502\nA.k0=401 k400=466 k401=-1 first5=0,37,74,111,148\nB.snap=40 b=120 snap.has39=y snap.has40=n b.has119=y b.119=14161\nC.b=120 c=119 b.50=2500 c.50=-1 c.51=2601 c.sum=469349908\nD.eq=y ne=n d1.first=0,1,2,3 d2.first=3,2,1,0\nE.len=150 hits=150 key0=150 key149=149 key150=-1 kex=-1 first3=key0,key1,key2\nF.len=23 w0=22 w22=21 total=500\nG.g1=30 hits=30 u=40 i=15 d=15 s=25 sub=y disj=y\nG.eq=y first5=0,7,14,21,28\nH.len=25 s24=y s25=n first3=s0,s1,s2\nI.len=101 u250=1 u50=1050 u99=198 merged=101 m7=8 m250=1 sum=540517482\n"),
    ];
    for (name, source, expected) in cases { check_routes(&dir, name, source, expected, "", 0); }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn structural_collection_keys_preserve_declared_types_and_order() {
    let dir = std::env::temp_dir().join(format!("almide-native-structural-keys-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("binary_search_duplicate_keys", include_str!("../spec/wasm_cross/binary_search_duplicate_keys.almd"), "dup5  = some(4)\ndup4  = some(3)\nmid   = some(3)\none   = some(0)\nnone  = none\nempty = none\nfirst = some(0)\nlast  = some(2)\n"),
        ("compound_eq", include_str!("../spec/wasm_cross/compound_eq.almd"), "T\nT\nF\nT\nF\nS0\nS1\nN\nS1\n3\n2\n2\n2\nT\nF\n2\n3\n2\nSz\nSb\nN\nT\n2\nS99\nN\n2\nT\n2\nT\n2\n"),
        ("compound_repr_records_interp", include_str!("../spec/wasm_cross/compound_repr_records_interp.almd"), "point=Point { x: 1, y: 2 }\nperson=Person { name: \"Ada \\\"A\\\"\", age: 36, score: 9.5 }\nperson_int=Person { name: \"Bob\", age: 40, score: 10 }\nline=Line { start: Point { x: 1, y: 2 }, end: Point { x: 3, y: 4 } }\npoints=[Point { x: 1, y: 2 }, Point { x: 5, y: 6 }]\nopt_rec=some(Point { x: 7, y: 8 })\nrec_map=[\"o\": Point { x: 0, y: 0 }]\nclick=Click(10, 20)\nscroll=Scroll { dy: 5 }\nquit=Quit\ncircle=Circle(2)\ncircle_f=Circle(2.5)\nshapes=[Circle(1), Rect(2, 3), Label { text: \"box\", at: Point { x: 0, y: 0 } }]\nshape_map=[\"a\": Circle(3), \"b\": Rect(4, 5)]\nzero=0\nneg_zero=-0\none=1\nbig=1000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\nfrac=0.25\nfloat_list=[0, 1, 0.5]\nfloat_tuple=(2, 3.5)\nempty_list=[]\nempty_map=[:]\nempty_set=set.from_list([])\nempty_nested=[]\n"),
        ("sort_by_compound_key", include_str!("../spec/wasm_cross/sort_by_compound_key.almd"), "count fig 3\ncount apple 2\ncount pear 2\ncount date 1\nii 1 10\nii 1 30\nii 2 5\nii 2 10\nsi apple 2\nsi date 1\nsi fig 3\nsi pear 2\nbi 1 10\nbi 2 10\nbi 2 5\nbi 1 30\nlk date 1\nlk pear 2\nlk apple 2\nlk fig 3\nok date 1\nok fig 3\nok pear 2\nok apple 2\nsc 4\nsc 2\nsc 3\nsc 1\nst z\nst y\nst x\nst w\nempty=0\none solo 7\nrounds=1320\n"),
        ("structural_key_value_boundaries", r#"type Key = { payload: Result[Int, String], name: String }
fn key(n: Int) -> Key = { payload: ok(n), name: "k" }
fn make_map() -> Map[Key, Int] = map.set(map.new(), key(1), 4)
fn main() -> Unit = {
 var m = make_map()
 m = map.set(m, { name: "k", payload: ok(1) }, 9)
 println("${map.len(m)} ${map.get(m, key(1)) ?? -1}")
 let s: Set[Key] = set.from_list([key(1), key(1), key(2)])
 let more = set.insert(s, { name: "k", payload: err("e") })
 println("${set.len(s)} ${set.len(more)} ${set.contains(s, { payload: ok(2), name: "k" })}")
 println("${set.contains(more, { name: "k", payload: err("e") })}")
}
"#, "1 9\n2 3 true\ntrue\n"),
    ];
    for (name, source, expected) in cases { check_routes(&dir, name, source, expected, "", 0); }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cached_sort_keys_run_once_per_element_including_singletons() {
    let dir = std::env::temp_dir().join(format!("almide-native-cached-keys-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = r#"type Row = { key: Float, label: String }
effect fn main() -> Unit = {
 var calls = 0
 let rows = [Row { key: -0.0, label: "minus" }, Row { key: 2.0, label: "two-a" }, Row { key: 0.0, label: "plus" }, Row { key: -3.0, label: "negative" }, Row { key: 2.0, label: "two-b" }]
 let sorted = list.sort_by(rows, (r) => { calls = calls + 1; r.key })
 println("${calls}")
 println(list.map(sorted, (r) => r.label) |> list.join(","))
 println(list.map(rows, (r) => r.label) |> list.join(","))
 let empty: List[Row] = []
 let zero = list.sort_by(empty, (r) => { calls = calls + 1; r.key })
 println("${calls} ${list.len(zero)}")
 let single = list.sort_by([Row { key: 1.0, label: "one" }], (r) => { calls = calls + 1; r.key })
 println("${calls} ${list.len(single)}")
 var int_calls = 0
 let ints = list.sort_by([9], (n) => { int_calls = int_calls + 1; n })
 var str_calls = 0
 let strings = list.sort_by(["x"], (s) => { str_calls = str_calls + 1; s })
 println("${int_calls} ${str_calls} ${list.len(ints)} ${list.len(strings)}")
}
"#;
    check_routes(&dir, "float_sort_cached_keys", source,
        "5\nnegative,minus,plus,two-a,two-b\nminus,two-a,plus,negative,two-b\n5 0\n6 1\n1 1 1 1\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn source_order_preserves_recursive_variants_and_float_total_order() {
    let dir = std::env::temp_dir().join(format!("almide-native-source-order-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("ord_record_variant", include_str!("../spec/wasm_cross/ord_record_variant.almd"), "rec 1 a\nrec 1 y\nrec 2 x\nnest a 9 a\nnest b 1 a\nnest b 1 z\nunit Low\nunit Low\nunit Mid\nunit High\nshape Dot\nshape Line { len: 1 }\nshape Line { len: 5 }\nshape Box { w: 1, h: 2 }\nshape Box { w: 1, h: 9 }\npair Zero\npair One(2)\npair One(9)\npair Two(1, \"a\")\npair Two(1, \"z\")\nmin_rec=some(Inner { n: 1, s: \"a\" })\nmax_rec=some(Inner { n: 2, s: \"x\" })\nmin_shape=some(Dot)\nmax_shape=some(Box { w: 1, h: 9 })\nmin_pair=some(Zero)\nmax_pair=some(Two(1, \"z\"))\nmin_unit=some(Low)\nrkey x 2\nrkey z 2\nrkey y 1\nvkey y 1\nvkey x 2\nvkey z 2\nwide -9223372036854775808 lo\nwide 0 z\nwide 9223372036854775807 hi\nstable x\nstable y\nstable z\nempty=0\nempty_min=none\none=1\n"),
        ("ord_recursive", include_str!("../spec/wasm_cross/ord_recursive.almd"), "tree 0(9())\ntree 1()\ntree 1(2())\ntree 1(2(5()))\ntree 1(9())\ndeep 0(1(2()))\ndeep 0(1(2(7())))\ndeep 0(1(3()))\nexpr Lit { n: 1 }\nexpr Lit { n: 5 }\nexpr Add { l: [Lit { n: 1 }], r: [] }\nexpr Add { l: [Lit { n: 1 }], r: [Lit { n: 0 }] }\nexpr Add { l: [Lit { n: 2 }], r: [] }\nnode a<>\nnode a<[]/3>\nnode a<[z]/1>\nnode b<>\nleaf []/9\nleaf [a]/0\nleaf [m]/0\nmin_tree=0(9())\nmax_tree=1(9())\nmin_expr=some(Lit { n: 1 })\nmax_expr=some(Add { l: [Lit { n: 2 }], r: [] })\nkey x 2\nkey z 2\nkey y 1\nempty=0\nempty_min=-\none=1\n"),
        ("list_float_total_order", include_str!("../spec/wasm_cross/list_float_total_order.almd"), "0/0 const: sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\n0/0 runtime: sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\ninf-inf: sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\n0*inf: sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\ninf/inf: sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\nsqrt(-1): sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\nfloat.nan(): sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\n-(0/0): sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\n-(0/0) const: sort=-1.0,1.0,NaN min=1.0 max=NaN sort_by=neg,one,nan\nsort: 0.0,1.2,1.2,2.8,3.5\nmin:  0.0\nmax:  3.5\nsort zeros: -0.0,0.0\nsort neg: -3.0,-1.5,-0.5,2.0\nmin neg:  -3.0\nmax neg:  2.0\nsort inf: -inf,-2.0,0.5,1.0,inf\nmax inf:  inf\nmin inf:  -inf\nsort_by: d,b,c,a\n"),
        ("sort_by_call_count", include_str!("../spec/wasm_cross/sort_by_call_count.almd"), "[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]\ncalls=10\n[1, 2, 3]\n[3, 2, 1]\n"),
    ];
    for (name, source, expected) in cases { check_routes(&dir, name, source, expected, "", 0); }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn collection_order_and_fallible_search_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-fallible-search-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("fallible_hof_matrix_list", include_str!("../spec/wasm_cross/fallible_hof_matrix_list.almd"), "  visit 1\n  visit 3\nany ok true\n  visit 1\n  visit -2\nany err neg -2\n  visit 1\nall ok false\n  visit 1\nall ok false\n  visit 1\n  visit 3\n  visit 5\ncount ok 2\n  visit 1\n  visit -2\ncount err neg -2\n  visit 1\n  visit 3\nfind_index ok some 1\n  visit 1\n  visit -2\nfind_index err neg -2\n  pick 1\n  pick 3\nfind_map ok some 30\n  pick 1\n  pick -2\nfind_map err bad pick -2\n  pick 1\n  pick 2\nfind_map ok none\n  visit 1\n  visit 3\n  visit 5\npartition ok [3,5][1]\n  visit 1\n  visit -2\npartition err neg -2\n  visit 5\n  visit 4\n  visit 1\ntake_while ok [5,4]\n  visit 5\n  visit -4\ntake_while err neg -4\n  visit 5\n  visit 4\n  visit 1\ndrop_while ok [1,-1]\n  visit 5\n  visit -4\ndrop_while err neg -4\n  add 3\n  add 5\nreduce ok some 9\n  add -2\nreduce err bad add -2\nreduce ok none\n  add 1\n  add 3\n  add 5\nscan ok [11,14,19]\n  add 1\n  add -2\nscan err bad add -2\n  key 5\n  key 1\n  key 4\n  key 3\nsort_by ok [3,1,4,5]\n  key 5\n  key -1\nsort_by err bad key -1\n  key 5\n  key 1\n  key 4\n  key 3\ngroup_by ok 2:[5] 1:[1,4] 0:[3]\n  key 5\n  key -1\ngroup_by err bad key -1\n  key 5\n  key 1\n  key 4\n  key 3\n  key 2\nunique_by ok [5,1,3]\n  key 5\n  key -1\nunique_by err bad key -1\n  add 10\n  add 20\nzip_with ok [11,22]\n  add 10\n  add -20\nzip_with err bad add -20\n  add 100\nupdate ok [1,103,5]\nupdate ok [1,3,5]\n  add -1\nupdate err bad add -1\n  add 1\n  add 2\n  add 4\n  add 8\niterate ok [1,2,4,8]\n  add -1\niterate err bad add -1\n  order 5 1\n  order 4 3\n  order 1 3\n  order 5 3\n  order 5 4\nsort_with ok [1,3,4,5]\n  order -1 4\nsort_with err bad order -1 4\n  key 5\n  key 1\n  key 4\n  key 3\nmin_by ok some 3\n  key 5\n  key -1\nmin_by err bad key -1\n  key 5\n  key 1\n  key 4\n  key 3\nmax_by ok some 5\n  key 5\n  key -1\nmax_by err bad key -1\n  order 1 5\n  order 4 1\n  order 3 1\nmin_with ok some 1\n  order -1 5\nmin_with err bad order -1 5\n  order 1 5\n  order 4 5\n  order 3 5\nmax_with ok some 5\n  order -1 5\nmax_with err bad order -1 5\n  ident 5\n  ident 7\n  ident 5\nbinary_search_by ok some 2\n  ident -5\nbinary_search_by err bad ident -5\n  order 5 5\n  order 7 5\n  order 5 5\nbinary_search_with ok some 2\n  order -5 5\nbinary_search_with err bad order -5 5\ntotal all true\n"),
        ("list_unique_by_str_key", include_str!("../spec/wasm_cross/list_unique_by_str_key.almd"), "a = [true]\nb = [1, 2]\nc = [1, 2, 3]\n"),
        ("list_total_order", include_str!("../spec/wasm_cross/list_total_order.almd"), "i_min=some(1)\ni_max=some(8)\ni_minx=some(-9223372036854775808)\ni_maxx=some(9223372036854775807)\nb_min=some(false)\nb_max=some(true)\ns_min=some(\"apple\")\ns_max=some(\"zebra\")\nt_min=some((1, \"a\"))\nt_max=some((2, \"b\"))\ntb_max=some((true, true))\nti_max=some((256, true))\nl_min=some([0, 9])\nl_max=some([1, 2, 3])\nls_min=some([\"a\"])\no_min=some(none)\no_max=some(some(5))\ne_min=none\ns_int=[1, 2, 2, 5, 8]\ns_str=[\"apple\", \"banana\", \"cherry\"]\ns_bool=[false, false, true, true]\ns_tup=[(1, \"a\"), (1, \"z\"), (2, \"b\")]\ns_lint=[[0, 9], [1], [1, 2], [1, 2, 3]]\ns_opt=[none, none, some(2), some(5)]\nsb_bool=[3, 1, 2, 4]\nsb_str=[1, 20, 3]\nsb_len=[\"a\", \"cc\", \"bbb\"]\nsb_bool2=[false, false, true, true, true]\nu_bool=[1]\nu_parity=[1, 2]\nu_modk=[10, 21, 32, 43]\nu_id=[3, 1, 2]\n"),
    ];
    for (name, source, expected) in cases { check_routes(&dir, name, source, expected, "", 0); }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fallible_tuple_values_preserve_reuse_and_short_circuiting() {
    let dir = std::env::temp_dir().join(format!("almide-native-result-pairs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("fallible_hof_matrix_containers", include_str!("../spec/wasm_cross/fallible_hof_matrix_containers.almd"), "  dbl 1\n  dbl 3\n  dbl 5\nmap.map ok a=2,b=6,c=10\n  dbl 1\n  dbl -2\nmap.map err bad dbl -2\n  visit a\n  visit b\n  visit c\nmap.filter ok b=3,c=5\n  visit a\n  visit b\nmap.filter err neg b\n  acc a\n  acc b\n  acc c\nmap.fold ok 9\n  acc a\n  acc b\nmap.fold err bad acc b\n  visit a\n  visit b\nmap.any ok true\n  visit a\n  visit b\nmap.any err neg b\n  visit a\nmap.all ok false\n  visit x\n  visit y\nmap.all err neg y\n  visit a\n  visit b\n  visit c\nmap.count ok 2\n  visit a\n  visit b\nmap.count err neg b\n  visit a\n  visit b\nmap.find ok b=3\n  visit a\n  visit b\nmap.find err neg b\n  dbl 3\nmap.update ok a=1,b=6,c=5\nmap.update ok a=1,b=3,c=5\n  dbl -2\nmap.update err bad dbl -2\n  dbl 5\nmap.upsert ok a=1,b=3,c=10\nmap.upsert ok a=1,b=3,c=5,z=9\n  dbl -4\nmap.upsert err bad dbl -4\n  dbl 1\n  dbl 3\n  dbl 5\nset.map ok 2,6,10\n  dbl 1\n  dbl -2\nset.map err bad dbl -2\n  visit 1\n  visit 3\n  visit 5\nset.filter ok 3,5\n  visit 1\n  visit -2\nset.filter err neg -2\n  add 1\n  add 3\n  add 5\nset.fold ok 9\n  add 1\n  add -2\nset.fold err bad add -2\n  visit 1\n  visit 3\nset.any ok true\n  visit 1\n  visit -2\nset.any err neg -2\n  visit 1\nset.all ok false\n  visit 4\n  visit -1\nset.all err neg -1\n  visit 1\n  visit 3\n  visit 5\nset.count ok 2\n  visit 1\n  visit -2\nset.count err neg -2\n  visit 1\n  visit 3\nset.find ok some 3\n  visit 1\n  visit -2\nset.find err neg -2\n  add 10\n  add 20\nset.zip_with ok 11,23\n  add 1\n  add -2\nset.zip_with err bad add -2\n  pick 1\n  pick 3\n  pick 5\nset.filter_map ok 30,50\n  pick 1\n  pick -2\nset.filter_map err bad pick -2\n  pick 1\n  pick 3\nset.find_map ok some 30\n  pick 1\n  pick -2\nset.find_map err bad pick -2\n  add 3\n  add 5\nset.reduce ok some 9\n  add -2\nset.reduce err bad add -2\n  add 1\n  add 3\n  add 5\nset.scan ok 11,14,19\n  add 1\n  add -2\nset.scan err bad add -2\n  visit 1\n  visit 3\n  visit 5\nset.partition ok 3,5|1\n  visit 1\n  visit -2\nset.partition err neg -2\n  key 1\n  key 3\n  key 5\nset.group_by ok 3\n  key 1\n  key -2\nset.group_by err bad key -2\n  key 1\n  key 3\n  key 5\nset.sort_by ok 3,1,5\n  key 1\n  key -2\nset.sort_by err bad key -2\n  ord 5 3\n  ord 5 1\n  ord 3 1\nset.sort_with ok 5,3,1\n  ord -2 1\nset.sort_with err bad ord -2 1\n  key 1\n  key 3\n  key 5\nset.min_by ok some 3\n  key 1\n  key -2\nset.min_by err bad key -2\n  key 1\n  key 3\n  key 5\nset.max_by ok some 5\n  key 1\n  key -2\nset.max_by err bad key -2\n  ord 3 1\n  ord 5 1\nset.min_with ok some 1\n  ord -2 1\nset.min_with err bad ord -2 1\n  ord 3 1\n  ord 5 3\nset.max_with ok some 5\n  ord -2 1\nset.max_with err bad ord -2 1\n  pzip a\n  pzip b\nmap.zip_with ok 11,23\n  pzip a\n  pzip b\nmap.zip_with err bad pzip b\n  entry a\n  entry b\n  entry c\nmap.filter_map ok 30,50\n  entry a\n  entry b\nmap.filter_map err bad entry b\n  entry a\n  entry b\nmap.find_map ok some 30\n  entry a\n  entry b\nmap.find_map err bad entry b\n  pjoin b\n  pjoin c\nmap.reduce ok abc=9\n  pjoin b\nmap.reduce err bad pjoin b\n  pacc a\n  pacc b\n  pacc c\nmap.scan ok 1,4,9\n  pacc a\n  pacc b\nmap.scan err bad pacc b\n  entry a\n  entry b\n  entry c\nmap.partition ok b=3,c=5|a=1\n  entry a\n  entry b\nmap.partition err bad entry b\n  pkey a\n  pkey b\n  pkey c\nmap.group_by ok 3\n  pkey a\n  pkey b\nmap.group_by err bad pkey b\n  pkey a\n  pkey b\n  pkey c\nmap.sort_by ok b=3,a=1,c=5\n  pkey a\n  pkey b\nmap.sort_by err bad pkey b\n  pord c b\n  pord c a\n  pord b a\nmap.sort_with ok c=5,b=3,a=1\n  pord b a\nmap.sort_with err bad pord b a\n  pkey a\n  pkey b\n  pkey c\nmap.min_by ok b=3\n  pkey a\n  pkey b\nmap.min_by err bad pkey b\n  pkey a\n  pkey b\n  pkey c\nmap.max_by ok c=5\n  pkey a\n  pkey b\nmap.max_by err bad pkey b\n  pord b a\n  pord c a\nmap.min_with ok a=1\n  pord b a\nmap.min_with err bad pord b a\n  pord b a\n  pord c b\nmap.max_with ok c=5\n  pord b a\nmap.max_with err bad pord b a\n  dbl 4\noption.map ok some 8\n  dbl -4\noption.map err bad dbl -4\noption.map ok none\noption.flat_map ok some 2\noption.flat_map err odd half\n  visit 4\noption.filter ok some 4\n  visit -4\noption.filter err neg -4\noption.unwrap_or_else ok 7\noption.unwrap_or_else ok 1\noption.unwrap_or_else err no fallback\noption.or_else ok some 7\noption.or_else ok some 2\n"),
        ("fallible_tuple_fold", r#"effect fn step(a: (String, Int), e: (String, Int)) -> (String, Int) = {
 println(e.0)
 if e.1 < 0 then err("bad " + e.0)! else (a.0 + e.0, a.1 + e.1)
}
effect fn main() -> Unit = {
 let xs = [("a", 1), ("b", 2), ("c", 3)]
 let got: Result[(String, Int), String] = list.fold(xs, ("", 0), (a, e) => step(a, e)!)
 println("${got}")
 let bad: Result[(String, Int), String] = list.fold([("a", 1), ("b", -2), ("c", 3)], ("", 0), (a, e) => step(a, e)!)
 println("${bad}")
 println("${xs}")
}
"#, "a\nb\nc\nok((\"abc\", 6))\na\nb\nerr(\"bad b\")\n[(\"a\", 1), (\"b\", 2), (\"c\", 3)]\n"),
        ("result_pair_values", r#"fn s(x: (String, Int)) -> Result[(String, Int), String] = ok(x)
fn v(x: (Value, Int)) -> Result[(Value, Int), String] = ok(x)
fn ss(x: (List[String], Int)) -> Result[(List[String], Int), String] = ok(x)
fn vs(x: (List[Value], Int)) -> Result[(List[Value], Int), String] = ok(x)
fn main() -> Unit = {
 let a = ("abc", 1)
 let b = (value.string("日"), 2)
 let c = (["a", "b"], 3)
 let d = ([value.string("v"), value.int(4)], 4)
 println("${s(a)}")
 println("${v(b)}")
 println("${ss(c)}")
 println("${vs(d)}")
 println("${a} ${b} ${c} ${d}")
}
"#, "ok((\"abc\", 1))\nok((\"日\", 2))\nok(([\"a\", \"b\"], 3))\nok(([\"v\", 4], 4))\n(\"abc\", 1) (\"日\", 2) ([\"a\", \"b\"], 3) ([\"v\", 4], 4)\n"),
    ];
    for (name, source, expected) in cases { check_routes(&dir, name, source, expected, "", 0); }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn typed_display_and_tag_comparisons_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-display-tags-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "bytes_display", include_str!("../spec/wasm_cross/bytes_display.almd"), r#"[[97, 98]] [[]] [[0, 127, 128, 255]]
some([97, 98]) none
ok([97, 98])
[[97, 98], [0, 127, 128, 255]] ([97, 98], 1)
Frame { tag: "x", payload: [0, 127, 128, 255] } Data([97, 98])
"#, r#"Error: assertion failed
  at: line 24
  expected: ok([0, 127, 128, 255])
  found: ok([97, 98])
"#, 1);
    check_routes(&dir, "uint64_assert_display", include_str!("../spec/wasm_cross/uint64_assert_display.almd"), r#"before 18446744073709551615
"#, r#"Error: assertion failed
  at: line 11
  expected: (9223372036854775808, [9223372036854775808], some(9223372036854775808))
  found: (18446744073709551615, [18446744073709551615], some(18446744073709551615))
"#, 1);
    check_routes(&dir, "float32_display_shortest", include_str!("../spec/wasm_cross/float32_display_shortest.almd"), r#"0.1: 0.1 | 0.1
1/3: 0.33333334 | 0.33333334
2: 2 | 2.0
0: 0 | 0.0
-0: -0 | -0.0
inf: inf | inf
-inf: -inf | -inf
nan: NaN | NaN
max: 340282350000000000000000000000000000000 | 340282350000000000000000000000000000000.0
min normal: 0.000000000000000000000000000000000000011754944 | 0.000000000000000000000000000000000000011754944
min subnormal: 0.000000000000000000000000000000000000000000001 | 0.000000000000000000000000000000000000000000001
subnormal: 0.000000000000000000000000000000000000000104488 | 0.000000000000000000000000000000000000000104488
2^-12 tie: 0.00024414063 | 0.00024414063
2^20+0.25 tie: 1048576.3 | 1048576.3
1e10: 10000000000 | 10000000000.0
123.456: 123.456 | 123.456
[0.1, 2, 0.33333334]
P { x: 0.1, n: 1 }
some(0.1)
(0.1, 2)
Dot(0.1) Pair(0.33333334, 2)
"#, r#""#, 0);
    check_routes(&dir, "declaration_literal_domain", include_str!("../spec/wasm_cross/declaration_literal_domain.almd"), r#"top -128 255 -32768 65535 -2147483648 4294967295
top64 18446744073709551615 9223372036854775808
aggregate [127, -128] 65535 1
var 254 [1, 255]
assign -32768 [2.5] [200: -128] 0 32767
field 1 127 255 -32768 4294967295 18446744073709551615
variant p -128 9223372036854775808 empty
param -128 65535 18446744073709551615
"#, r#""#, 0);
    check_routes(&dir, "fallible_hof_option_effect_callback", include_str!("../spec/wasm_cross/fallible_hof_option_effect_callback.almd"), r#"filter_map_int=[0, 3, 6]
filter_map_str=["n0", "n2", "n3"]
filter_map_rec=["p0", "p2", "p4"]
filter_map_list=[[0, 1], [2, 3]]
map_int=[some(0), none, none, some(3)]
map_str=["n0", "-", "n2"]
map_list=[2, 0, 2]
capped_ok=[0, 3]
capped_err=too big 9
filter_inner=[0, 3]
fold_inner=9
bind=ok some(3)
arg=ok some(6)
arg_none=ok none
match=ok some(9)
tail=ok some(3)
tail_none=ok none
tail_scalar=ok 42
"#, r#""#, 0);
    check_routes(&dir, "stdlib_type_eq", include_str!("../spec/wasm_cross/stdlib_type_eq.almd"), r#"bare: true true false
result: true true false
option: true false
list: true true
alias: true false
url: true false true false
literal: true true true false
"#, r#""#, 0);
    check_routes(&dir, "sized_display_values", r#"fn main() -> Unit = {
 let low: Int8 = -128
 let high: Int8 = 127
 let xs = [low, high, low]
 let s = set.from_list(xs)
 println("${xs} ${s}")
 println("${set.contains(s, low)} ${set.to_list(set.remove(s, low))} ${set.to_list(s)}")
 let big: UInt64 = 18446744073709551615
 let near: UInt64 = 9223372036854775808
 let us = set.from_list([big, near, big])
 println("${us} ${set.to_list(us)}")
 let f: Float32 = float32.from(0.1)
 println("${[f, float32.from(1.25)]}")
 let a: Result[Bytes, String] = ok(bytes.from_list([0, 255]))
 let b: Result[Bytes, String] = ok(bytes.from_list([0, 255]))
 let c: Result[Bytes, String] = ok(bytes.from_list([0, 254]))
 let e: Result[Bytes, String] = err("bad")
 let e2: Result[Bytes, String] = err("other")
 println("${a == b} ${a == c} ${a == e} ${e == a} ${e == e} ${e == e2}")
 println("${a} ${e}")
}
"#, r#"[-128, 127, -128] set.from_list([-128, 127])
true [127] [-128, 127]
set.from_list([18446744073709551615, 9223372036854775808]) [18446744073709551615, 9223372036854775808]
[0.1, 1.25]
true false false false true false
ok([0, 255]) err("bad")
"#, r#""#, 0);
    check_routes(&dir, "optional_chain_heap_field", include_str!("../spec/wasm_cross/optional_chain_heap_field.almd"), r#"yes
no
no
yes
yes
3
2
0
yes
no
yes
2
yes
"#, "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn nested_sequence_patterns_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-patterns-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "nested_tuple_pattern", include_str!("../spec/wasm_cross/nested_tuple_pattern.almd"), "13\n", "", 0);
    check_routes(&dir, "list_pattern_variant_element", include_str!("../spec/wasm_cross/list_pattern_variant_element.almd"), "7\n-1\n-1\n-1\n2\n103\n-1\n", "", 0);
    check_routes(&dir, "list_pattern_in_option_payload", include_str!("../spec/wasm_cross/list_pattern_in_option_payload.almd"), "7\n5\n0\n0\n7\n0\n15\n0\n9\n0\n-1\n0\n", "", 0);
    check_routes(&dir, "nested_pattern_order", r#"type Item = | Plain(Int) | Named(String)
var events = ""
fn subject(xs: List[Item]) -> (List[Item], Int) = {
 events = events + "s"
 (xs, 1)
}
fn accept(n: Int) -> Bool = {
 events = events + "g"
 n > 5
}
fn fallback() -> String = { events = events + "f"; "fallback" }
fn choose(xs: List[Item]) -> String = match subject(xs) {
 ([Plain(n)], 1) if accept(n) => "hit ${n}",
 ([Named(s)], _) => "name ${s}",
 _ => fallback(),
}
fn main() -> Unit = {
 println(choose([Plain(7)]))
 println(choose([Plain(2)]))
 println(choose([Named("x")]))
 println(choose([]))
 println(choose([Plain(9), Plain(10)]))
 println(events)
 let xs = [Named("kept")]
 let out = match xs { [Named(s)] => [s, s], _ => ["bad"] }
 println("${out} ${xs}")
}
"#, r#"hit 7
fallback
name x
fallback
fallback
sgsgfssfsf
["kept", "kept"] [Named("kept")]
"#, "", 0);
}

#[test]
fn matrix_runtime_preserves_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-matrix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "matrix_composition_family", include_str!("../spec/wasm_cross/matrix_composition_family.almd"), "fma: (2, 3) [[-1, 4, 3], [8, 7, 12]]\nfma_zip: (2, 2) [[11, 22], [34, 45]]\nfma3: (2, 3) [[-0.5, 5, 4.5], [10, 9.5, 15]]\nmul_scaled: (2, 2) [[6.75, 7.875], [15, 18.75]]\nappend: (3, 3) [[1, 2, 3], [4, 5, 6], [7, 8, 9]]\nappend_base0: (2, 3) [[1, 2, 3], [4, 5, 6]]\nappend_extra0: (2, 3) [[1, 2, 3], [4, 5, 6]]\nf32 ctors: (2, 3) (3, 2) 0\nf32 mul: 3 12 (2, 2) 1.5\nov_1x1_nxn: (1, 1) [[6]]\nov_nxn_1x1: (1, 1) [[6.5]]\nov_wide_tall: (1, 1) [[-9]]\nov_tall_big: (3, 1) [[7], [18], [29]]\nov_empty: (0, 0) []\nov_big_empty: (2, 0) [[], []]\nov_alias: (3, 3) [[0, 0, 0], [0, 0, 0], [0, 0, 0]]\nov3_1x1: (1, 1) [[3.5]]\nov3_mixed: (1, 1) [[-2]]\nov3_empty: (0, 0) []\nround: (1, 1) [[0]]\nround3: (1, 1) [[0]]\n", "", 0);
    check_routes(&dir, "matrix_mul_no_fma", include_str!("../spec/wasm_cross/matrix_mul_no_fma.almd"), "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n", "", 0);
    check_routes(&dir, "matrix_neg_signed_zero", include_str!("../spec/wasm_cross/matrix_neg_signed_zero.almd"), "neg=[[-0, 0, -1, 1]]\nneg_recip=-inf,inf,-1.0,1.0\nneg_neg=[[0, -0, 1, -1]]\nscale_recip=-inf,inf\nmap_recip=-inf,inf\nsub_recip=inf\ntranspose_recip=-inf\n", "", 0);
    check_routes(&dir, "matrix_producers_uniform_rows", include_str!("../spec/wasm_cross/matrix_producers_uniform_rows.almd"), "zeros: r=2 c=3 [[0, 0, 0], [0, 0, 0]] t=[[0, 0], [0, 0], [0, 0]]\nones_neg_rows: r=0 c=0 [] t=[]\nzeros_neg_cols: r=2 c=0 [[], []] t=[]\nadd_zip: r=2 c=2 [[2, 4], [7, 9]] t=[[2, 7], [4, 9]]\nsub_zip: r=2 c=2 [[0, 0], [1, 1]] t=[[0, 1], [0, 1]]\ndiv_zip: r=2 c=2 [[1, 1], [1.3333333333333333, 1.25]] t=[[1, 1.3333333333333333], [1, 1.25]]\nfma_zip: r=2 c=2 [[5, 10], [17, 22]] t=[[5, 17], [10, 22]]\nneg: r=2 c=3 [[-1, -2, -3], [-4, -5, -6]] t=[[-1, -4], [-2, -5], [-3, -6]]\nscale: r=2 c=3 [[0.5, 1, 1.5], [2, 2.5, 3]] t=[[0.5, 2], [1, 2.5], [1.5, 3]]\nmap: r=2 c=3 [[10, 20, 30], [40, 50, 60]] t=[[10, 40], [20, 50], [30, 60]]\nbias_short: r=2 c=1 [[11], [14]] t=[[11, 14]]\nbias_long: r=2 c=3 [[11, 22, 33], [14, 25, 36]] t=[[11, 14], [22, 25], [33, 36]]\ntranspose: r=3 c=2 [[1, 4], [2, 5], [3, 6]] t=[[1, 2, 3], [4, 5, 6]]\nmul: r=2 c=2 [[22, 28], [49, 64]] t=[[22, 49], [28, 64]]\nmul_empty: r=2 c=0 [[], []] t=[]\nslice_past_end: r=1 c=3 [[4, 5, 6]] t=[[4], [5], [6]]\ngather_oob: r=4 c=3 [[4, 5, 6], [0, 0, 0], [0, 0, 0], [1, 2, 3]] t=[[4, 0, 0, 1], [5, 0, 0, 2], [6, 0, 0, 3]]\ngather_empty: r=0 c=0 [] t=[]\nsplit_uneven: r=2 c=1 [[1], [4]] t=[[1, 4]]\nsplit_uneven: r=2 c=1 [[2], [5]] t=[[2, 5]]\nsplit_over: r=2 c=0 [[], []] t=[]\nsplit_over: r=2 c=0 [[], []] t=[]\nsplit_over: r=2 c=0 [[], []] t=[]\nsplit_over: r=2 c=0 [[], []] t=[]\nconcat_cols: r=2 c=4 [[1, 2, 3, 7], [4, 5, 6, 8]] t=[[1, 4], [2, 5], [3, 6], [7, 8]]\nconcat_cols_empty_first: r=1 c=0 [[]] t=[]\nconcat_rows: r=4 c=3 [[1, 2, 3], [4, 5, 6], [1, 2, 3], [4, 5, 6]] t=[[1, 4, 1, 4], [2, 5, 2, 5], [3, 6, 3, 6]]\nfrom_f64: r=3 c=2 [[1, 2], [3, 4], [5, 6]] t=[[1, 3, 5], [2, 4, 6]]\nfrom_f32: r=3 c=2 [[1, 2], [3, 4], [5, 6]] t=[[1, 3, 5], [2, 4, 6]]\nfrom_f16: r=3 c=2 [[1, 2], [3, 4], [5, 6]] t=[[1, 3, 5], [2, 4, 6]]\n", "", 0);
    check_routes(&dir, "matrix_product_family", include_str!("../spec/wasm_cross/matrix_product_family.almd"), "mul: (2, 2) [[13.5, 15.75], [30, 37.5]]\nmul_k0: (3, 2) [[0, 0], [0, 0], [0, 0]]\nmul_b0: (2, 0) [[], []]\nmul_a0: (0, 0) []\n", "", 0);
    check_routes(&dir, "matrix_shape_precondition_family", include_str!("../spec/wasm_cross/matrix_shape_precondition_family.almd"), "mul=r=2 c=2 [[4, 5], [10, 11]]\nconcat=r=3 c=4 [[1, 0, 1, 0], [0, 1, 0, 1], [1, 1, 1, 1]]\nmul_empty_l=r=0 c=0 []\nmul_empty_r=r=2 c=0 [[], []]\nconcat_empty_member=r=3 c=2 [[1, 0], [0, 1], [1, 1]]\nconcat_empty_first=r=1 c=0 [[]]\nmul_rx0=r=2 c=0 [[], []]\nmul_empty_wide=r=0 c=0 []\nmul_empty_wide_r=r=1 c=0 [[]]\nmul_zeros_0x3_r=r=2 c=0 [[], []]\nfrom_uniform=r=2 c=2 [[1, 2], [3, 4]]\nfrom_zero_wide=r=2 c=0 [[], []]\nadd_zip=r=2 c=2 [[2, 2], [4, 6]]\nsub_zip=r=2 c=2 [[0, 2], [4, 4]]\ndiv_zip=r=2 c=2 [[1, inf], [inf, 5]]\nbar_short=r=2 c=1 [[11], [14]]\nbar_long=r=2 c=3 [[11, 22, 33], [14, 25, 36]]\n", "", 0);
    check_routes(&dir, "matrix_zero_cols_rows", include_str!("../spec/wasm_cross/matrix_zero_cols_rows.almd"), "zeros3x0: rows=3 cols=0 lists=[[], [], []]\nzeros1x-1: rows=1 cols=0 lists=[[]]\nzeros0x3: rows=0 cols=0 lists=[]\nones2x0: rows=2 cols=0 lists=[[], []]\nf32 2x0: some rows=2 cols=0 lists=[[], []]\nf32 past-end 1xMIN: some rows=1 cols=0 lists=[[]]\nf32 beyond-end 1x0: none\nf16 1x-1: some rows=1 cols=0 lists=[[]]\nf64 1x-1: some rows=1 cols=0 lists=[[]]\nlists3x0: rows=3 cols=0 lists=[[], [], []]\nadd3x0: rows=3 cols=0 lists=[[], [], []]\nzeros0x3 shape: (0, 0)\nf32 0x3: some rows=0 cols=0 lists=[]\nf32 0x3 off1: some (0, 0)\nf32 0x3 empty buf: some (0, 0)\nf16 0x3: some rows=0 cols=0 lists=[]\nf64 0x3 past-end: none\ngather none x3: rows=0 cols=0 lists=[]\ntranspose3x0: rows=0 cols=0 lists=[]\n", "", 0);
    check_routes(&dir, "matrix_dims_guard_product", include_str!("../spec/wasm_cross/matrix_dims_guard_product.almd"), "ok: 2 x 3\n", "Error: matrix dimensions too large\n", 1);
    check_routes(&dir, "matrix_select_rows_oob", include_str!("../spec/wasm_cross/matrix_select_rows_oob.almd"), "f32_shifted=[[0, 0], [-3, 4], [5, 6.5]]\nf32_edge=[[0, 0], [1.5, 2.5], [1.5, 2.5]]\nf32_negoff=[[1.5, 2.5]]\n", "", 0);
}

#[test]
fn native_os_entropy_preserves_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-entropy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("random_bytes", include_str!("../spec/wasm_cross/random_bytes.almd"),
         "len 32: 32 32\ntwo draws differ: true\nlen 0: 0\nlen 1: 1\nlen 256: 256\nlen 300: 300\ntail drawn: true\n", "", 0),
        ("random_bytes_negative_abort", include_str!("../spec/wasm_cross/random_bytes_negative_abort.almd"),
         "pre: 0\n", "Error: random.bytes: negative length -1\n", 1),
        ("uuid_v7_monotonic", include_str!("../spec/wasm_cross/uuid_v7_monotonic.almd"),
         "count: 2000\nversion 7: true\nvariant 10: true\n8-4-4-4-12 lowercase: true\n16 bytes: true\nstamps strictly increase: true\nuuids strictly increase: true\n", "", 0),
    ];
    for (name, source, stdout, stderr, exit) in cases {
        let mut expected_caps = vec![almide_mir::Capability::Stdout.id(), almide_mir::Capability::Entropy.id()];
        if name == "uuid_v7_monotonic" { expected_caps.push(almide_mir::Capability::Clock.id()); }
        assert_eq!(almide_mir::pipeline::native_capability_reach(source).unwrap(),
            expected_caps.into_iter().collect(), "{name}: native host capabilities");
        check_routes(&dir, name, source, stdout, stderr, exit);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn borrowed_mutation_and_unrelated_fan_slots_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-borrowed-mutation-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "match_binder_stored_owned", include_str!("../spec/wasm_cross/match_binder_stored_owned.almd"),
        "1.5\n1.5\n1.5\n1.5\n1.5\n1.5\n1\n3.0\n4.0\n5.0\n7.0\n8.0\na\n3\nb\nc!\n", "", 0);
    check_routes(&dir, "generated_binder_hygiene", include_str!("../spec/wasm_cross/generated_binder_hygiene.almd"),
        "6.0\n5\n15\n7\n7\n4\n4\n51\n3\n7.5\n118\n", "", 0);
    check_routes(&dir, "borrowed_mutation_shapes", r#"type C = { r: Float }
fn bump(mut xs: List[Int], k: Int) -> Unit = {
 xs[0] = xs[0] + k
 xs[1] = xs[0] + xs[1]
}
fn paint(mut xs: List[C], c: C) -> Unit = { xs[0] = c }
fn main() -> Unit = {
 var xs = [2, 3]
 let old = xs
 bump(xs, 5)
 println("${xs} ${old}")
 var cs = [C { r: 1.0 }]
 let kept = cs
 paint(cs, C { r: 4.0 })
 println("${cs[0].r} ${kept[0].r}")
}
"#, "[7, 10] [2, 3]\n4 1\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn float_variant_slots_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-float-variant-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "ref_grain_nan_struct", include_str!("../spec/wasm_cross/ref_grain_nan_struct.almd"),
        "ne\nT\nT\nF\nstruct-ne\nlist-ne\nNaN inf -inf\n0.0\n", "", 0);
    check_routes(&dir, "list_cross_module_variant", include_str!("../spec/wasm_cross/list_cross_module_variant.almd"),
        "3\n21.700000000000003\n0\n1\n7\n", "", 0);
    check_routes(&dir, "enum_float_slots", r#"type N = | Empty | One(Float) | Pair(Float, Float) | Mixed(Int, Bool, Float)
fn value(n: N) -> Float = match n {
 Empty => 42.0,
 One(x) => x,
 Pair(x, y) => x + y,
 Mixed(i, b, x) => if b then x + (if i == 2 then 2.0 else 0.0) else x,
}
fn main() -> Unit = {
 let nan = 0.0 / 0.0
 let neg = One(-0.0)
 println(float.to_string(1.0 / value(neg)))
 println(float.to_string(value(Pair(1.25, 2.5))))
 println(float.to_string(value(Mixed(2, true, 0.5))))
 println(if One(nan) == One(nan) then "bad" else "nan unequal")
 println(if One(-0.0) == One(0.0) then "zero equal" else "bad")
 let xs = [Empty, neg, One(nan), Pair(0.25, -0.25)]
 for x in xs {
  let y = value(x)
  println(if y == y then float.to_string(y) else "NaN")
 }
}
"#, "-inf\n3.75\n2.5\nnan unequal\nzero equal\n42.0\n-0.0\nNaN\n0.0\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn declared_defaults_preserve_codepoint_and_byte_contracts() {
    let dir = std::env::temp_dir().join(format!("almide-native-defaults-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "string_byte_slice", include_str!("../spec/wasm_cross/string_byte_slice.almd"),
        "byte offsets and codepoints ok\n", "", 0);
    check_routes(&dir, "string_codepoint_index", include_str!("../spec/wasm_cross/string_codepoint_index.almd"),
        "len=6\nindex_of_abc=3\nindex_of_u=2\nlast_index_of_a=0\nindex_of_missing=-1\nslice13=いう\nslice2=うabc\nslice_neg=あい\nget1=い\nget4=b\nget_neg=?\nget_oob=?\ncompose=あいう\nascii_index_of=2\nascii_len=5\nemoji_len=3\nemoji_index=2\nempty_last=3\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn generic_scalar_arguments_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-generic-scalar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "stdlib_manufactured_nan_sign", include_str!("../spec/wasm_cross/stdlib_manufactured_nan_sign.almd"),
        "math.sqrt: +NaN +NaN\nmath.sqrt: +NaN +NaN\nmath.log: +NaN +NaN\nmath.log2: +NaN +NaN\nmath.log10: +NaN +NaN\nmath.log.log: +NaN +NaN\nfloat.pow: +NaN +NaN\nfloat.pow(-1): +NaN +NaN\npow operator: +NaN +NaN\nmath.sin: +NaN +NaN\nmath.sin(-inf): +NaN +NaN\nmath.cos: +NaN +NaN\nmath.cos(-inf): +NaN +NaN\nmath.tan: +NaN +NaN\nmath.tan(-inf): +NaN +NaN\nseed 1792404782 #69: some(-0)\n", "", 0);
    check_routes(&dir, "generic_float_insert", r#"fn main() -> Unit = {
 let old = [-0.0, 3.5]
 let neg = list.insert(old, 0, -0.0)
 let tail = list.insert(old, 99, 1.25)
 let at = list.insert(old, 1, -2.5)
 println("${neg} ${tail} ${at} ${old}")
 println(float.to_string(1.0 / neg[0]))
 println(float.to_string(1.0 / old[0]))
 let xs = list.insert([1.0], 0, 0.0 / 0.0)
 println(if xs[0] == xs[0] then "bad" else "nan unequal")
 println("${list.insert([1, 3], 1, 2)}")
 println("${list.insert([false], 0, true)}")
}
"#,
        "[-0, -0, 3.5] [-0, 3.5, 1.25] [-0, -2.5, 3.5] [-0, 3.5]\n-inf\n-inf\nnan unequal\n[1, 2, 3]\n[true, false]\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn global_mutable_receivers_publish_between_sibling_expressions() {
    let dir = std::env::temp_dir().join(format!("almide-native-global-mutation-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "mutable_global_field_mut_arg", include_str!("../spec/wasm_cross/mutable_global_field_mut_arg.almd"),
        "3 3\n0,1,2,3,4,u,6\nhi 1 1\n6 6\n0 0 0\n", "", 0);
    check_routes(&dir, "mutable_field_alias", r#"type Box = { xs: List[String], tail: List[Float] }
var box: Box = { xs: ["a", "b"], tail: [-0.0, 2.5] }
fn main() -> Unit = {
 let before = box
 let last = list.pop(box.xs)
 println("${last} ${box.xs} ${before.xs}")
 let n = list.pop(box.tail)
 println("${n} ${box.tail} ${before.tail}")
 let zero = list.pop(box.tail) ?? 1.0
 println(float.to_string(1.0 / zero))
 println("${list.pop(box.tail)} ${list.pop(box.xs)} ${list.pop(box.xs)}")
 println("${box.xs} ${box.tail} ${before.xs} ${before.tail}")
}
"#,
        "some(\"b\") [\"a\"] [\"a\", \"b\"]\nsome(2.5) [-0] [-0, 2.5]\n-inf\nnone some(\"a\") none\n[] [] [\"a\", \"b\"] [-0, 2.5]\n", "", 0);
    check_routes(&dir, "mutable_field_order", r#"type Box = { xs: List[String] }
var box: Box = { xs: ["base"] }
fn append_inner() -> String = { list.push(box.xs, "inner"); "outer" }
fn show(n: Int, p: String?, m: Int) -> String = "${n} ${p} ${m}"
fn main() -> Unit = {
 let old = box
 list.push(box.xs, append_inner())
 println("${box.xs} ${old.xs}")
 println(show(list.len(box.xs), list.pop(box.xs), list.len(box.xs)))
 let pair = (list.len(box.xs), list.pop(box.xs), list.len(box.xs))
 println("${pair}")
 let hidden = if list.len(box.xs) == 77 then list.pop(box.xs) else none
 println("${hidden} ${box.xs}")
 for _ in 0..<2 { println("${list.pop(box.xs)} ${list.len(box.xs)}") }
 println("${old.xs}")
}
"#,
        "[\"base\", \"inner\", \"outer\"] [\"base\"]\n3 some(\"outer\") 2\n(2, some(\"inner\"), 1)\nnone [\"base\"]\nsome(\"base\") 0\nnone 0\n[\"base\"]\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ordered_fan_mappers_preserve_every_element_and_first_error() {
    let dir = std::env::temp_dir().join(format!("almide-native-ordered-fan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "fan_block_rc_closure_capture", include_str!("../spec/wasm_cross/fan_block_rc_closure_capture.almd"), "block 30\ncaptures 38 p\nmap 60\nparam 9\nbytes 7\nany 10\narm 2\narm 1\narm 3\n", "Error: arm one failed\n", 1);
    check_routes(&dir, "fan_effect_mapper_capability", include_str!("../spec/wasm_cross/fan_effect_mapper_capability.almd"), "B ok alpha\nB err fb\nD ok beta\nD err fb2\nmap ok [\"alpha\", \"beta\"]\nmap err [\"fb3\"]\n", "", 0);
    check_routes(&dir, "fan_map_err_runs_every_element", include_str!("../spec/wasm_cross/fan_map_err_runs_every_element.almd"), "element 1\nelement 2\nelement 3\nelement 4\nelement 5\n", "Error: element 2 failed\n", 1);
    check_routes(&dir, "fan_prefetch_fs", include_str!("../spec/wasm_cross/fan_prefetch_fs.almd"), "alpha|bravo|charlie\nerr=fs.read_text(\"almide_xt_fanpf_tmp\"): Is a directory (os error 21)\ndirect-err=fs.read_text(\"almide_xt_fanpf_tmp\"): Is a directory (os error 21)\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn owned_loop_containers_and_lazy_constructor_payloads_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-loop-containers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "result_list_value_int_tuple", include_str!("../spec/wasm_cross/result_list_value_int_tuple.almd"), "3 @ 3\n", "", 0);
    check_routes(&dir, "list_of_maps_loop_build", include_str!("../spec/wasm_cross/list_of_maps_loop_build.almd"), "0\n", "", 0);
    check_routes(&dir, "constructor_fallbacks", r#"fn fallback(label: String) -> String = { println(label); label }
fn wrap(x: String?) -> (String?, Result[String, String], Result[Int, String]) = {
 let a = some(x ?? fallback("some"))
 let b: Result[String, String] = ok(x ?? fallback("ok"))
 let c: Result[Int, String] = err(x ?? fallback("err"))
 (a, b, c)
}
fn main() -> Unit = {
 println("${wrap(some("keep"))}")
 println("${wrap(none)}")
}
"#, "(some(\"keep\"), ok(\"keep\"), err(\"keep\"))\nsome\nok\nerr\n(some(\"some\"), ok(\"ok\"), err(\"err\"))\n", "", 0);
    check_routes(&dir, "loop_owned_containers", r#"fn grow(n: Int, acc: List[Value]) -> Result[(List[Value], Int), String] =
 if n < 0 then err("negative")
 else if n == 0 then ok((acc, list.len(acc)))
 else grow(n - 1, acc + [value.int(n)])
fn main() -> Unit = {
 var xs: List[Map[String, Int]] = []
 let empty = xs
 var item = ["id": -1]
 for i in 0..<4 {
  item = map.set(item, "id", i)
  xs = xs + [item]
 }
 let old = xs
 item = map.set(item, "id", 99)
 xs = xs + [["id": 7], ["id": 8]]
 println("${empty} ${old} ${xs} ${item}")
 var ordered = [["id": 11]]
 let joined = ordered + { ordered = [["id": 22]]; ordered }
 println("${joined} ${ordered}")
 println("${grow(3, [])}")
 println("${grow(0, [])}")
 println("${grow(-1, [])}")
}
"#, "[] [[\"id\": 0], [\"id\": 1], [\"id\": 2], [\"id\": 3]] [[\"id\": 0], [\"id\": 1], [\"id\": 2], [\"id\": 3], [\"id\": 7], [\"id\": 8]] [\"id\": 99]\n[[\"id\": 11], [\"id\": 22]] [[\"id\": 22]]\nok(([3, 2, 1], 3))\nok(([], 0))\nerr(\"negative\")\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn parallel_fan_mappers_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-parallel-fan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "fan_deterministic", include_str!("../spec/wasm_cross/fan_deterministic.almd"), "4:2,4,6,8\nn11|n12\nempty=0\nany=7\nsettle=ok(1) err(\"bad\") ok(3)\n", "", 0);
    check_routes(&dir, "fan_effect_mapper", include_str!("../spec/wasm_cross/fan_effect_mapper.almd"), "any=50\nmap=30,40\nsettle=3 oks=2:50,90\n", "", 0);
    check_routes(&dir, "fan_map_all_workers_abort_one_line", include_str!("../spec/wasm_cross/fan_map_all_workers_abort_one_line.almd"), "a\n", "Error: out of memory\n", 1);
    check_routes(&dir, "fan_map_err", include_str!("../spec/wasm_cross/fan_map_err.almd"), "", "Error: element 2 failed\n", 1);
    check_routes(&dir, "fan_map_inline_err", include_str!("../spec/wasm_cross/fan_map_inline_err.almd"), "", "Error: element 2 failed\n", 1);
    check_routes(&dir, "fan_map_inline_lambda", include_str!("../spec/wasm_cross/fan_map_inline_lambda.almd"), "tens=3:10,20,30\nscaled=100,200\nnamed=n7|n8\nbanged=a!,b!\nnested=1001,1002,1003\n", "", 0);
    check_routes(&dir, "fan_map_worker_abort_keeps_stdout", include_str!("../spec/wasm_cross/fan_map_worker_abort_keeps_stdout.almd"), "a\n", "Error: out of memory\n", 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn parallel_fan_typed_captures_and_payloads_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-parallel-values-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "parallel_map_values", r#"type Entry = { name: String, n: Int }
effect fn main() -> Unit = {
  let prefix = "p:"
  let words = ["a", "b", "c"]
  let before = words
  let mapped = fan.map(words, (s) => ok(prefix + s))!
  println("${before} ${mapped} ${prefix}")
  let floats = fan.map([1.5, -2.0, 0.25], (x) => ok(x + 0.5))!
  println("${floats}")
  let to_float = fan.map([1, 2, 3], (x) => ok(float.from(x) / 4.0))!
  println("${to_float}")
  let to_int = fan.map([1.5, -2.0], (x) => ok(if x > 0.0 then 7 else 9))!
  println("${to_int}")
  let to_str = fan.map([1.5, -2.0], (x) => ok(float.to_string(x)))!
  println("${to_str}")
  let pairs = fan.map([1, 2], (x) => ok((x, prefix)))!
  println("${pairs}")
  let records = fan.map(pairs, (pair) => {
    let (n, s) = pair
    ok(Entry { name: s + int.to_string(n), n: n * 10 })
  })!
  println("${records}")
  let names = fan.map(records, (r) => ok(r.name))!
  println("${names}")
}
"#, "[\"a\", \"b\", \"c\"] [\"p:a\", \"p:b\", \"p:c\"] p:\n[2, -1.5, 0.75]\n[0.25, 0.5, 0.75]\n[7, 9]\n[\"1.5\", \"-2.0\"]\n[(1, \"p:\"), (2, \"p:\")]\n[Entry { name: \"p:1\", n: 10 }, Entry { name: \"p:2\", n: 20 }]\n[\"p:1\", \"p:2\"]\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn parallel_fan_blocks_and_callback_carriers_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-parallel-blocks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "fan_map_nested_combinators", include_str!("../spec/wasm_cross/fan_map_nested_combinators.almd"), "map=3 14\nfilter=1,2\nfold=6,36\nflat_map=4 55\nblock=306,2,24,6\nany=18\n", "", 0);
    check_routes(&dir, "fan_map_parallel_scalar", include_str!("../spec/wasm_cross/fan_map_parallel_scalar.almd"), "pairs=40 last=39,726603\ncell=7 135114.7142857143\nerr=bad 3\nsums=269400,1951406\npicked=97600,87000\n", "", 0);
    check_routes(&dir, "fan_list_map_trap_order", include_str!("../spec/wasm_cross/fan_list_map_trap_order.almd"), "", "Error: index out of bounds\n", 1);
    check_routes(&dir, "fan_worker_abort_keeps_stdout", include_str!("../spec/wasm_cross/fan_worker_abort_keeps_stdout.almd"), "a\n", "Error: out of memory\n", 1);
    check_routes(&dir, "leak_free_result_option_carriers", include_str!("../spec/wasm_cross/leak_free_result_option_carriers.almd"), "to_option: 4 -1\nunwrap_or: in 41 fb -1\ncombinators: abc? abcF -\nfilter_map: e2,e4\nfan: p3,p4 p5 5\ndistinct: 3\nmut: 12 10\n", "", 0);
    check_routes(&dir, "witness_straightline", include_str!("../spec/wasm_cross/witness_straightline.almd"), "3 w 3 3\n2 7 W\nhi\n3 hi w! 2\n4 0 a! b\n3 11 6\ne\n2 3\n3 4\nx hi y!\n[\"a!\", \"bc!\"] [\"bc\"] 3\nabc bc\n[1, 2] [0]\n2 5\n2\n[\"a!\", \"b!\"] c!\nq p!q\nf g\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn parallel_list_family_typed_captures_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-parallel-list-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "parallel_list_values", r#"type Entry = { name: String, n: Int }
effect fn main() -> Unit = {
 let xs = [-1.5, 2.0, 3.25]
 let old = xs
 let (a, b, c, d) = fan {
  list.map(xs, (x) => x + 0.5)
  list.filter(xs, (x) => x > 0.0)
  list.any(xs, (x) => x < 0.0)
  list.all(xs, (x) => x > 0.0)
 }
 println("${old} ${a} ${b} ${c} ${d}")
 let prefix = "p:"
 let words = ["a", "b"]
 let (records, kept, any, all) = fan {
  list.map(words, (s) => Entry { name: prefix + s, n: string.len(s) })
  list.filter(words, (s) => s == "b")
  list.any(words, (s) => s == "a")
  list.all(words, (s) => string.len(s) == 1)
 }
 println("${words} ${records} ${kept} ${any} ${all} ${prefix}")
 let no_values: List[Int] = []
 let (empty, found, yes) = fan {
  list.map(no_values, (x) => x + 1)
  list.any(no_values, (x) => x == 0)
  list.all(no_values, (x) => x == 0)
 }
 println("${empty} ${found} ${yes}")
}
"#, "[-1.5, 2, 3.25] [-1, 2.5, 3.75] [2, 3.25] true false\n[\"a\", \"b\"] [Entry { name: \"p:a\", n: 1 }, Entry { name: \"p:b\", n: 1 }] [\"b\"] true true p:\n[] false true\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn runtime_backed_nominal_releases_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-nominal-release-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "stdlib_opaque_shadow", include_str!("../spec/wasm_cross/stdlib_opaque_shadow.almd"), "x {\"a\":[1,2]} 3\ntrue false\n2 b z\n2 [1,2]\n", "", 0);
    check_routes(&dir, "stdlib_type_shadow", include_str!("../spec/wasm_cross/stdlib_type_shadow.almd"), "14 big 21 true Value { n: 7 }\n{\"a\":1} 3\nsmall 3 big body\n258 513 1 3 [1, 2, 3]\n5 1\n9 example.com\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn callback_result_constraints_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-callback-results-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "map_set_typechange", include_str!("../spec/wasm_cross/map_set_typechange.almd"), "a=2,bb=3,ccc=1\na=1!,b=22!,c=333!\na=0.5,b=11.0,c=166.5\na=1:1,b=22:484,c=333:110889\n1=2,2=3,3=1\na=10,b=20,c=30\n1,2,3\nn1,n2,n3\nF,T\n10,20,30\n", "", 0);
    check_routes(&dir, "runtime_callback_results", r#"fn main() -> Unit = {
 let mi = ["a": 1, "b": 22, "c": -3]
 let mf = map.map(mi, (v) => float.from(v) / 2.0)
 println("${mi} ${mf}")
 let signs = map.map(mf, (v) => if v < 1.0 then -1 else 1)
 println("${signs}")
 let stored = (v: Float) => v + 0.25
 println("${map.map(mf, stored)}")
 let offset = 0.125
 let shifted = map.map(mf, (v) => v + offset)
 println("${shifted} ${mf}")
 let int_keys = [1: 2, 2: 5]
 let values = map.map(int_keys, (v) => float.from(v) / 4.0)
 println("${values}")
 let nested = map.map(mi, (v) => [float.from(v), -0.0])
 println("${nested}")
}
"#, "[\"a\": 1, \"b\": 22, \"c\": -3] [\"a\": 0.5, \"b\": 11, \"c\": -1.5]\n[\"a\": -1, \"b\": 1, \"c\": -1]\n[\"a\": 0.75, \"b\": 11.25, \"c\": -1.25]\n[\"a\": 0.625, \"b\": 11.125, \"c\": -1.375] [\"a\": 0.5, \"b\": 11, \"c\": -1.5]\n[1: 0.5, 2: 1.25]\n[\"a\": [1, -0], \"b\": [22, -0], \"c\": [-3, -0]]\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn exhaustive_boolean_tuple_bindings_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-boolean-tuples-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "boolean_tuple_results", r#"fn pick(t: (Bool, String, Int)) -> String = match t {
 (true, text, _) => text,
 (false, _, n) => int.to_string(n),
}
fn guards(t: (Bool, String)) -> String = match t {
 (true, text) if text == "hit" => text + "!",
 (true, text) => text + "?",
 (false, text) => text + ".",
}
fn optional(t: (Bool, String, Int)?) -> String = match t {
 none => "absent",
 some((true, text, _)) if text == "hit" => text + "!",
 some((true, text, _)) => text + "?",
 some((false, _, n)) => int.to_string(n),
}
fn main() -> Unit = {
 println(pick((true, "yes", 7)))
 println(pick((false, "ignored", -4)))
 println(guards((true, "hit")))
 println(guards((true, "miss")))
 println(guards((false, "tail")))
 println(optional(none))
 println(optional(some((true, "hit", 1))))
 println(optional(some((true, "miss", 1))))
 println(optional(some((false, "skip", 9))))
}
"#, "yes\n-4\nhit!\nmiss?\ntail.\nabsent\nhit!\nmiss?\n9\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn primary_nominal_library_bodies_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-primary-path-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "primary_path_values", r#"import json
fn path_of(n: Int) -> value.Path = list.fold(list.range(0, n), value.root(), (p, _) => value.key(p, "k"))
fn main() -> Unit = {
 let path = path_of(2)
 let doc = value.set_path(value.null(), path, value.int(7)) ?? value.null()
 println(json.stringify(doc))
 println(json.stringify(value.get_path(doc, path) ?? value.null()))
 let array = value.array([value.int(1), value.int(2)])
 let index = value.index(value.root(), 1)
 println(json.stringify(value.get_path(array, index) ?? value.null()))
 println(json.stringify(value.get_path(array, value.index(value.root(), -1)) ?? value.null()))
 println(json.stringify(value.remove_path(array, index)))
 println(json.stringify(value.remove_path(doc, path)))
 println(json.stringify(doc))
}
"#, "{\"k\":{\"k\":7}}\n7\n2\nnull\n[1]\n{\"k\":{}}\n{\"k\":{\"k\":7}}\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn owned_fallback_fields_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-owned-fallback-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "owned_fallback_fields", r#"fn row(o: String?, fallback: List[String]) -> (Int, String, String) = (1, o ?? fallback[0], "tail")
fn main() -> Unit = {
 let original = some("kept")
 println("${row(original, [])}")
 println("${original}")
 println("${row(none, ["fallback"])}")
}
"#, "(1, \"kept\", \"tail\")\nsome(\"kept\")\n(1, \"fallback\", \"tail\")\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn owned_map_captures_and_runtime_headers_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-map-captures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "http_response_headers", include_str!("../spec/wasm_cross/http_response_headers.almd"), "text/plain; charset=utf-8 1 1 1\ntext/plain; charset=utf-8\nc/d c/d 2\ntext/plain; charset=utf-8 text/plain; charset=utf-8 text/plain; charset=utf-8 none\napplication/json application/json\napplication/octet-stream\n/x /x text/plain; charset=utf-8 ''\nv1 v1 text/plain; charset=utf-8\nv v text/plain; charset=utf-8\nhi text/plain; charset=utf-8\nyo application/octet-stream\n", "", 0);
    check_routes(&dir, "runtime_backed_value_repr", include_str!("../spec/wasm_cross/runtime_backed_value_repr.almd"), "HttpRequest { method: \"POST\", path: \"/login?next=%2F\", body: \"a\\\"b\\\\c\\nd\\re\\tf\", headers: 2 }\nHttpResponse { status: 401, body: \"denied\", headers: 4 }\nWrap { req: HttpRequest { method: \"POST\", path: \"/login?next=%2F\", body: \"a\\\"b\\\\c\\nd\\re\\tf\", headers: 2 }, resp: HttpResponse { status: 401, body: \"denied\", headers: 4 } }\n[HttpRequest { method: \"POST\", path: \"/login?next=%2F\", body: \"a\\\"b\\\\c\\nd\\re\\tf\", headers: 2 }, HttpRequest { method: \"POST\", path: \"/login?next=%2F\", body: \"a\\\"b\\\\c\\nd\\re\\tf\", headers: 2 }]\n[HttpResponse { status: 401, body: \"denied\", headers: 4 }]\nsome(HttpRequest { method: \"POST\", path: \"/login?next=%2F\", body: \"a\\\"b\\\\c\\nd\\re\\tf\", headers: 2 })\nsome(HttpResponse { status: 401, body: \"denied\", headers: 4 })\n(HttpRequest { method: \"POST\", path: \"/login?next=%2F\", body: \"a\\\"b\\\\c\\nd\\re\\tf\", headers: 2 }, HttpResponse { status: 401, body: \"denied\", headers: 4 }, 1)\nleaked: false\n", "", 0);
    check_routes(&dir, "runtime_backed_variant_repr", include_str!("../spec/wasm_cross/runtime_backed_variant_repr.almd"), "Got(HttpRequest { method: \"GET\", path: \"/v\", body: \"\", headers: 2 })\nHit { at: HttpResponse { status: 200, body: \"ok\", headers: 1 } }\nTagged(7, HttpRequest { method: \"GET\", path: \"/v\", body: \"\", headers: 2 })\nNothing\nleaked: false\n", "", 0);
    check_routes(&dir, "owned_map_captures", r#"fn picker(m: Map[String, String]) -> (String) -> String = (k) => map.get(m, k) ?? "missing"
fn nested(m: Map[Int, List[String]]) -> (Int) -> List[String] = (k) => map.get(m, k) ?? []
fn row(o: String?, fallback: List[String]) -> List[String] = ["prefix", o ?? fallback[0]]
fn main() -> Unit = {
 let source = ["a": "alpha", "b": "beta"]
 let lookup = picker(source)
 println("${lookup("a")} ${lookup("c")} ${source}")
 let lists = [1: ["one", "first"], 2: ["two"]]
 let get_list = nested(lists)
 println("${get_list(1)} ${get_list(9)} ${lists}")
 println("${row(some("kept"), [])} ${row(none, ["fallback"])}")
 let selected = list.flat_map(["a", "c", "b"], (k) => [k, lookup(k)])
 println("${selected} ${source}")
}
"#, "alpha missing [\"a\": \"alpha\", \"b\": \"beta\"]\n[\"one\", \"first\"] [] [1: [\"one\", \"first\"], 2: [\"two\"]]\n[\"prefix\", \"kept\"] [\"prefix\", \"fallback\"]\n[\"a\", \"alpha\", \"c\", \"missing\", \"b\", \"beta\"] [\"a\": \"alpha\", \"b\": \"beta\"]\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn nominal_display_preserves_privacy_and_evaluation_count() {
    let dir = std::env::temp_dir().join(format!("almide-native-nominal-display-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "runtime_nominal_display", r#"import http
type Event = | Received(HttpRequest) | Missing
type Boxed = { event: Event, requests: List[HttpRequest] }
var calls = 0
fn make() -> HttpRequest = {
 calls = calls + 1
 println("make ${calls}")
 http.new_request("GET", "/secret", "", [("Authorization", "hidden")])
}
fn main() -> Unit = {
 let req = make()
 let requests = [req, req]
 let optional = some(req)
 let result: Result[HttpRequest, String] = ok(req)
 let boxed = Boxed { event: Received(req), requests: requests }
 println("${requests}")
 println("${optional}")
 println("${result}")
 println("${boxed}")
 println("${Received(make())}")
 println("calls ${calls}")
}
"#, "make 1\n[HttpRequest { method: \"GET\", path: \"/secret\", body: \"\", headers: 1 }, HttpRequest { method: \"GET\", path: \"/secret\", body: \"\", headers: 1 }]\nsome(HttpRequest { method: \"GET\", path: \"/secret\", body: \"\", headers: 1 })\nok(HttpRequest { method: \"GET\", path: \"/secret\", body: \"\", headers: 1 })\nBoxed { event: Received(HttpRequest { method: \"GET\", path: \"/secret\", body: \"\", headers: 1 }), requests: [HttpRequest { method: \"GET\", path: \"/secret\", body: \"\", headers: 1 }, HttpRequest { method: \"GET\", path: \"/secret\", body: \"\", headers: 1 }] }\nmake 2\nReceived(HttpRequest { method: \"GET\", path: \"/secret\", body: \"\", headers: 1 })\ncalls 2\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn map_merge_in_place_preserves_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-map_merge_in_place-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "map_merge_in_place", include_str!("../spec/wasm_cross/map_merge_in_place.almd"), "k0=v5,k1=v6,k2=v2,n0=w0,n1=w1,n2=w2,k3=v3,n3=w3,k4=v4,n4=w4,n5=w5,n6=w6\nk0=a,k1=b,k2=c | k0=a,k1=b,k2=c 1 3 | k0=a,k1=b,k2=c\nk3=v3,k0=v5,n0=w0,k1=v1,n1=w1,k2=v2,n2=w2,n3=w3,k4=v4,n4=w4,n5=w5\nk4=v4,k0=v10,n0=w0,k2=v2,n2=w2,n4=w4,k1=v6,n6=w6,k3=v8,n8=w8,n10=w10\np=1,q=3,r=4 | p=1,q=2\nk0=a,k1=b,k2=c 3\n86 40 some(-3) some(9) some(29) [51, 117, 54, 118, 57, 119, 60, 120, 63, 121, 66, 122, 69, 123, 72, 124, 75, 125, 78, 126, 81, 127, 84, 128, 87, 129]\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn list_update_callback_result_ownership_preserves_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-list_update_callback_result_ownership-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "list_update_callback_result_ownership", include_str!("../spec/wasm_cross/list_update_callback_result_ownership.almd"), "borrow res    = err(\"boom\") / 0\nborrow str    = 2 / 0\nborrow li     = [1, 2] / 0\nborrow ls     = [\"x\", \"y\"] / 0\nborrow opt_i  = some(2) / 0\nfresh res     = 3 / 0\nfresh str     = 4 / 0\nfresh li      = 5 / 0\nfresh tup     = 6 / 0\nfresh opt_s   = 7 / 0\nscalar int    = 7 / 0\nkept str      = 8 / 2\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn field_mutators_publish_and_preserve_aliases() {
    let dir = std::env::temp_dir().join(format!("almide-native-field-mutators-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "list_pop_map_insert_on_record_field", include_str!("../spec/wasm_cross/list_pop_map_insert_on_record_field.almd"), "6\n[\"x\": 3] [\"x\": 3, \"y\": 2]\nccc some(\"bb\") some(\"a\") [\"a\"] [] [\"a\", \"bb\", \"ccc\"]\nnone none []\n[:] [\"x\": 3, \"y\": 2] c\n[\"0\": 994, \"1\": 995, \"2\": 996, \"3\": 997, \"4\": 998, \"5\": 999, \"6\": 993] [0, 1] 499499\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn indexed_record_fields_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-indexed-record-fields-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "list_head_read_borrows", include_str!("../spec/wasm_cross/list_head_read_borrows.almd"), "7 -1 first blank\nfirst:2 second:1 blank:0 blank:0\n3 0 first -\n9 0 70 0\nsome([\"a\", \"b\"]) none\nfirst 2 [\"a\", \"b\"] 7 blank\nfirst [\"a\", \"b\"] 2 blank\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn secret_fields_compare_and_keep_redacted_display() {
    let dir = std::env::temp_dir().join(format!("almide-native-secret-fields-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "secret_eq_and_decode", include_str!("../spec/wasm_cross/secret_eq_and_decode.almd"), "true false true false\ntrue false\ntrue false false\ntrue false false\ntrue false false\nLoginInput { user: \"ana\", password: Secret(***), pin: Secret(***) }\ntrue 0\n42\ntrue\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn nested_literal_lists_and_constant_aliases_preserve_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-nested-literals-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "codec_triple_list", include_str!("../spec/wasm_cross/codec_triple_list.almd"), "{\"ll\":[[[1,2],[]],[[3]]]}\n2\n6\n", "", 0);
    check_routes(&dir, "list_heap_ops_release_temporaries", include_str!("../spec/wasm_cross/list_heap_ops_release_temporaries.almd"), "contains=TF\nflatten=a,b,c\nflat_map=ab!,ab,x!,x,cd!,cd,ab!,ab / c1,c2,c1,c2\nfilter_map=ab:long,cd:long,ab:long\nfilter_map_borrowed=q,q,q,q\nunique_by=ab,x\nupdate=ab,xx!,cd,ab\nset=zz!,x,cd,ab / ab,x,cd,ab\nfold=sxxxx\n", "", 0);
    check_routes(&dir, "gleam_toplet_consts", include_str!("../spec/wasm_cross/gleam_toplet_consts.almd"), "111\n111\n1111\nguard-hit\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn map_value_equality_preserves_order_nan_and_operand_effects() {
    let dir = std::env::temp_dir().join(format!("almide-native-map-value-equality-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "map_value_equality", r#"fn tagged(label: String, m: Map[String, Value]) -> Map[String, Value] = {
 println(label)
 m
}
fn main() -> Unit = {
 let a = ["a": value.int(1), "b": value.array([value.string("x")])]
 let b = ["b": value.array([value.string("x")]), "a": value.int(1)]
 let c = ["a": value.int(2), "b": value.array([value.string("x")])]
 let d = ["a": value.int(1), "c": value.array([value.string("x")])]
 let empty: Map[String, Value] = map.new()
 println("${tagged("left", a) == tagged("right", b)} ${a != b} ${a == c} ${a == d}")
 println("${empty == empty} ${empty == a} ${a != empty}")
 let ordered = ["x": value.object([("a", value.int(1)), ("b", value.int(2))])]
 let reversed = ["x": value.object([("b", value.int(2)), ("a", value.int(1))])]
 println("${ordered == reversed} ${ordered != reversed}")
 let nan = ["x": value.float(float.from_bits(9221120237041090560))]
 let zero = ["x": value.float(0.0)]
 let negzero = ["x": value.float(-0.0)]
 println("${nan == nan} ${nan != nan} ${zero == negzero}")
 let ia = map.set(map.set(map.new(), 1, value.string("one")), 2, value.null())
 let ib = map.set(map.set(map.new(), 2, value.null()), 1, value.string("one"))
 println("${ia == ib} ${ia != ib}")
}
"#, "left\nright\ntrue false false false\ntrue false true\nfalse true\nfalse true true\ntrue false\n", "", 0);
    check_routes(&dir, "json_to_map_duplicate_keys", include_str!("../spec/wasm_cross/json_to_map_duplicate_keys.almd"), "twice [\"k\": 2] len=1\n  k -> 2 get=some(2)\nfirst-position [\"a\": \"y\", \"b\": true] len=2\n  a -> \"y\" get=some(\"y\")\n  b -> true get=some(true)\nmixed [\"s\": [], \"n\": null, \"f\": 1.5, \"arr\": [1]] len=4\n  s -> [] get=some([])\n  n -> null get=some(null)\n  f -> 1.5 get=some(1.5)\n  arr -> [1] get=some([1])\nbuilt [\"k\": {\"x\":3}, \"j\": \"two\"] len=2\n  k -> {\"x\":3} get=some({\"x\":3})\n  j -> \"two\" get=some(\"two\")\nparsed err json: duplicate key at pos 17\nempty [:] len=0\nnot-object none\nequal=true\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mutable_parameters_share_closure_cells_and_publish_changes() {
    let dir = std::env::temp_dir().join(format!("almide-native-mut-param-cells-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "mut_param_closure_capture_read", include_str!("../spec/wasm_cross/mut_param_closure_capture_read.almd"), "draw 12 w=3\nlater 206 n=3\nthrough_callee 612 n=6\nmixed 75 n=12\n", "", 0);
    check_routes(&dir, "mut_param_closure_capture_write", include_str!("../spec/wasm_cross/mut_param_closure_capture_write.almd"), "total 10 n=6\nstored 88 n=36\nrecord n=39 log=3\ncount 22 c=11\ngrow len=4 last=2\nerr over: 7 n=7\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fallible_field_operands_preserve_partial_writes_and_short_circuiting() {
    let dir = std::env::temp_dir().join(format!("almide-native-field-propagation-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "fallible_field_assignment", r#"type R = { n: Int }
effect fn write(mut r: R, text: String) -> Unit = {
 r.n = r.n + 10
 r.n = r.n + int.parse(text)!
}
fn report(r: Result[Unit, String]) -> String = match r {
 ok(_) => "ok", err(_) => "err",
}
effect fn main() -> Unit = {
 var r = R { n: 1 }
 println("${report(write(r, "2"))} ${r.n}")
 println("${report(write(r, "bad"))} ${r.n}")
 println("${report(write(r, "3"))} ${r.n}")
}
"#, "ok 13\nerr 23\nok 36\n", "", 0);
    check_routes(&dir, "leak_free_propagating_literal_operand", include_str!("../spec/wasm_cross/leak_free_propagating_literal_operand.almd"), "list err bad x\nlist ok 2\ntuple err bad x\ntuple ok 5\nrecord err bad x\nrecord ok 6\nspread err bad x\nspread ok 9\n", "", 0);
    check_routes(&dir, "unwrap_in_if_arm", include_str!("../spec/wasm_cross/unwrap_in_if_arm.almd"), "ok:3\nok:1\nerr:bad\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn parameter_recurrence_and_ordered_mutation_keep_all_routes() {
    let dir = std::env::temp_dir().join(format!("almide-native-param-recurrence-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "mut_param_tail_recursion", include_str!("../spec/wasm_cross/mut_param_tail_recursion.almd"), "list len=4 head=4 tail=1\nmap len=4 at2=20 at4=40\nscalar d=4\nrepeat len=500 last=1\nmutual total=22 caller=22\n", "", 0);
    check_routes(&dir, "mut_call_hoisted_args_in_branch", include_str!("../spec/wasm_cross/mut_call_hoisted_args_in_branch.almd"), "different\nequal\nnorm calls: 2\npair: 46 n=4\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mutable_loop_conditions_publish_even_the_final_false_check() {
    let dir = std::env::temp_dir().join(format!("almide-native-mut-loop-conditions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "mutable_loop_condition", r#"type R = { n: Int }
fn next(mut r: R) -> Int = { r.n = r.n + 1; r.n }
fn scan(mut r: R, limit: Int) -> Unit = {
 while next(r) < limit { println("body ${r.n}") }
}
fn main() -> Unit = {
 var r = R { n: 0 }
 scan(r, 0)
 println("zero ${r.n}")
 scan(r, 4)
 println("many ${r.n}")
 scan(r, 5)
 println("last ${r.n}")
}
"#, "zero 1\nbody 2\nbody 3\nmany 4\nlast 5\n", "", 0);
    check_routes(&dir, "licm_mut_param_named_call", include_str!("../spec/wasm_cross/licm_mut_param_named_call.almd"), "for-in: 1,2,3\nwhile: 1,2,3\nnested: 1,2,20,3,4,40\neffect: 1,2,3\nclosure: 101,202,303\nbump-and-return: 11,22,33\ntwo-levels: 1,2,3\nfield-path: 701,702,703\nmut-list: 1,2,0,3,0,4\nufcs: 1,2,3\ntype-method: 2,4,6\nwhile-cond: 10,20,30\ndeep-path-in-branch: miss,hit,hit:2\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mutable_unit_guard_skips_the_continuation_on_error() {
    let dir = std::env::temp_dir().join(format!("almide-native-mut-unit-guard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "mutable_unit_guard", r#"effect fn append(mut xs: List[String], k: String) -> Unit = {
 guard k != "" else err("empty")
 xs = xs + [k]
}
fn report(r: Result[Unit, String]) -> String = match r {
 ok(_) => "ok", err(e) => e,
}
effect fn main() -> Unit = {
 var xs: List[String] = []
 println("${report(append(xs, "a"))} ${list.len(xs)}")
 println("${report(append(xs, ""))} ${list.len(xs)}")
 println("${report(append(xs, "b"))} ${list.join(xs, ",")}")
}
"#, "ok 1\nempty 1\nok a,b\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unit_result_binding_keeps_tag_guards_and_tuple_errors() {
    let dir = std::env::temp_dir().join(format!("almide-native-unit-result-binding-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "unit_result_binding", r#"fn pass(r: Result[Unit, (String, Int)]) -> Result[Unit, (String, Int)] = match r {
 ok(u) => ok(u), err(e) => err(e),
}
fn main() -> Unit = {
 match pass(ok(())) { ok(_) => println("ok"), err((m, n)) => println("${m} ${n}") }
 let bad: Result[Unit, (String, Int)] = err(("bad", 7))
 match pass(bad) { ok(_) => println("bad ok"), err((m, n)) => println("${m} ${n}") }
}
"#, "ok\nbad 7\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mutable_result_joins_keep_carriers_and_nested_guard_exits() {
    let dir = std::env::temp_dir().join(format!("almide-native-mutable-result-joins-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "mut_param_effect_can_err",
        include_str!("../spec/wasm_cross/mut_param_effect_can_err.almd"),
        "2 2\nn=4 4\n7 3 7\n[11, 4] 11\n-1 11\n16 16\n22 22\n29 7 29\n1 [9]\n2 s3\n1 2 [8] 2\n5 [9, 1, 5]\n1 2 [\"a\", \"b\"]\n",
        "Error: bad\n", 1);
    check_routes(&dir, "mut_param_move_mode_gaps",
        include_str!("../spec/wasm_cross/mut_param_move_mode_gaps.almd"),
        "[3, 4] [6, 8]\n5 6 -1 [3, 4, 5] [6, 8, 50]\n[1, 7] [2, 14]\nok 22 err neg -2\n[\"x\", \"x\"] [\"x!\"] | err empty\nmismatch 3 [1, 2]\nok true [1]\nmismatch false false true []\n[0, 0, 5]\n",
        "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mutable_higher_order_reassignment_preserves_old_value_on_error() {
    let dir = std::env::temp_dir().join(format!("almide-native-mutable-hof-reassignment-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "mut_param_reassign_from_hof",
        include_str!("../spec/wasm_cross/mut_param_reassign_from_hof.almd"),
        "filter 1a,3c\nmap 10a,20b,30c\nfilter_map 1a!,3c!\nflat_map 1a,1a,2b,2b,3c,3c\nsort_by 3c,2b,1a\nreverse 3c,2b,1a\ntake 1a\ndrop 2b,3c\ntake_while 1a\ndrop_while 2b,3c\nconcat 1a,2b,3c,1a,2b,3c\nunique_by 1a,2b\nslice 1a\ninsert 9z,1a,2b,3c\nremove_at 2b,3c\nupdate 7a,2b,3c\nscan 1a,3b,6c\ntake_end 3c\nfold 3c,2b,1a\nnested 1a\ntwo 2a,4c\nbranch keep 1a,2b,3c\nbranch drop 1a,3c\nloop 3c\nself_read 1a,3c\nkept 2 1a,2b,3c,1a,3c\ntail 3c,2b,1a 1a,2b,3c\nsome 1a,2b,3c\nif_tail 1a,2b,3c \nfallible ok 44,55\nfallible err 06,0x\nstring [HI!]\nmap_filter 2\nmap_set 3 5\nset_filter 2\nset_insert 3\nfield filter 1a,3c\nfield map 101a,103c\nfield sort 103c,101a\nfield reverse 101a,103c\nfield label L+\nfield tags 1\nfield ids 1\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn produced_aggregate_readers_keep_temporaries_and_effect_order() {
    let dir = std::env::temp_dir().join(format!("almide-native-produced-readers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "leak_free_reader_temporaries", include_str!("../spec/wasm_cross/leak_free_reader_temporaries.almd"),
        "match: 13 7 qz\nrest: 10 b,c\neq: true false false true false\nread: 2 7 nx 3 r4\nfloat=1.5 third=0.3333333333333333\nrepr: Circle(9) [\"k\": [1, 2]] [:]\nfold: 7+x-2+\n", "", 0);
    check_routes(&dir, "leak_free_spread_produced_base", include_str!("../spec/wasm_cross/leak_free_spread_produced_base.almd"),
        "item-7 0 t7,u\n5320\n", "", 0);
    check_routes(&dir, "tuple_reader_order", r#"fn mark(mut seen: List[Int], n: Int) -> Int = {
 list.push(seen, n)
 n
}
fn main() -> Unit = {
 var seen: List[Int] = []
 let selected = (mark(seen, 1), mark(seen, 2), mark(seen, 3)).1
 println("${selected} ${seen}")
}
"#, "2 [1, 2, 3]\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn empty_record_protocol_receivers_keep_nominal_types() {
    let dir = std::env::temp_dir().join(format!("almide-native-empty-record-protocol-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "protocol_bound_method_named_like_a_record_field",
        include_str!("../spec/wasm_cross/protocol_bound_method_named_like_a_record_field.almd"),
        "greatest lexical=some(\"pear\") by length=some(\"banana\") empty=none\nplain=[\"apple\", \"banana\", \"fig\", \"fig\", \"kiwi\", \"pear\"]\nshort=[\"fig\", \"fig\", \"kiwi\", \"pear\", \"apple\", \"banana\"]\nbest=some(\"banana\") empty=none\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn global_tagged_reads_keep_shape_and_snapshot_ownership() {
    let dir = std::env::temp_dir().join(format!("almide-native-global-tagged-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "top_let_rc_value", include_str!("../spec/wasm_cross/top_let_rc_value.almd"),
        "0\n3\n3\n4\n1t\n2\n3\n2\n", "", 0);
    check_routes(&dir, "global_tagged_read", include_str!("fixtures/global_tagged_read.almd"), "first 1\nheld first 1\nnone\nlast 2\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn recursive_global_initializers_keep_owned_payloads() {
    let dir = std::env::temp_dir().join(format!("almide-native-recursive-global-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "recursive_payload_top_let", include_str!("../spec/wasm_cross/recursive_payload_top_let.almd"), "node\na\nnone\n", "", 0);
    check_routes(&dir, "recursive_global_values", include_str!("fixtures/recursive_global_values.almd"), "a a a\nheld a\nchanged a\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn short_circuit_tail_calls_preserve_order_and_non_tail_recursion() {
    let dir = std::env::temp_dir().join(format!("almide-native-short-tail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "short_circuit_tail_calls", include_str!("fixtures/short_circuit_tail_calls.almd"), "a4\na3\na2\nall false\nb1\nall true\nc2\nc1\nany true\nany false\n", "", 0);
    check_routes(&dir, "tuple_scrutinee_recursive_variants", include_str!("../spec/wasm_cross/tuple_scrutinee_recursive_variants.almd"),
        "same: true false\nconv: true false\ntag: 10\ntri: 9\nswap: ((2 1) ((2 3) 1)) (9 ((2 3) 1))\ntwice: 10\nopt: 2 8\nguarded: 1 2 3\nvia_var: 6\nafter: (1 (2 3)) (1 2)\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn single_constructor_matches_transfer_the_declared_owned_type() {
    let dir = std::env::temp_dir().join(format!("almide-native-single-ctor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "unzip_accumulator_rotate", include_str!("../spec/wasm_cross/unzip_accumulator_rotate.almd"), "(1,2,3)(10,20,30)\n(9)(8,7)\n()()\n", "", 0);
    check_routes(&dir, "single_ctor_owned_results", include_str!("fixtures/single_ctor_owned_results.almd"), "x/2\nx [\"x\", \"x!\"]\nx/3\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fresh_fallback_callees_own_one_selected_closure() {
    let dir = std::env::temp_dir().join(format!("almide-native-fresh-callee-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "unwrap_or_fresh_closure_fallback", include_str!("../spec/wasm_cross/unwrap_or_fresh_closure_fallback.almd"),
        "fresh 5050\nnamed 7450\ncaptured 7545\nnested 6767\ntail 7450\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn tail_conversion_captures_invocation_values_not_loop_cells() {
    let dir = std::env::temp_dir().join(format!("almide-native-tco-capture-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "merge_runs_shadowed_scrutinee", include_str!("../spec/wasm_cross/merge_runs_shadowed_scrutinee.almd"), "[0,1,2,3][]\n[0,1,2,3][]\n[1,3][2,2,5][]\n[]\n", "", 0);
    check_routes(&dir, "tco_capture_values", include_str!("fixtures/tco_capture_values.almd"), "7 10\n[13, 12, 11]\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn option_none_then_wildcard_preserves_abort_observations() {
    let dir = std::env::temp_dir().join(format!("almide-native-option-none-wildcard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "testing_assert_abort_none", include_str!("../spec/wasm_cross/testing_assert_abort_none.almd"),
        "before\n", "Error: assertion failed\n  at: line 12\n  expected: none\n  found: some([\"a\", \"b\"])\n", 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn interpolation_heap_fallback_preserves_order_and_lazy_evaluation() {
    let dir = std::env::temp_dir().join(format!("almide-native-interpolation-fallback-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "interpolation_heap_fallback", include_str!("fixtures/interpolation_heap_fallback.almd"),
        "before true after true [2]\n[2] true [2, 3]\ntrue after\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn linked_zlib_bodies_keep_writebacks_bounds_and_namespaces() {
    let dir = std::env::temp_dir().join(format!("almide-native-linked-zlib-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "zlib_call_vs_user_namesake", include_str!("../spec/wasm_cross/zlib_call_vs_user_namesake.almd"),
        "hello hello hello\nhello hello hello\nhello hello hello\nhello hello hello\nhello hello hello\naaaaaaaabbbbcc daaaa\na%20b=c%26d\n910\n4\nuser:x 15\n", "", 0);
    check_routes(&dir, "zlib_selfhost", include_str!("../spec/wasm_cross/zlib_selfhost.almd"),
        "decompress-ka-ok\ninflate-ka-ok\ngunzip-ka-ok\ncompress-rt-ok\ndeflate-rt-ok\ngzip-rt-ok\ncompress-level-rt-ok\ndeflate-level-rt-ok\ngunzip-empty-err zlib.gunzip: unexpected end of file\ninflate-bad-table err\ninflate-truncated err\ninflate-fixed-dist30 err\ndecompress-bad-table err\ndecompress-truncated-adler err\ngunzip-truncated-trailer err\n", "", 0);
    check_routes(&dir, "zlib_output_bound", include_str!("../spec/wasm_cross/zlib_output_bound.almd"),
        "inflate_max 1: err zlib: output exceeds 1 bytes\ndecompress_max 1: err zlib: output exceeds 1 bytes\ngunzip_max 1: err zlib: output exceeds 1 bytes\ninflate_max 45: err zlib: output exceeds 45 bytes\ndecompress_max 45: err zlib: output exceeds 45 bytes\ngunzip_max 45: err zlib: output exceeds 45 bytes\ninflate_max 46: err zlib: output exceeds 46 bytes\ndecompress_max 46: err zlib: output exceeds 46 bytes\ngunzip_max 46: err zlib: output exceeds 46 bytes\ninflate_max 47: err zlib: output exceeds 47 bytes\ndecompress_max 47: err zlib: output exceeds 47 bytes\ngunzip_max 47: err zlib: output exceeds 47 bytes\ninflate_max 100: err zlib: output exceeds 100 bytes\ndecompress_max 100: err zlib: output exceeds 100 bytes\ngunzip_max 100: err zlib: output exceeds 100 bytes\ninflate_max 181: err zlib: output exceeds 181 bytes\ndecompress_max 181: err zlib: output exceeds 181 bytes\ngunzip_max 181: err zlib: output exceeds 181 bytes\ninflate_max 182: err zlib: output exceeds 182 bytes\ndecompress_max 182: err zlib: output exceeds 182 bytes\ngunzip_max 182: err zlib: output exceeds 182 bytes\ninflate_max 183: ok 183 68\ndecompress_max 183: ok 183 68\ngunzip_max 183: ok 183 68\ninflate_max 184: ok 183 68\ndecompress_max 184: ok 183 68\ngunzip_max 184: ok 183 68\ninflate_max 1000000: ok 183 68\ndecompress_max 1000000: ok 183 68\ngunzip_max 1000000: ok 183 68\ngunzip_max two 183: err zlib: output exceeds 183 bytes\ngunzip_max two 365: err zlib: output exceeds 365 bytes\ngunzip_max two 366: ok 366 68\ngunzip_max two 367: ok 366 68\ninflate_max stored-cut 49: err zlib: output exceeds 49 bytes\ninflate_max stored-cut 50: err zlib.inflate: incomplete deflate stream\ninflate_max stored-cut 51: err zlib.inflate: incomplete deflate stream\ninflate: ok 183 68\ndecompress: ok 183 68\ngunzip: ok 366 68\n", "PANIC: zlib.inflate_max: max_len must be positive, got 0", 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unit_call_arguments_keep_arity_effects_and_callback_types() {
    let dir = std::env::temp_dir().join(format!("almide-native-unit-calls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "unit_call_arguments", include_str!("fixtures/unit_call_arguments.almd"),
        "named\nmiddle\nABC\nclosure:named\nok(\"named\")\nerr(\"kept\")\nzero\none\nABCDYT\nnamed\nnamed\nnamed\nABCDYTRV\n", "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rooted_fs_callbacks_keep_unit_and_owned_result_branches() {
    let dir = std::env::temp_dir().join(format!("almide-native-rooted-fs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_compiled_routes(&dir, "fs_root_escape", include_str!("../spec/wasm_cross/fs_root_escape.almd"),
        "nested: ok inside\ndot segments: ok inside\ndotdot: err fs: path escapes root: ../secret.txt\ndeep dotdot: err fs: path escapes root: sub/../../secret.txt\nabsolute: err fs: path escapes root: /etc/hosts\nwrite: ok written\nwrite escape: err fs: path escapes root: ../c.txt\nlist: ok a.txt,b.txt\nexists: ok true\nexists escape: err fs: path escapes root: ../secret.txt\nremove: ok removed\nremove escape: err fs: path escapes root: ../secret.txt\noutside kept: true c.txt made: false\nmissing root: err fs.open_root(\"no_such_root_4022\"): not a directory\n", "", 0);
    // This corpus case is already one of the recorded interpreter abstentions.
    // Keep that boundary explicit; native, v0 and wasm above must all execute.
    let ir = interp_oracle::lower(include_str!("../spec/wasm_cross/fs_root_escape.almd")).unwrap();
    let reference = almide_interp::Interpreter::new(&ir).run_main();
    assert!(matches!(reference.status, almide_interp::RunStatus::Unsupported(ref reason)
        if reason == "fs.open_root: no lowered body"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn optional_raw_bytes_keep_binary_data_absence_errors_and_owned_results() {
    let dir = std::env::temp_dir().join(format!("almide-native-optional-bytes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let read_only = "import fs\neffect fn main() -> Unit = { match fs.read_bytes_if_exists(\"unused\") { ok(_) => (), err(_) => () } }";
    assert_eq!(almide_mir::pipeline::native_capability_reach(read_only).unwrap(),
        [almide_mir::Capability::FsRead.id()].into_iter().collect());
    check_routes(&dir, "optional_raw_bytes", include_str!("fixtures/optional_raw_bytes.almd"),
        "none\nsome []\nsome [0, 255, 128, 13, 10]\nsome [1]\nerror\n", "", 0);
    check_routes(&dir, "fs_read_bytes_raw_call_head", include_str!("../spec/wasm_cross/fs_read_bytes_raw_call_head.almd"),
        include_str!("fixtures/fs_read_bytes_raw_call_head.stdout"), "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fs_optional_lines_and_metadata_preserve_routes_and_error_heads() {
    // Use the same compilation stack budget as native capability corpus tests.
    std::thread::Builder::new().stack_size(512 << 20)
        .spawn(fs_optional_lines_and_metadata_on_stack).unwrap().join().unwrap();
}

fn fs_optional_lines_and_metadata_on_stack() {
    let dir = std::env::temp_dir().join(format!("almide-native-fs-metadata-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for api in ["read_lines_if_exists", "stat"] {
        let source = format!("import fs\neffect fn main() -> Unit = {{ match fs.{api}(\"unused\") {{ ok(_) => (), err(_) => () }} }}");
        assert_eq!(almide_mir::pipeline::native_capability_reach(&source).unwrap(),
            [almide_mir::Capability::FsRead.id()].into_iter().collect(), "{api}");
    }
    check_routes(&dir, "fs_append_bytes_true_append", include_str!("../spec/wasm_cross/fs_append_bytes_true_append.almd"),
        include_str!("fixtures/fs_append_bytes_true_append.stdout"), "", 0);
    check_routes(&dir, "fs_metadata_family", include_str!("../spec/wasm_cross/fs_metadata_family.almd"),
        include_str!("fixtures/fs_metadata_family.stdout"), "", 0);
    check_routes(&dir, "fs_read_directory_errno", include_str!("../spec/wasm_cross/fs_read_directory_errno.almd"),
        include_str!("fixtures/fs_read_directory_errno.stdout"), "", 0);
    check_routes(&dir, "fs_read_non_regular", include_str!("../spec/wasm_cross/fs_read_non_regular.almd"),
        include_str!("fixtures/fs_read_non_regular.stdout"), "", 0);
    check_routes(&dir, "fs_composition_family", include_str!("../spec/wasm_cross/fs_composition_family.almd"),
        include_str!("fixtures/fs_composition_family.stdout"), "", 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unit_entry_tail_propagation_preserves_errors_and_success() {
    let dir = std::env::temp_dir().join(format!("almide-native-tail-unit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, stdout, stderr, exit) in [
        ("tail_error", "fn finish() -> Unit! = err(\"tail failure\")\neffect fn main() -> Unit = { println(\"before\"); finish()! }", "before\n", "Error: tail failure\n", 1),
        ("tail_success", "fn finish() -> Unit! = ok(())\neffect fn main() -> Unit = { println(\"before\"); finish()! }", "before\n", "", 0),
        ("bare_tail_error", "fn finish() -> Unit! = err(\"bare failure\")\neffect fn main() -> Unit = finish()!", "", "Error: bare failure\n", 1),
        ("handled_tail_error", "fn finish() -> Unit! = err(\"handled\")\neffect fn main() -> Unit = { match finish() { ok(_) => (), err(e) => println(e) } }", "handled\n", "", 0),
    ] {
        check_routes(&dir, name, source, stdout, stderr, exit);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fs_streaming_callbacks_preserve_lines_and_first_error() {
    std::thread::Builder::new().stack_size(512 << 20)
        .spawn(fs_streaming_callbacks_on_stack).unwrap().join().unwrap();
}

fn fs_streaming_callbacks_on_stack() {
    let dir = std::env::temp_dir().join(format!("almide-native-streaming-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // These streaming host APIs remain outside the interpreter's admitted
    // host floor. Exercise native certificates, v0 and WASI here; the final
    // three-way audit must retain its explicit interpreter-abstention ledger.
    macro_rules! case {
        ($name:literal, $exit:literal) => {
            check_compiled_routes(&dir, $name,
                include_str!(concat!("../spec/wasm_cross/", $name, ".almd")),
                include_str!(concat!("fixtures/", $name, ".stdout")),
                include_str!(concat!("fixtures/", $name, ".stderr")), $exit);
        };
    }
    case!("fs_fallible_stream_callback", 0);
    case!("fs_fallible_walker_call_head", 1);
    case!("line_endings_bare_cr", 0);
    case!("statement_after_tail_walker_bang", 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn heap_abort_branches_preserve_owned_values_and_error_paths() {
    std::thread::Builder::new().stack_size(512 << 20)
        .spawn(heap_abort_branches_on_stack).unwrap().join().unwrap();
}

fn heap_abort_branches_on_stack() {
    let dir = std::env::temp_dir().join(format!("almide-native-heap-abort-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "heap_abort_branches", include_str!("fixtures/heap_abort_branches.almd"),
        "owned\na:b\nowned\nowned\nowned:tuple\nowned\na:b\n", "", 0);
    for (name, source, stdout, stderr, exit) in [
        ("abort_first", "fn pick(n: Int) -> String = if n > 0 then panic(\"first\") else \"kept\"\nfn main() -> Unit = println(pick(1))", "", "PANIC: first", 1),
        ("abort_last", "fn pick(n: Int) -> String = if n > 0 then \"kept\" else panic(\"last\")\nfn main() -> Unit = println(pick(0))", "", "PANIC: last", 1),
        ("abort_option", "fn pick(s: String?) -> String = match s { some(v) => v, none => panic(\"none\") }\nfn main() -> Unit = println(pick(none))", "", "PANIC: none", 1),
    ] {
        check_routes(&dir, name, source, stdout, stderr, exit);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fs_range_and_chunked_callbacks_keep_partition_and_owned_captures() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(|| {
        let dir = std::env::temp_dir().join(format!("almide-native-ranges-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Keep the interpreter's explicit streaming-host boundary; native,
        // independent certificates, v0 and WASI all exercise these fixtures.
        check_compiled_routes(&dir, "fs_call_head_family",
            include_str!("../spec/wasm_cross/fs_call_head_family.almd"),
            include_str!("fixtures/fs_call_head_family.stdout"), "", 0);
        check_compiled_routes(&dir, "fs_range_chunked_owned",
            include_str!("fixtures/fs_range_chunked_owned.almd"),
            include_str!("fixtures/fs_range_chunked_owned.stdout"), "", 0);
        std::fs::remove_dir_all(dir).unwrap();
    }).unwrap().join().unwrap();
}

#[test]
fn recursive_borrowed_equality_and_typed_error_display_preserve_routes() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(|| {
        let dir = std::env::temp_dir().join(format!("almide-borrowed-eq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        check_routes(&dir, "lambda_failure_channel",
            include_str!("../spec/wasm_cross/lambda_failure_channel.almd"),
            include_str!("fixtures/lambda_failure_channel.stdout"), "", 0);
        check_routes(&dir, "recursive_borrowed_equality",
            include_str!("fixtures/recursive_borrowed_equality.almd"),
            include_str!("fixtures/recursive_borrowed_equality.stdout"), "", 0);
        let rust = std::fs::read_to_string(dir.join("recursive_borrowed_equality.rs")).unwrap();
        let ty = rust.split("fn almd_equal(v0: &").nth(1).unwrap().split(',').next().unwrap();
        let mut deep = rust.replacen("fn main() {", "fn corpus_main() {", 1);
        deep.push_str(&format!(r#"
fn main() {{
    std::thread::Builder::new().stack_size(128 * 1024).spawn(|| {{
        let mut a = {ty}::C0((-0.0_f64).to_bits() as i64);
        let mut b = {ty}::C0(0.0_f64.to_bits() as i64);
        for _ in 0..100_000 {{ a = {ty}::C1(Box::new(a)); b = {ty}::C1(Box::new(b)); }}
        assert_eq!(almd_equal(&a, &b), 1);
        // Drain iteratively: this measures equality, separately from recursive Drop.
        for mut node in [a, b] {{
            loop {{ match node {{ {ty}::C1(next) => node = *next, _ => break }} }}
        }}
    }}).unwrap().join().unwrap();
}}
"#));
        let source = dir.join("deep.rs"); let binary = dir.join("deep");
        std::fs::write(&source, deep).unwrap();
        let build = Command::new("rustc").args(["--edition=2021", "-C", "opt-level=0"])
            .arg(&source).arg("-o").arg(&binary).output().unwrap();
        assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
        let run = Command::new(binary).output().unwrap();
        assert!(run.status.success(), "100000 equality levels on 128KiB: {}", String::from_utf8_lossy(&run.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }).unwrap().join().unwrap();
}

#[test]
fn float32_collection_boundaries_preserve_value_equality_and_compiled_routes() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(|| {
        let dir = std::env::temp_dir().join(format!("almide-float32-collections-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        check_routes(&dir, "sized_display_matrix", include_str!("../spec/wasm_cross/sized_display_matrix.almd"),
            include_str!("fixtures/sized_display_matrix.stdout"), "", 0);
        check_compiled_routes(&dir, "float32_collection_keys", include_str!("fixtures/float32_collection_keys.almd"),
            include_str!("fixtures/float32_collection_keys.stdout"), "", 0);
        // Extra fixture beyond the fixed corpus: the interpreter refuses its
        // Float32 collection core. Keep the abstention explicit, never a match.
        let ir = interp_oracle::lower(include_str!("fixtures/float32_collection_keys.almd")).unwrap();
        let reference = almide_interp::Interpreter::new(&ir).run_main();
        assert!(matches!(reference.status, almide_interp::RunStatus::Unsupported(ref reason)
            if reason.starts_with("prim.handle of a container holding a Float element")));

        std::fs::remove_dir_all(dir).unwrap();
    }).unwrap().join().unwrap();
}


#[test]
fn never_calls_preserve_termination_evaluation_order_and_error_channels() {
    std::thread::Builder::new().stack_size(512 << 20)
        .spawn(never_calls_on_stack).unwrap().join().unwrap();
}

fn never_calls_on_stack() {
    let dir = std::env::temp_dir().join(format!("almide-native-never-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_routes(&dir, "never_fn_value_positions", include_str!("../spec/wasm_cross/never_fn_value_positions.almd"),
        "7\nleft\n", "", 4);
    check_routes(&dir, "never_raw", include_str!("fixtures/never_raw.almd"), "before\n", "PANIC: stopped", 1);
    check_routes(&dir, "never_fallible", include_str!("fixtures/never_fallible.almd"), "before\n", "Error: stopped\n", 1);
    // A non-constant branch keeps both arms in the compiler. Execute every
    // value position with a real abort and check the earlier operand runs once.
    for (name, expr) in [
        ("list", "[left(1), stop(\"list\")]"),
        ("argument", "add(left(1), stop(\"argument\"))"),
        ("operand", "left(1) + stop(\"operand\")"),
        ("field", "{ first: left(1), last: stop(\"field\") }"),
    ] {
        let source = format!(r#"
fn stop(s: String) -> Never = {{ let tag = s + "!"; panic(tag) }}
fn left(n: Int) -> Int = {{ println("left"); n }}
fn add(a: Int, b: Int) -> Int = a + b
fn run(n: Int) -> Unit = {{
 if n > 0 then {{ let value = {expr}; println("unreachable") }} else println("other")
 println("after")
}}
fn main() -> Unit = run(1)
"#);
        check_routes(&dir, name, &source, "left\n", &format!("PANIC: {name}!"), 1);
    }
    std::fs::remove_dir_all(dir).unwrap();
}


#[test]
fn owned_regions_preserve_views_bounds_permissions_and_detached_lifetimes() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(|| {
        let dir = std::env::temp_dir().join(format!("almide-native-regions-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        check_compiled_routes(&dir, "bytes_rawptr", include_str!("../spec/wasm_cross/bytes_rawptr.almd"),
            include_str!("fixtures/bytes_rawptr.stdout"), "", 0);
        check_compiled_routes(&dir, "region_shared_views", include_str!("fixtures/region_shared_views.almd"),
            include_str!("fixtures/region_shared_views.stdout"), "", 0);
        // The pre-codegen interpreter has no ffi bodies. Record its exact
        // existing refusal separately; native/v0/WASI above are real matches.
        for (source, expected) in [
            (include_str!("../spec/wasm_cross/bytes_rawptr.almd"), "ffi.region: no lowered body"),
            (include_str!("fixtures/region_shared_views.almd"), "ffi.alloc: no lowered body"),
        ] {
            let ir = interp_oracle::lower(source).unwrap();
            let reference = almide_interp::Interpreter::new(&ir).run_main();
            assert!(matches!(reference.status, almide_interp::RunStatus::Unsupported(ref reason) if reason == expected),
                "unexpected interpreter verdict: {:?}", reference.status);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }).unwrap().join().unwrap();
}
