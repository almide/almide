//! The codegen runtime's half of the native stack guard (C-196): the module
//! itself is `almide_base::native_stack_guard::NATIVE_STACK_GUARD` (the doc
//! there says how it works); this adds the entry points the generated `main`
//! and the runtime's worker entries call, and the stdout hook. The stdout
//! already written stays written: the buffer is line-flushed, and the hook
//! writes out a partial line still in the faulting thread's buffer when the
//! buffer exists and is not borrowed (a fault inside a write leaves it
//! borrowed, and that partial line is dropped). A fault on a `fan` worker
//! exits without the ADR-0024 D6 trap wait — the wait blocks on the group's
//! lock, which a handler may not take — so a lower element's output that had
//! not reached the fds yet is not printed (declared in C-196).

const STACK_GLUE: &str = r#"
VIS fn almide_stack_guard_install() {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    almide_stack_guard::install();
}
VIS fn almide_stack_guard_register() {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    almide_stack_guard::register();
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn almide_stack_guard_stdout() {
    if ALMIDE_STDOUT_LIVE.with(|c| c.get()) {
        let _ = ALMIDE_STDOUT_BUF.try_with(|b| {
            if let Ok(w) = b.try_borrow_mut() {
                almide_stack_guard::raw_write(1, w.buffer());
            }
        });
    }
}
"#;

/// The stack-guard prelude as emitted into a program (`vis` is `pub ` for the
/// `almide_rt` rlib, empty for an inline main).
pub(crate) fn stack_guard_prelude(vis: &str) -> String {
    let mut s = String::from(almide_base::native_stack_guard::NATIVE_STACK_GUARD);
    s.push_str(&STACK_GLUE.replace("VIS ", vis));
    s
}
