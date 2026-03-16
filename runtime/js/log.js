const __almd_log = {
  debug(msg) { console.error("[DEBUG] " + msg); },
  info(msg) { console.error("[INFO] " + msg); },
  warn(msg) { console.error("[WARN] " + msg); },
  error(msg) { console.error("[ERROR] " + msg); },
  debug_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[DEBUG] " + msg + " " + kv); },
  info_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[INFO] " + msg + " " + kv); },
  warn_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[WARN] " + msg + " " + kv); },
  error_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[ERROR] " + msg + " " + kv); },
};
