//! almide-gpu-run — the GPU leg (#1331, #4236).
//!
//! Two halves, kept apart so each can be judged alone:
//!
//! - [`kernel`]: a STRICT emitter from a pure `(Int) -> Int` IR function to a
//!   WGSL compute kernel. It accepts only what it can reproduce bit-exactly on
//!   the device and refuses everything else by name — never an approximation.
//!   WGSL has no 64-bit integer, so `Int` is carried as two `u32` halves and
//!   `+ - *` wrap on two's complement exactly as every other leg does.
//! - [`device`]: a wgpu runner that dispatches one kernel over a list.
//!
//! The parity claim is checked, not asserted: `tests/int_map_parity.rs` runs
//! the same source through the IR interpreter (the 3-way oracle's third leg)
//! and through this leg, and compares the outputs.

pub mod device;
pub mod kernel;
