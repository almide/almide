//! #2327: Never tails execute effects without manufacturing a return value.
//! Cross exit-code boundaries with expression positions; assert the actual
//! answer as well as agreement so a shared wrong exit cannot pass.
use std::process::Command;

#[test]
fn dynamic_exit_codes_execute_in_every_tail_position() {
    let bin =
        std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let dir = tempfile::tempdir().unwrap();
    let positions = [
        ("tail", "process.exit(code)"),
        ("block", "{ process.exit(code) }"),
        (
            "if",
            "if code >= 0 then process.exit(code) else process.exit(code)",
        ),
        (
            "match",
            "match code { 0 => process.exit(code), _ => process.exit(code) }",
        ),
        (
            "guard",
            "{ guard false else process.exit(code)\n println(\"unreachable\") }",
        ),
        (
            "statement",
            "{ process.exit(code)\n println(\"unreachable\") }",
        ),
    ];
    for (position, tail) in positions {
        for inline in [false, true] {
            let tail = if inline {
                tail.replace("process.exit(code)", "process.exit(chosen(code)!)")
            } else {
                tail.to_string()
            };
            for code in [-1, 0, 1, 125, 126, 255, 256] {
                let file = dir.path().join(format!("{position}_{code}_{inline}.almd"));
                let binding = if inline {
                    code.to_string()
                } else {
                    format!("chosen({code})!")
                };
                std::fs::write(
                    &file,
                    format!(
                        "import process\n\
                 effect fn chosen(k: Int) -> Int = {{ println(\"chosen\")\n k }}\n\
                 effect fn main() -> Unit = {{\n\
                   println(\"before-exit\")\n\
                   let code = {binding}\n\
                   {tail}\n\
                 }}\n"
                    ),
                )
                .unwrap();
                for leg in ["native", "wasm"] {
                    let mut cmd = Command::new(&bin);
                    cmd.arg("run").arg(&file);
                    if leg != "native" {
                        cmd.args(["--target", "wasm"]);
                    }
                    let out = cmd.output().unwrap();
                    let stderr = String::from_utf8(out.stderr).unwrap().lines().collect::<Vec<_>>().join("\n");
                    let context =
                        format!("{position}, code {code}, inline {inline}, {leg}: {stderr}");
                    // C-350: the exit status 0..=255 passes through.
                    let (want_code, want_err) = if !(0..=255).contains(&code) {
                        (1, "Error: exit code must be in 0..=255")
                    } else {
                        (code, "")
                    };
                    assert_eq!(out.status.code(), Some(want_code), "{context}");
                    assert_eq!(
                        String::from_utf8(out.stdout).unwrap(),
                        "before-exit\nchosen\n",
                        "{context}"
                    );
                    assert_eq!(stderr, want_err, "{context}");
                }
            }
        }
    }
}
