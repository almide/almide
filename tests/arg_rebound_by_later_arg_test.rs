//! #3801 / #3815 — a call argument that rebinds a variable an EARLIER
//! argument reads, on both legs.
//!
//! Arguments evaluate left to right, once, by value (ALS-E26, C-258), so
//! `st |> set.fold({ st = …; 0 }, f)` folds the set `st` held before the
//! init ran. The native leg passed the earlier argument as a borrow of the
//! variable (`almide_rt_set_fold(&st, { st = …; 0 }, f)`), and the borrow was
//! still live when the later argument assigned the variable: rustc E0506,
//! and the program did not build. The fold family (`set.fold`, `map.fold`,
//! `fs.fold_lines`) is the shape the issue names; any call whose earlier
//! argument is taken by reference had the same failure, so the cells cover
//! runtime calls over every container kind and a user fn whose param is
//! inferred borrowed.
//!
//! #3815 — the wasm leg passed such an earlier argument as a view of the
//! variable's block, and the later argument's assignment released it: the
//! non-fold calls read freed memory (`list.contains` printed `false`,
//! `map.get` `none`). Every cell now runs on BOTH legs and must print the
//! same bytes. The fold cells are also in the cross-target fixture
//! `spec/wasm_cross/fold_init_rebinds_source.almd`, the non-fold family in
//! `spec/wasm_cross/arg_rebinds_earlier_arg.almd`.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run_leg(name: &str, src: &str, wasm: bool) -> String {
    let leg = if wasm { "wasm" } else { "native" };
    let dir = std::env::temp_dir().join(format!("almd_arg_rebound_{name}_{leg}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new(almide());
    cmd.current_dir(&dir).args(["run", file.to_str().unwrap()]);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().expect("almide run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let errors: Vec<&str> = stderr.lines().filter(|l| l.starts_with("error") || l.contains("trap")).collect();
    assert!(out.status.success(), "{name}: {leg} run failed:\n{}\n--- program ---\n{src}", errors.join("\n"));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run on both legs; they must print the same bytes, and that is the answer.
fn run_both(name: &str, src: &str) -> String {
    let native = run_leg(name, src, false);
    let wasm = run_leg(name, src, true);
    assert_eq!(native, wasm, "{name}: native and wasm disagree");
    native
}

fn main_with(body: &str) -> String {
    format!("effect fn main() -> Unit = {{\n{body}\n}}\n")
}

#[test]
fn a_fold_init_that_rebinds_the_source_folds_the_old_value() {
    let cases = [
        ("set_fold", "  var st = set.from_list([1, 2, 3])\n  let r = st |> set.fold({\n    st = set.from_list([0])\n    0\n  }, (a, x) => a + x)\n  println(\"${r} ${set.len(st)}\")", "6 1"),
        ("map_fold", "  var m = [\"a\": 1, \"b\": 2]\n  let r = m |> map.fold({\n    m = [\"z\": 9]\n    0\n  }, (a, k, v) => a + v)\n  println(\"${r} ${map.len(m)}\")", "3 1"),
        ("set_fold_read_first", "  var st = set.from_list([\"a\", \"b\"])\n  let r = st |> set.fold({\n    let n = set.len(st)\n    st = set.from_list([\"z\"])\n    \"${n}:\"\n  }, (a, x) => a + x)\n  println(\"${r} ${set.len(st)}\")", "2:ab 1"),
        ("map_fold_read_first", "  var m = [\"a\": \"1\"]\n  let r = m |> map.fold({\n    let n = map.len(m)\n    m = [\"z\": \"9\", \"y\": \"8\"]\n    \"${n}:\"\n  }, (a, k, v) => a + k + v)\n  println(\"${r} ${map.len(m)}\")", "1:a1 2"),
    ];
    for (name, body, want) in cases {
        assert_eq!(run_both(name, &main_with(body)), want, "{name}");
    }
}

#[test]
fn an_argument_that_rebinds_an_earlier_borrowed_argument_sees_the_old_value() {
    let cases = [
        ("list_contains", "  var a = [\"a\", \"b\"]\n  let r = list.contains(a, {\n    a = [\"z\"]\n    \"a\"\n  })\n  println(\"${r} ${list.len(a)}\")", "true 1"),
        ("set_contains", "  var s = set.from_list([1, 2])\n  let r = set.contains(s, {\n    s = set.from_list([0])\n    1\n  })\n  println(\"${r} ${set.len(s)}\")", "true 1"),
        ("map_get", "  var m = [\"a\": 1]\n  let r = map.get(m, {\n    m = [\"z\": 9]\n    \"a\"\n  })\n  println(\"${r} ${map.len(m)}\")", "some(1) 1"),
        ("string_contains", "  var t = \"abc\"\n  let r = string.contains(t, {\n    t = \"z\"\n    \"a\"\n  })\n  println(\"${r} ${t}\")", "true z"),
    ];
    for (name, body, want) in cases {
        assert_eq!(run_both(name, &main_with(body)), want, "{name}");
    }
    let user = "fn cnt(xs: List[String], n: Int) -> Int = list.len(xs) + n\n\neffect fn main() -> Unit = {\n  var u = [\"a\", \"b\"]\n  let r = cnt(u, {\n    u = [\"z\"]\n    0\n  })\n  println(\"${r} ${list.len(u)}\")\n}\n";
    assert_eq!(run_both("user_fn", user), "2 1", "user fn");
    // A scalar read is a value too: an inlined small fn read `k` after the
    // block ran on wasm (`71`).
    let scalar = "fn add(a: Int, b: Int) -> Int = a * 10 + b\n\neffect fn main() -> Unit = {\n  var k = 2\n  let r = add(k, {\n    k = 7\n    1\n  })\n  println(\"${r} ${k}\")\n}\n";
    assert_eq!(run_both("user_fn_scalar", scalar), "21 7", "user fn scalar");
}

/// A read BETWEEN two rebinding arguments sees the first rebind and not the
/// second: binding it ahead of the call must not move it ahead of the
/// argument before it (native read the original value, `33`).
#[test]
fn a_read_between_two_rebinding_arguments_sees_the_first_rebind_only() {
    let src = "fn mid(n: Int, xs: List[String], m: Int) -> Int = list.len(xs) * 10 + n + m\n\neffect fn main() -> Unit = {\n  var w = [\"a\", \"b\", \"c\"]\n  let r = mid({\n    w = [\"y\", \"y\"]\n    1\n  }, w, {\n    w = [\"z\"]\n    2\n  })\n  println(\"${r} ${list.len(w)}\")\n}\n";
    assert_eq!(run_both("read_between", src), "23 1");
}

/// An index is an operand sequence too: the container, then the index.
#[test]
fn an_index_that_rebinds_its_container_indexes_the_old_value() {
    let cases = [
        ("list_index", "  var xs = [\"a\", \"b\"]\n  let r = xs[{\n    xs = [\"z\"]\n    1\n  }]\n  println(\"${r} ${list.len(xs)}\")", "b 1"),
        ("map_index", "  var m = [\"a\": 1, \"b\": 2]\n  let r = m[{\n    m = [\"z\": 9]\n    \"b\"\n  }]\n  println(\"${r} ${map.len(m)}\")", "some(2) 1"),
    ];
    for (name, body, want) in cases {
        assert_eq!(run_both(name, &main_with(body)), want, "{name}");
    }
}

/// The shapes the corpus fixture leaves out because the native v1 lowering
/// walls them (the map index above is one): a list `+`, a scalar bound of
/// `list.slice`, a tuple.
#[test]
fn the_v1_walled_operand_shapes_see_the_old_value() {
    let cases = [
        ("list_concat", "  var xs = [\"a\", \"b\"]\n  let r = xs + {\n    xs = [\"z\"]\n    [\"c\"]\n  }\n  println(\"${r} ${list.len(xs)}\")", "[\"a\", \"b\", \"c\"] 1"),
        ("slice_scalar", "  var k = 1\n  let r = list.slice([1, 2, 3, 4], k, {\n    k = 0\n    3\n  })\n  println(\"${r} ${k}\")", "[2, 3] 0"),
        ("tuple", "  var q = [\"a\", \"b\"]\n  let r = (q, {\n    q = [\"z\"]\n    1\n  })\n  println(\"${r} ${list.len(q)}\")", "([\"a\", \"b\"], 1) 1"),
    ];
    for (name, body, want) in cases {
        assert_eq!(run_both(name, &main_with(body)), want, "{name}");
    }
}

#[test]
fn a_fold_lines_init_that_rebinds_the_path_reads_the_old_path() {
    let src = "import fs\n\neffect fn main() -> Unit = {\n  fs.write(\"in.txt\", \"a\\nb\\n\")!\n  var p = \"in.txt\"\n  let r = fs.fold_lines(p, {\n    p = \"missing.txt\"\n    \"\"\n  }, (a, l) => a + l)!\n  println(\"${r} ${p}\")\n}\n";
    assert_eq!(run_both("fold_lines", src), "ab missing.txt");
}
