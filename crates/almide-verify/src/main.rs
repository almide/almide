//! The `almide-verify` command line.
//!
//!   almide-verify <property> <witness-file>   one witness — the `proofs/checker`
//!                                             interface: prints ACCEPT / REJECT
//!   almide-verify bundle <bundle-file>        every witness of a certificate bundle
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

exit: 0 accept, 1 reject, 3 incomplete bundle, 2 usage or unreadable input";

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
    let (verdicts, outcome) = bundle::verify(&parsed);
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
