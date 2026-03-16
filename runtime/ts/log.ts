const __almd_log = {
  debug(msg: string): void { console.error(`[DEBUG] ${msg}`); },
  info(msg: string): void { console.error(`[INFO] ${msg}`); },
  warn(msg: string): void { console.error(`[WARN] ${msg}`); },
  error(msg: string): void { console.error(`[ERROR] ${msg}`); },
  debug_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[DEBUG] ${msg} ${kv}`); },
  info_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[INFO] ${msg} ${kv}`); },
  warn_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[WARN] ${msg} ${kv}`); },
  error_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[ERROR] ${msg} ${kv}`); },
};
