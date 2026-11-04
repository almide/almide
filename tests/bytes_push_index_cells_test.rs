//! The Bytes push and index cells (#4006): `bytes.__append` (the functional
//! push a `bytes.push(buf, x)` statement lowers to) and `bytes.__index` (the
//! checked `b[i]`) in stdlib/bytes.almd, each an `@intrinsic` over
//! runtime/rs/src/bytes.rs and each the generic the self-hosted `bytes.append`
//! / `bytes.index` body specializes, so the certified native render serves them
//! from the declared floor.
//!
//! The index write `b[i] = x` lowers to `bytes.__set_u8`, already declared
//! over its runtime item; its self-hosted body is registered under that name
//! itself, and the native floor serves it by writing the caller's own buffer
//! in place (`mut b`, `&mut Vec<u8>`).
//!
//! A `mut b: Bytes` fn the move-mode rewrite calls for its result
//! (`b = bytes.append_u16_le(b, x)`) edits its own copy of the arg — the arg
//! itself when it dies at the call — on the same `&mut Vec<u8>` floor entry.
//!
//! The family is the three shapes the MIR lowering produces for a Bytes
//! receiver: a functional append, a checked read and an in-place write. Every
//! other Bytes call has a public declaration of its own.

const CELLS: &[(&str, &str, &str, &str, bool)] = &[
    // (cell, its runtime item, the self-hosted body, its file, whether that
    //  body specializes the cell rather than being registered under its name)
    ("__append", "almide_rt_bytes_append_fresh", "bytes_append", "stdlib/bytes_core.almd", true),
    ("__index", "almide_rt_bytes_index_checked", "bytes_index", "stdlib/bytes_core.almd", true),
    ("__set_u8", "almide_rt_bytes_set_u8", "bytes_set_u8", "stdlib/bytes_mutate.almd", false),
];

#[test]
fn every_cell_is_declared_specialized_and_on_the_native_floor() {
    let decls = std::fs::read_to_string("stdlib/bytes.almd").unwrap();
    let rt = std::fs::read_to_string("runtime/rs/src/bytes.rs").unwrap();
    for (cell, symbol, body, file, specializes) in CELLS {
        // The declaration's attributes (`@reuse(b)` among them) sit between.
        let declared = decls
            .split(&format!("@intrinsic(\"{symbol}\")\n"))
            .nth(1)
            .and_then(|rest| rest.lines().find(|l| !l.starts_with('@')))
            .is_some_and(|l| l.starts_with(&format!("fn {cell}(")));
        assert!(declared, "bytes.{cell} is not declared over `{symbol}`");
        assert!(rt.contains(&format!("pub fn {symbol}(")), "the runtime has no `{symbol}`");
        let src = std::fs::read_to_string(file).unwrap();
        let head = if *specializes { format!("@specializes(\"bytes.{cell}\")\nfn {body}(") } else { format!("\nfn {body}(") };
        assert!(src.contains(&head), "`{body}` is not the self-hosted body of bytes.{cell}");
        let call = if *specializes { format!("bytes.{}", body.trim_start_matches("bytes_")) } else { format!("bytes.{cell}") };
        for c in [call, format!("bytes.{cell}")] {
            assert_eq!(almide_mir::native_floor::refusal(&c), None, "`{c}` is not on the native floor");
        }
    }
}

const PROGRAM: &str = r#"fn main() -> Unit = {
  var b = bytes.from_list([1, 2])
  bytes.push(b, 300)
  bytes.push(b, -1)
  println("${bytes.to_list(b)} ${b[0]} ${b[3]}")
  let c = bytes.__append(b, 7)
  println("${bytes.to_list(c)} ${bytes.__index(c, 4)} ${bytes.len(b)}")
  b[1] = 300
  bytes.__set_u8(b, 0, 9)
  bytes.__set_u8(b, 9, 9)
  println("${bytes.to_list(b)}")
  bytes.append_u16_le(b, 258)
  println("${bytes.to_list(b)}")
  println("${b[6]}")
}
"#;

/// What v0, the certified native render and wasm print (measured on all three):
/// the last read is past the end and aborts.
const EXPECTED_OUT: &str = "[1, 2, 44, 255] 1 255\n[1, 2, 44, 255, 7] 7 4\n[9, 44, 44, 255]\n[9, 44, 44, 255, 2, 1]\n";

fn run(args: &[&str]) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!("bytes_cells_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("cells.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_almide")).args(args).arg(&src).output().unwrap()
}

#[test]
fn both_backends_print_every_cell_identically_and_abort_past_the_end() {
    for (leg, out) in [("native", run(&["run"])), ("wasm", run(&["run", "--target", "wasm"]))] {
        if leg == "wasm" && String::from_utf8_lossy(&out.stderr).contains("wasmtime") && out.stdout.is_empty() {
            continue; // no wasm runner on this host
        }
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED_OUT, "{leg}");
        assert_eq!(out.status.code(), Some(1), "{leg}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("index out of bounds"), "{leg}: {}", String::from_utf8_lossy(&out.stderr));
    }
}
