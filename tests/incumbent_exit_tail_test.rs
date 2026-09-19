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
            for code in [-1, 0, 1, 125, 126, 256] {
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
                for leg in ["native", "structural", "incumbent"] {
                    let mut cmd = Command::new(&bin);
                    cmd.arg("run")
                        .arg(&file)
                        .env_remove("ALMIDE_WASM_INCUMBENT");
                    if leg != "native" {
                        cmd.args(["--target", "wasm"]);
                    }
                    if leg == "incumbent" {
                        cmd.env("ALMIDE_WASM_INCUMBENT", "1");
                    }
                    let out = cmd.output().unwrap();
                    let stderr = String::from_utf8(out.stderr).unwrap();
                    // The forced-leg notice is compiler routing information.
                    let stderr = stderr
                        .lines()
                        .filter(|line| !line.starts_with("[almide] ALMIDE_WASM_INCUMBENT is set:"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let context =
                        format!("{position}, code {code}, inline {inline}, {leg}: {stderr}");
                    let valid = (0..=125).contains(&code);
                    assert_eq!(
                        out.status.code(),
                        Some(if valid { code } else { 1 }),
                        "{context}"
                    );
                    assert_eq!(
                        String::from_utf8(out.stdout).unwrap(),
                        "before-exit\nchosen\n",
                        "{context}"
                    );
                    assert_eq!(
                        stderr,
                        if valid {
                            ""
                        } else {
                            "Error: exit code must be in 0..=125"
                        },
                        "{context}"
                    );
                }
            }
        }
    }
}
