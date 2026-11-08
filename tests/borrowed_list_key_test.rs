//! #4597: a List parameter becomes a borrowed slice on the native leg.
//! Exercise both collection APIs, and the runtime's linear/hash/ordered lanes.
use std::process::Command;

const SOURCE: &str = r#"
fn ints(s: Set[List[Int]], m: Map[List[Int], Int], key: List[Int]) -> Unit = {
  println(if set.contains(s, key) then "yes" else "no")
  println(int.to_string(map.get(m, key) ?? -1))
}
fn strings(s: Set[List[String]], key: List[String]) -> Unit =
  println(if set.contains(s, key) then "yes" else "no")
fn main() -> Unit = {
  let keys = [[], [1, 2], [3]]
  let s = set.from_list(keys)
  let m = map.from_list([([], 10), ([1, 2], 20), ([3], 30)])
  ints(s, m, [])
  ints(s, m, [1, 2])
  ints(s, m, [2, 1])
  strings(set.from_list([[], ["a", "bc"], ["ab", "c"]]), ["a", "bc"])
  strings(set.from_list([[], ["a", "bc"]]), ["ab", "c"])
}
"#;

const RUNTIME: &str = r#"
#[test]
fn borrowed_keys_preserve_all_lookup_lanes() {
    fn check<T: Clone + PartialEq + std::fmt::Debug + 'static>(keys: Vec<Vec<T>>, absent: Vec<T>, hashable: bool) {
        for n in [3, 40] {
            let keys = &keys[..n];
            let mut map: AlmideMap<Vec<T>, usize> = keys.iter().cloned().enumerate().map(|(i,k)| (k,i)).collect();
            let mut set = AlmideSet::new();
            for k in keys { set.insert(k.clone()); }
            assert_eq!(matches!(map.lookup, AlmideKeyLookup::Built(_)), hashable && n >= ALMIDE_MAP_INDEX_THRESHOLD);
            assert_eq!(matches!(set.lookup, AlmideKeyLookup::Built(_)), hashable && n >= ALMIDE_MAP_INDEX_THRESHOLD);
            for ordered in [false, true] {
                if ordered {
                    map.to_ordered();
                    set.to_ordered();
                    assert_eq!(matches!(map.lookup, AlmideKeyLookup::Ordered(_)), hashable);
                    assert_eq!(matches!(set.lookup, AlmideKeyLookup::Ordered(_)), hashable);
                }
                for (i, k) in keys.iter().enumerate() {
                    assert_eq!(map.get(k.as_slice()), Some(&i));
                    assert!(map.contains_key(k.as_slice()));
                    assert!(set.contains(k.as_slice()));
                    assert_eq!(k.almide_key_hash(), k.as_slice().almide_key_hash());
                    assert_eq!(k.almide_key_bytes(), k.as_slice().almide_key_bytes());
                }
                assert_eq!(map.get(absent.as_slice()), None);
                assert!(!set.contains(absent.as_slice()));
            }
        }
    }
    let mut ints = vec![Vec::new()];
    ints.extend((1..50i64).map(|i| vec![i, -i]));
    check(ints, vec![1, 1], true);
    let mut strings: Vec<Vec<AlmideString>> = vec![vec![], vec!["a".into(), "bc".into()], vec!["ab".into(), "c".into()]];
    strings.extend((3..50).map(|i| vec![format!("key{i}").into(), "日本語".into()]));
    check(strings, vec!["absent".into()], true);
    // Float list keys deliberately stay unhashable, including an empty key.
    let mut floats = vec![Vec::new()];
    floats.extend((1..50).map(|i| vec![i as f64]));
    check(floats, vec![-1.0], false);
}
"#;

#[test]
fn borrowed_list_keys_build_and_agree_with_wasm() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("keys.almd");
    std::fs::write(&source, SOURCE).unwrap();
    let bin = env!("CARGO_BIN_EXE_almide");
    for (name, args) in [
        ("native", vec![]),
        ("v0", vec!["--no-verified"]),
        ("wasm", vec!["--target", "wasm"]),
    ] {
        let out = if name == "wasm" {
            Command::new(bin)
                .arg("run")
                .arg(&source)
                .args(args)
                .output()
                .unwrap()
        } else {
            let executable = dir.path().join(name);
            let built = Command::new(bin)
                .arg("build")
                .arg(&source)
                .args(args)
                .arg("-o")
                .arg(&executable)
                .env("ALMIDE_NO_VERIFIED_OK", "1")
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&built.stderr)
            );
            Command::new(executable).output().unwrap()
        };
        assert!(
            out.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"yes\n10\nyes\n20\nno\n-1\nyes\nno\n", "{name}");
        assert!(
            out.stderr.is_empty(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let emitted = Command::new(bin).arg("emit").arg(&source).output().unwrap();
    assert!(
        emitted.status.success(),
        "{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    let rust = dir.path().join("keys.rs");
    let emitted = String::from_utf8(emitted.stdout).unwrap();
    let module = "mod almide_runtime {\n";
    assert_eq!(
        emitted.matches(module).count(),
        1,
        "runtime probe needs the isolated runtime module"
    );
    std::fs::write(
        &rust,
        emitted.replacen(module, &format!("{module}{RUNTIME}\n"), 1),
    )
    .unwrap();
    let executable = dir.path().join("runtime-test");
    let built = Command::new("rustc")
        .args(["--edition=2021", "--test", "-C", "opt-level=1"])
        .arg(&rust)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let out = Command::new(executable).output().unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
