//! Preserve the checked entry declaration through native rendering.
//! Plain and effect mains have different Result handling in the standard route.
use std::cell::Cell;
thread_local! {
    static EFFECT_MAIN: Cell<Option<bool>> = const { Cell::new(None) };
}
pub(crate) struct EntryGuard(Option<bool>);
impl EntryGuard {
    pub(crate) fn set(is_effect: bool) -> Self {
        Self(EFFECT_MAIN.with(|m| m.replace(Some(is_effect))))
    }
}
impl Drop for EntryGuard {
    fn drop(&mut self) { EFFECT_MAIN.with(|m| m.set(self.0)); }
}
pub(crate) fn is_effect_main() -> Option<bool> { EFFECT_MAIN.with(Cell::get) }
