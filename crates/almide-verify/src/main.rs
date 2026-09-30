//! The `almide-verify` command line.
//!
//!   almide-verify <property> <witness-file>   one witness — the `proofs/checker`
//!                                             interface: prints ACCEPT / REJECT
//!   almide-verify bundle <bundle-file>        every witness of a certificate bundle, and
//!                                             (version 2) the SHA-256 of the artifact it names
//!   almide-verify --version | --help
//!
//! Exit codes: 0 accept / certified, 1 reject (a bundle with no witness is a
//! reject too), 3 a bundle whose witnesses all accept but which declares
//! uncertified functions, 2 usage error or an input that cannot be read.

use almide_verify::{bundle, check, Property, FORMATS, VERSION};
use std::process::ExitCode;

const USAGE: &str = "\
usage: almide-verify <property> <witness-file>
       almide-verify bundle <bundle-file>
       almide-verify --version

properties: ownership | names | caps | caps-transitive | call-modes

exit: 0 accept, 1 reject (a bundle whose artifact hash differs too), 3 incomplete bundle,
      2 usage or unreadable input";

fn read(path: &str) -> Result<Vec<u8>, ExitCode> {
    std::fs::read(path).map_err(|e| {
        eprintln!("almide-verify: cannot read {path}: {e}");
        ExitCode::from(2)
    })
}

fn verify_one(property: &str, path: &str) -> ExitCode {
    let Some(property) = Property::from_name(property) else {
        eprintln!("almide-verify: unknown property `{property}`\n\n{USAGE}");
        return ExitCode::from(2);
    };
    match read(path) {
        Ok(bytes) if check(property, &bytes) => {
            println!("ACCEPT");
            ExitCode::SUCCESS
        }
        Ok(_) => {
            println!("REJECT");
            ExitCode::from(1)
        }
        Err(code) => code,
    }
}

fn verify_bundle(path: &str) -> ExitCode {
    let bytes = match read(path) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let parsed = match bundle::parse(&bytes) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("almide-verify: {path}: {e}");
            return ExitCode::from(2);
        }
    };
    for (key, value) in &parsed.metadata {
        println!("{key}: {value} (producer claim, not checked)");
    }
    let artifact_ok = match &parsed.artifact {
        None => true,
        Some(a) => match check_artifact(path, a) {
            Ok(ok) => ok,
            Err(code) => return code,
        },
    };
    let (verdicts, outcome) = bundle::verify(&parsed);
    // A bundle whose artifact does not hash to the recorded digest certifies
    // bytes that are not the file in hand: a reject, whatever the witnesses say.
    let outcome = if artifact_ok { outcome } else { bundle::Outcome::Rejected };
    for v in &verdicts {
        let word = if v.accepted { "ACCEPT" } else { "REJECT" };
        println!("{word}  {:<15}  {}", v.property.name(), v.function);
    }
    for (function, reason) in &parsed.uncertified {
        println!("UNCERTIFIED  {function}: {reason}");
    }
    let accepted = verdicts.iter().filter(|v| v.accepted).count();
    println!(
        "almide-verify {VERSION}: {} witness(es), {accepted} accepted, {} rejected, {} uncertified -> {}",
        verdicts.len(),
        verdicts.len() - accepted,
        parsed.uncertified.len(),
        outcome.label()
    );
    ExitCode::from(outcome.exit_code() as u8)
}

/// Recompute the SHA-256 of the file a version-2 bundle names (relative to
/// the bundle's directory unless absolute) and report it; `Ok(false)` on a
/// mismatch, `Err` (exit 2) when the file cannot be read.
fn check_artifact(bundle_path: &str, a: &bundle::Artifact) -> Result<bool, ExitCode> {
    let p = std::path::Path::new(&a.path);
    let file = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::path::Path::new(bundle_path).parent().unwrap_or(std::path::Path::new("")).join(p)
    };
    let bytes = read(&file.display().to_string())?;
    let got = almide_verify::sha256::hex(&bytes);
    if got == a.sha256 {
        println!("ARTIFACT  sha256 {got}  {}: MATCH", a.path);
        Ok(true)
    } else {
        println!("ARTIFACT  sha256 {got}  {}: MISMATCH (the bundle names {})", a.path, a.sha256);
        Ok(false)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--version"] | ["-V"] => {
            println!("almide-verify {VERSION} (formats: {FORMATS})");
            ExitCode::SUCCESS
        }
        ["--help"] | ["-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        ["bundle", path] => verify_bundle(path),
        [property, path] => verify_one(property, path),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
