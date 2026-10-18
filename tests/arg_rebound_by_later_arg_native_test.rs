//! #3801 — native: a call argument that rebinds a variable an EARLIER
//! argument reads.
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
//! The fold cells are also in the cross-target fixture
//! `spec/wasm_cross/fold_init_rebinds_source.almd`; this test pins the
//! native leg for the remaining cells.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run_native(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almd_arg_rebound_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide())
        .current_dir(&dir)
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("almide run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let errors: Vec<&str> = stderr.lines().filter(|l| l.starts_with("error")).collect();
    assert!(out.status.success(), "{name}: native run failed:\n{}\n--- program ---\n{src}", errors.join("\n"));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
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
        assert_eq!(run_native(name, &main_with(body)), want, "{name}");
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
        assert_eq!(run_native(name, &main_with(body)), want, "{name}");
    }
    let user = "fn cnt(xs: List[String], n: Int) -> Int = list.len(xs) + n\n\neffect fn main() -> Unit = {\n  var u = [\"a\", \"b\"]\n  let r = cnt(u, {\n    u = [\"z\"]\n    0\n  })\n  println(\"${r} ${list.len(u)}\")\n}\n";
    assert_eq!(run_native("user_fn", user), "2 1", "user fn");
}

#[test]
fn a_fold_lines_init_that_rebinds_the_path_reads_the_old_path() {
    let src = "import fs\n\neffect fn main() -> Unit = {\n  fs.write(\"in.txt\", \"a\\nb\\n\")!\n  var p = \"in.txt\"\n  let r = fs.fold_lines(p, {\n    p = \"missing.txt\"\n    \"\"\n  }, (a, l) => a + l)!\n  println(\"${r} ${p}\")\n}\n";
    assert_eq!(run_native("fold_lines", src), "ab missing.txt");
}
