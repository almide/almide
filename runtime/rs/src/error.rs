// error extern — Rust native implementations

pub fn almide_rt_error_message<T>(r: &Result<T, String>) -> AlmideString { let __almide_out: String = match r { Ok(_) => String::new(), Err(e) => e.clone() }; __almide_out.into() }
pub fn almide_rt_error_context<T: Clone>(r: &Result<T, String>, ctx: &str) -> Result<T, String> {
    r.clone().map_err(|e| format!("{}: {}", ctx, e))
}
pub fn almide_rt_error_chain(outer: &str, cause: &str) -> AlmideString { let __almide_out: String = format!("{}\ncaused by: {}", outer, cause); __almide_out.into() }
