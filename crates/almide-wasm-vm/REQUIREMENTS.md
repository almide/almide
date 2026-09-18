# almide-wasm-vm — requirements

The execution environment for Almide's wasm output that a qualification can
cover (#865). A general-purpose runtime has no realistic qualification path:
it is a JIT with a feature surface far beyond what Almide emits. This crate
narrows the surface instead, the same way the Critical profile narrows the
language. It interprets exactly the instructions and module features that
Almide's shipped artifact can contain, and it refuses everything else before
running anything.

Each requirement names the evidence that holds it. A requirement without
executable evidence is not claimed.

| Id | Requirement | Evidence |
|---|---|---|
| REQ-VM-1 | **Input is the shipped artifact.** The VM runs the bytes `almide build --target wasm` writes (the structural emitter's module after the `to_wasi` transform), unmodified. | `tests/wasm_vm_parity_test.rs` builds every `spec/wasm_cross` fixture with the product CLI and runs the file it wrote. |
| REQ-VM-2 | **Closed set.** The accepted instructions are exactly the ones the emitter and the `to_wasi` shims can write: 127 of them. The accepted module features are also closed. Types return at most one value. Declared types are i32, i64 and f64 only, with f32 existing only transiently on the operand stack. There is at most one funcref table and exactly one unshared 32-bit memory. Globals have constant initializers. Element and data segments are active only, on table and memory 0. Custom sections are skipped. There is no start section, and `_start` takes and returns nothing. Anything else is refused at load, with the offset of the deciding byte. | `tests/allowlist.rs` compares the VM's set with the emitter's source in both directions. `tests/refuse.rs` covers refusals. |
| REQ-VM-3 | **Validated before run.** Each body passes the standard validation algorithm restricted to the closed set, in one pass. The same pass translates it to a flat form whose branches carry their resolved target, operand height and kept value. No instruction of an unvalidated body executes. | `tests/refuse.rs` covers ill-typed bodies, block types and garbage or truncated input, which must be refused without a panic. |
| REQ-VM-4 | **Host surface.** Only the five WASI preview-1 imports are linked, each with its exact signature. `fd_write` is served for stdout and stderr, `fd_read` for stdin, and `proc_exit` ends the run with its code. `random_get` and `clock_time_get` trap and name themselves. They back the Rand and Time capabilities, which a Critical program is never granted. A bad file descriptor returns EBADF and a bad pointer returns EFAULT, with nothing written. | `src/wasi.rs` unit tests. `tests/run.rs` covers `stdin_is_served_and_the_unserved_calls_trap_by_name`. |
| REQ-VM-5 | **Fixed memory.** Every buffer is allocated when the instance is built: linear memory up to its cap, the value stack and the frame stack. Running allocates nothing. A call checks the callee's whole stack need, its locals plus its validated maximum operand height, before it enters. Exceeding either fixed bound traps with `call stack exhausted`. `memory.grow` past the cap returns -1. | `tests/run.rs` covers `recursion_past_the_fixed_depth_traps`, `memory_grows_to_its_cap_and_no_further` and `a_tail_call_reuses_its_frame`, which runs a million tail calls in four frames. |
| REQ-VM-6 | **Fuel.** Every executed instruction costs one unit of fuel, and running out traps. | `tests/run.rs` covers `fuel_bounds_every_run`. |
| REQ-VM-7 | **Traps are the spec's.** Integer division by zero and overflow, out-of-bounds memory, and bad indirect calls all trap as the WebAssembly specification defines. The bad-call cases are a null entry, an index outside the table and a signature mismatch. After a trap, no further instruction executes and no host call is made. | `tests/run.rs` covers the numeric, memory and indirect-call cases. `src/numeric.rs` unit tests cover the edge cases. |
| REQ-VM-8 | **The runner's contract.** A run that returns exits 0. `proc_exit` exits with its code. A trap exits 1 and writes one stderr line, `Error: wasm trap: <reason>`, spelled as the embedded host spells it. The line is omitted when the program's last stderr line already starts with `Error: `. That is the die convention the embedded host follows. A refused module exits 2. | `tests/run.rs` covers the exit-code and trap-line tests. `src/wasi.rs` covers `the_last_line_is_judged_as_str_lines_would`. |
| REQ-VM-9 | **Equivalence with the stock runtime and native.** Every shipped artifact meets one of three outcomes. If its imports are all among the five, it loads and runs to the stock runtime's stdout, stderr and exit code. Alternatively, it stops on a named trap for a clock or entropy call, after output that is a prefix of the stock runtime's. Every other artifact is refused at load. Every fixture that passes `almide check --profile critical`, the profile's deny-all default, runs equal, and it also equals the native binary's run. | `tests/wasm_vm_parity_test.rs`, which is release-only and runs in the commissioned wasm gates job in CI. |
| REQ-VM-10 | **Trusted base.** There are no dependencies beyond the Rust standard library, and `unsafe_code` is forbidden through the workspace lints. Test-only crates, the text assembler for test modules, are not part of the base. | `Cargo.toml` has an empty `[dependencies]` and `[lints] workspace = true`. |

## Scope, and what it leaves out

- **One artifact family.** The VM runs the preview-1 artifacts that the
  structural emitter produces. A component (`--component`) is a different
  format and is out of scope.
- **The Critical profile's default surface.** That means console output,
  exit codes and stdin. An `--allow IO` grant also reaches the file system,
  and file system, environment and argument access all import calls beyond
  the five, so those artifacts are refused at load. Clock and entropy trap by
  name. None of these can be a silent wrong answer. Serving the file system
  would multiply the trusted host surface, which is the opposite of what this
  crate is for.
- **Limits are parameters, not constants.** The defaults are the 32-bit memory
  ceiling, a 4 Mi-cell value stack, 256 Ki frames and unlimited fuel. They are
  set so that every corpus program runs as it does on the stock runtime. A
  deployment states its own bounds with `--fuel`, `--memory-pages`,
  `--stack-cells` and `--call-depth`.
- **Future extraction.** Once this heads toward an actual qualification, it
  moves to its own repository with a frozen release lifecycle. The allowlist
  test is the coupling that has to travel with it. That test is what pins the
  VM's set to the emitter's.
