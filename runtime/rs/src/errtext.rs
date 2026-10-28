// errtext — C-418's escaper for user data in runtime error texts (#4126).
//
// The text is almide-base's err_text.rs, inlined here ONCE: fs.rs, process.rs
// and net.rs call `almide_rt_errtext_quote` / `almide_rt_errtext_escape`, and
// RUNTIME_DEPS pulls this module in beside any of them by that prefix. A copy
// in each caller would collide in the one generated crate (E0428, the
// native_runtime_name_collision_pin_test class).
include!("../../../crates/almide-base/src/err_text.rs");
