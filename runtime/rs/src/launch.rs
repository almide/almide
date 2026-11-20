// launch — the PATH the program was launched with (#4844, C-459). The text
// is rt-core's launch_core.rs, inlined here ONCE: process.rs (through
// process_core) and env.rs call `almide_rt_launch_*`, and RUNTIME_DEPS pulls
// this module in beside either of them by that prefix.
include!("../../../crates/almide-rt-core/src/launch_core.rs");
