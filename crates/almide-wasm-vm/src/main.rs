//! `almide-wasm-vm [--fuel N] [--memory-pages N] [--stack-cells N]
//! [--call-depth N] <program.wasm>` — run a shipped artifact. Exit codes:
//! the program's own (0, its `proc_exit` code, or 1 after a trap), or 2
//! when the module is refused or the command line is wrong.

use std::io::Write;
use std::process::ExitCode;

use almide_wasm_vm::{run_program, Limits};

const USAGE: &str = "usage: almide-wasm-vm [--fuel N] [--memory-pages N] [--stack-cells N] [--call-depth N] <program.wasm>";

fn parse(args: &[String]) -> Result<(Limits, String), String> {
    let mut limits = Limits::default();
    let mut path = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| -> Result<u64, String> {
            let v = it.next().ok_or(format!("{name} needs a value"))?;
            v.parse().map_err(|_| format!("{name}: `{v}` is not a number"))
        };
        let narrow = |name: &str, v: u64| u32::try_from(v).map_err(|_| format!("{name}: {v} is too large"));
        match a.as_str() {
            "--fuel" => limits.fuel = value("--fuel")?,
            "--memory-pages" => limits.memory_pages = narrow("--memory-pages", value("--memory-pages")?)?,
            "--stack-cells" => limits.stack_cells = narrow("--stack-cells", value("--stack-cells")?)?,
            "--call-depth" => limits.call_depth = narrow("--call-depth", value("--call-depth")?)?,
            _ if a.starts_with("--") => return Err(format!("unknown option `{a}`")),
            _ if path.is_none() => path = Some(a.clone()),
            _ => return Err("more than one program given".to_string()),
        }
    }
    path.map(|p| (limits, p)).ok_or_else(|| "no program given".to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (limits, path) = match parse(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: cannot read `{path}`: {e}");
            return ExitCode::from(2);
        }
    };
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    let mut err = std::io::stderr().lock();
    let mut input = std::io::stdin().lock();
    match run_program(&bytes, limits, &mut input, &mut out, &mut err) {
        Ok(code) => {
            let _ = out.flush();
            drop(out);
            std::process::exit(code)
        }
        Err(e) => {
            let _ = writeln!(err, "Error: {e}");
            ExitCode::from(2)
        }
    }
}
