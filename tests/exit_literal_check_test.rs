//! #2328: the checker rejects literal exit codes outside the portable domain.
use std::process::Command;

#[test]
fn literal_exit_codes_follow_the_portable_domain() {
    let bin =
        std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("exit.almd");
    let forms = [
        ("import process", "process.exit(VALUE)"),
        ("import process as p", "p.exit(VALUE)"),
        ("import process.{exit}", "exit(VALUE)"),
        ("import process", "VALUE |> process.exit"),
        ("import process", "VALUE |> process.exit()"),
    ];
    let literals = [
        ("0", true),
        ("1", true),
        ("125", true),
        ("-0", true),
        ("-(-125)", true),
        ("0x7d", true),
        ("0b1111101", true),
        ("0o175", true),
        ("1_25", true),
        ("-1", false),
        ("126", false),
        ("200", false),
        ("256", false),
        ("0x7e", false),
        ("0b1111110", false),
        ("0o176", false),
        ("1_26", false),
        ("(200)", false),
        ("-(1)", false),
        ("-(-126)", false),
    ];
    for (imports, call) in forms {
        for (literal, accepted) in literals {
            let program = format!(
                "{imports}\neffect fn main() -> Unit = {}\n",
                call.replace("VALUE", literal)
            );
            std::fs::write(&source, &program).unwrap();
            let output = Command::new(&bin)
                .arg("check")
                .arg(&source)
                .output()
                .unwrap();
            let diagnostics = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.success(),
                accepted,
                "{program}\n{diagnostics}"
            );
            if !accepted {
                assert!(diagnostics.contains("E084"), "{program}\n{diagnostics}");
                assert!(diagnostics.contains("0..=125"), "{diagnostics}");
            }
        }
    }
}

#[test]
fn computed_codes_and_local_functions_remain_checkable() {
    let bin =
        std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("computed.almd");
    for program in [
        "import process\neffect fn main() -> Unit = { let code = 200\n process.exit(code) }",
        "import process\neffect fn main() -> Unit = process.exit(100 + 100)",
        "fn exit(code: Int) -> Int = code\nfn main() -> Unit = { let _ = exit(200) }",
        "import process.{exit}\nfn main() -> Unit = { let exit = (code: Int) => code\n let _ = exit(200) }",
    ] {
        std::fs::write(&source, program).unwrap();
        let output = Command::new(&bin)
            .arg("check")
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{program}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn a_user_module_named_process_has_no_exit_domain_restriction() {
    let bin =
        std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::write(
        root.join("almide.toml"),
        "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    )
    .unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/process.almd"),
        "fn exit(code: Int) -> Int = code\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.almd"),
        "import self.process\nfn main() -> Unit = { let _ = process.exit(200) }\n",
    )
    .unwrap();
    let output = Command::new(&bin)
        .current_dir(root)
        .args(["check", "src/main.almd"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_diagnostic_points_to_the_argument_without_choosing_a_replacement() {
    let bin =
        std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("span.almd");
    let line = "effect fn main() -> Unit = process.exit(200)";
    std::fs::write(&source, format!("import process\n{line}\n")).unwrap();
    let output = Command::new(&bin)
        .args(["check", "--json"])
        .arg(&source)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let diagnostic = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["code"] == "E084")
        .expect("E084 JSON diagnostic");
    assert_eq!(diagnostic["line"], 2);
    assert_eq!(diagnostic["col"], line.find("200").unwrap() + 1);
    assert_eq!(diagnostic["end_col"], line.find("200").unwrap() + 4);
    assert!(diagnostic["try_replace"].is_null());
}
