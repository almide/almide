// log extern — structured logging to stderr

pub fn almide_rt_log_debug(msg: String) { eprintln!("[DEBUG] {}", msg); }
pub fn almide_rt_log_info(msg: String) { eprintln!("[INFO] {}", msg); }
pub fn almide_rt_log_warn(msg: String) { eprintln!("[WARN] {}", msg); }
pub fn almide_rt_log_error(msg: String) { eprintln!("[ERROR] {}", msg); }

pub fn almide_rt_log_debug_with(msg: String, data: String) { eprintln!("[DEBUG] {} {}", msg, data); }
pub fn almide_rt_log_info_with(msg: String, data: String) { eprintln!("[INFO] {} {}", msg, data); }
pub fn almide_rt_log_warn_with(msg: String, data: String) { eprintln!("[WARN] {} {}", msg, data); }
pub fn almide_rt_log_error_with(msg: String, data: String) { eprintln!("[ERROR] {} {}", msg, data); }
