// error extern — Rust native implementations

pub fn almide_rt_error_message<T>(r: &Result<T, AlmideString>) -> AlmideString { let __almide_out: String = match r { Ok(_) => String::new(), Err(e) => e.clone() }; __almide_out.into() }
pub fn almide_rt_error_context<T: Clone>(r: &Result<T, AlmideString>, ctx: &str) -> Result<T, AlmideString> { let __almide_r: Result<T, String> = (|| { r.clone().map_err(|e| format!("{}: {}", ctx, e)) })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_error_chain(outer: &str, cause: &str) -> AlmideString { let __almide_out: String = format!("{}\ncaused by: {}", outer, cause); __almide_out.into() }
