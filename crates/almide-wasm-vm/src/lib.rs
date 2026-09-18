//! almide-wasm-vm: a wasm interpreter for exactly the instruction set
//! Almide's structural emitter produces (#865). It runs the shipped
//! `to_wasi` artifact of a Critical-profile program — the same bytes a stock
//! runtime runs — with a closed instruction allowlist, a load-time validator
//! that refuses everything outside it, fixed-capacity stacks, no allocation
//! after instantiation, and fuel. The requirements it is built and tested
//! against are REQUIREMENTS.md, one `REQ-VM-N` per clause.

pub mod error;
pub mod exec;
pub mod ir;
pub mod module;
pub mod numeric;
pub mod reader;
pub mod types;
pub mod validate;
pub mod wasi;

use std::io::{Read, Write};

pub use error::{LoadError, Trap};
pub use exec::{Instance, Limits, Outcome};
pub use module::{decode, Module};

/// What one run of the runner produced, besides its output.
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    /// The process exit code the runner uses.
    pub exit: i32,
    /// The trap that ended the run, if one did.
    pub trap: Option<Trap>,
}

/// Load, run and report one program, the way the runner does (REQ-VM-8):
/// the exit code is 0 when `_start` returns, the code `proc_exit` was given,
/// or 1 after a trap — which writes ONE stderr line, `Error: wasm trap:
/// <reason>`, unless the program's own last stderr line already names the
/// abort (`Error: <msg>` followed by `unreachable`, the die convention).
/// This is the embedded host's cross-target contract, so the three
/// observables compare byte-for-byte with it and with native.
pub fn run_program(
    bytes: &[u8],
    limits: Limits,
    input: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Report, LoadError> {
    let module = decode(bytes)?;
    let mut instance = Instance::new(&module, limits)?;
    let mut io = wasi::Io::new(input, out, err);
    let outcome = instance.run(&mut io);
    let _ = io.out.flush();
    Ok(match outcome {
        Outcome::Finished => Report { exit: 0, trap: None },
        Outcome::Exit(code) => Report { exit: code, trap: None },
        Outcome::Trapped(trap) => {
            let named_die = trap == Trap::Unreachable && io.err_tail.last_line_is_error();
            if !named_die {
                let _ = writeln!(io.err, "Error: wasm trap: {trap}");
            }
            let _ = io.err.flush();
            Report { exit: 1, trap: Some(trap) }
        }
    })
}
