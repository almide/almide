// Shared host lifecycle state for both native renderers.
pub(crate) static ALMIDE_AT_EXIT: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();
pub(crate) fn almide_at_exit_run() { if let Some(f) = ALMIDE_AT_EXIT.get() { f() } }
pub(crate) struct AlmideContained(pub String);
thread_local! { pub(crate) static ALMIDE_CONTAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(crate) fn almide_containment(active: bool) { ALMIDE_CONTAIN.with(|c| c.set(active)); }
pub(crate) static ALMIDE_REQUEST_HOOKS: std::sync::Mutex<Vec<fn(u8)>> = std::sync::Mutex::new(Vec::new());
pub(crate) fn almide_request_hook(f: fn(u8)) { ALMIDE_REQUEST_HOOKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(f) }
pub(crate) fn almide_request_hooks(stage: u8) { let hs: Vec<fn(u8)> = ALMIDE_REQUEST_HOOKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone(); for f in hs { f(stage) } }
pub(crate) fn almide_contained_unwind(text: impl FnOnce() -> String) { if ALMIDE_CONTAIN.with(|c| c.get()) { std::panic::resume_unwind(Box::new(AlmideContained(text()))) } }
