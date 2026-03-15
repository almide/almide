// error extern — Rust native implementations

pub fn almide_rt_error_message(e: &str) -> String { e.to_string() }
pub fn almide_rt_error_context(e: &str, ctx: &str) -> String { format!("{}: {}", ctx, e) }
pub fn almide_rt_error_chain(e: &str, cause: &str) -> String { format!("{}\ncaused by: {}", e, cause) }
