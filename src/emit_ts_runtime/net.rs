/// Network and utility modules: http, regex, time, crypto, uuid, log.

// ──────────────────────────────── http ────────────────────────────────

pub(super) const MOD_HTTP_TS: &str = r#"const __almd_http = {
  async serve(port: number, handler: (req: any) => any): Promise<void> { const server = Deno.serve({ port }, async (request: Request) => { const url = new URL(request.url); const method = request.method; const path = url.pathname; const body = method === "POST" || method === "PUT" ? await request.text() : ""; const headers: Record<string, string> = {}; request.headers.forEach((v: string, k: string) => { headers[k] = v; }); const req = { method, path, body, headers }; const res = handler(req); return new Response(res.body, { status: res.status, headers: res.headers || {} }); }); },
  response(status: number, body: string): any { return { status, body, headers: { "content-type": "text/plain" } }; },
  json(status: number, body: string): any { return { status, body, headers: { "content-type": "application/json" } }; },
  with_headers(status: number, body: string, headers: any): any { const h: Record<string, string> = {}; if (headers instanceof Map) { headers.forEach((v: string, k: string) => { h[k] = v; }); } else { Object.assign(h, headers); } return { status, body, headers: h }; },
  async get(url: string): Promise<string> { const r = await fetch(url); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
  async post(url: string, body: string): Promise<string> { const r = await fetch(url, { method: "POST", body, headers: { "content-type": "application/json" } }); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
  async get_with_headers(url: string, headers: Map<string, string>): Promise<string> { const h: Record<string, string> = {}; headers.forEach((v, k) => { h[k] = v; }); const r = await fetch(url, { headers: h }); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
  async request(method: string, url: string, body: string, headers: Map<string, string>): Promise<string> { const h: Record<string, string> = {}; headers.forEach((v, k) => { h[k] = v; }); const opts: any = { method, headers: h }; if (body) opts.body = body; const r = await fetch(url, opts); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
};
"#;

pub(super) const MOD_HTTP_JS: &str = r#"const __almd_http = {
  async serve(port, handler) { const http = require("http"); const server = http.createServer(async (req, res) => { let body = ""; req.on("data", (c) => { body += c; }); req.on("end", () => { const r = handler({ method: req.method, path: req.url, body, headers: req.headers || {} }); const headers = r.headers || {}; res.writeHead(r.status, headers); res.end(r.body); }); }); server.listen(port); },
  response(status, body) { return { status, body, headers: { "content-type": "text/plain" } }; },
  json(status, body) { return { status, body, headers: { "content-type": "application/json" } }; },
  with_headers(status, body, headers) { const h = {}; if (headers instanceof Map) { headers.forEach((v, k) => { h[k] = v; }); } else { Object.assign(h, headers); } return { status, body, headers: h }; },
  async get(url) { return new Promise((resolve, reject) => { const m = url.startsWith("https") ? require("https") : require("http"); m.get(url, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }).on("error", reject); }); },
  async post(url, body) { return new Promise((resolve, reject) => { const u = new URL(url); const m = u.protocol === "https:" ? require("https") : require("http"); const req = m.request({ hostname: u.hostname, port: u.port, path: u.pathname + u.search, method: "POST", headers: { "content-type": "application/json", "content-length": Buffer.byteLength(body) } }, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }); req.on("error", reject); req.write(body); req.end(); }); },
  async get_with_headers(url, headers) { const h = {}; headers.forEach((v, k) => { h[k] = v; }); return new Promise((resolve, reject) => { const u = new URL(url); const m = u.protocol === "https:" ? require("https") : require("http"); m.get({ hostname: u.hostname, port: u.port, path: u.pathname + u.search, headers: h }, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }).on("error", reject); }); },
  async request(method, url, body, headers) { const h = {}; headers.forEach((v, k) => { h[k] = v; }); return new Promise((resolve, reject) => { const u = new URL(url); const m = u.protocol === "https:" ? require("https") : require("http"); const req = m.request({ hostname: u.hostname, port: u.port, path: u.pathname + u.search, method, headers: h }, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }); req.on("error", reject); if (body) req.write(body); req.end(); }); },
};
"#;

// ──────────────────────────────── regex ────────────────────────────────

pub(super) const MOD_REGEX_TS: &str = r#"const __almd_regex = {
  is_match(pat: string, s: string): boolean { return new RegExp(pat).test(s); },
  full_match(pat: string, s: string): boolean { return new RegExp(`^(?:${pat})$`).test(s); },
  find(pat: string, s: string): string | null { const m = s.match(new RegExp(pat)); return m ? m[0] : null; },
  find_all(pat: string, s: string): string[] { const m = s.match(new RegExp(pat, 'g')); return m ? [...m] : []; },
  replace(pat: string, s: string, rep: string): string { return s.replace(new RegExp(pat, 'g'), rep); },
  replace_first(pat: string, s: string, rep: string): string { return s.replace(new RegExp(pat), rep); },
  split(pat: string, s: string): string[] { return s.split(new RegExp(pat)); },
  captures(pat: string, s: string): string[] | null { const m = s.match(new RegExp(pat)); return m && m.length > 1 ? m.slice(1) : null; },
};
"#;

pub(super) const MOD_REGEX_JS: &str = r#"const __almd_regex = {
  is_match(pat, s) { return new RegExp(pat).test(s); },
  full_match(pat, s) { return new RegExp(`^(?:${pat})$`).test(s); },
  find(pat, s) { const m = s.match(new RegExp(pat)); return m ? m[0] : null; },
  find_all(pat, s) { const m = s.match(new RegExp(pat, 'g')); return m ? [...m] : []; },
  replace(pat, s, rep) { return s.replace(new RegExp(pat, 'g'), rep); },
  replace_first(pat, s, rep) { return s.replace(new RegExp(pat), rep); },
  split(pat, s) { return s.split(new RegExp(pat)); },
  captures(pat, s) { const m = s.match(new RegExp(pat)); return m && m.length > 1 ? m.slice(1) : null; },
};
"#;

// ──────────────────────────────── time ────────────────────────────────

pub(super) const MOD_TIME_TS: &str = r#"const __almd_time = {
  now(): number { return Math.floor(Date.now() / 1000); },
  millis(): number { return Date.now(); },
  sleep(ms: number): void { /* Deno */ if (typeof Deno !== "undefined") { const end = Date.now() + ms; while (Date.now() < end) {} } },
  _parts(ts: number): [number, number, number, number, number, number] { const d = new Date(ts * 1000); return [d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds()]; },
  year(ts: number): number { return new Date(ts * 1000).getUTCFullYear(); },
  month(ts: number): number { return new Date(ts * 1000).getUTCMonth() + 1; },
  day(ts: number): number { return new Date(ts * 1000).getUTCDate(); },
  hour(ts: number): number { return new Date(ts * 1000).getUTCHours(); },
  minute(ts: number): number { return new Date(ts * 1000).getUTCMinutes(); },
  second(ts: number): number { return new Date(ts * 1000).getUTCSeconds(); },
  weekday(ts: number): number { const d = new Date(ts * 1000).getUTCDay(); return d === 0 ? 6 : d - 1; },
  to_iso(ts: number): string { const [y, m, d, h, mi, s] = __almd_time._parts(ts); return `${String(y).padStart(4,"0")}-${String(m).padStart(2,"0")}-${String(d).padStart(2,"0")}T${String(h).padStart(2,"0")}:${String(mi).padStart(2,"0")}:${String(s).padStart(2,"0")}Z`; },
  from_parts(y: number, m: number, d: number, h: number, min: number, s: number): number { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
};
"#;

pub(super) const MOD_TIME_JS: &str = r#"const __almd_time = {
  now() { return Math.floor(Date.now() / 1000); },
  millis() { return Date.now(); },
  sleep(ms) { const end = Date.now() + ms; while (Date.now() < end) {} },
  _parts(ts) { const d = new Date(ts * 1000); return [d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds()]; },
  year(ts) { return new Date(ts * 1000).getUTCFullYear(); },
  month(ts) { return new Date(ts * 1000).getUTCMonth() + 1; },
  day(ts) { return new Date(ts * 1000).getUTCDate(); },
  hour(ts) { return new Date(ts * 1000).getUTCHours(); },
  minute(ts) { return new Date(ts * 1000).getUTCMinutes(); },
  second(ts) { return new Date(ts * 1000).getUTCSeconds(); },
  weekday(ts) { const d = new Date(ts * 1000).getUTCDay(); return d === 0 ? 6 : d - 1; },
  to_iso(ts) { const [y, m, d, h, mi, s] = __almd_time._parts(ts); return `${String(y).padStart(4,"0")}-${String(m).padStart(2,"0")}-${String(d).padStart(2,"0")}T${String(h).padStart(2,"0")}:${String(mi).padStart(2,"0")}:${String(s).padStart(2,"0")}Z`; },
  from_parts(y, m, d, h, min, s) { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
};
"#;

// ──────────────────────────────── crypto ────────────────────────────────

pub(super) const MOD_CRYPTO_TS: &str = r#"const __almd_crypto = {
  random_bytes(n: number): number[] {
    const buf = new Uint8Array(n);
    crypto.getRandomValues(buf);
    return Array.from(buf);
  },
  random_hex(n: number): string {
    const buf = new Uint8Array(n);
    crypto.getRandomValues(buf);
    return Array.from(buf).map(b => b.toString(16).padStart(2, "0")).join("");
  },
  async hmac_sha256(key: string, data: string): Promise<string> {
    const enc = new TextEncoder();
    const k = await crypto.subtle.importKey("raw", enc.encode(key), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
    const sig = await crypto.subtle.sign("HMAC", k, enc.encode(data));
    return Array.from(new Uint8Array(sig)).map(b => b.toString(16).padStart(2, "0")).join("");
  },
  async hmac_verify(key: string, data: string, signature: string): Promise<boolean> {
    const computed = await __almd_crypto.hmac_sha256(key, data);
    if (computed.length !== signature.length) return false;
    let diff = 0;
    for (let i = 0; i < computed.length; i++) diff |= computed.charCodeAt(i) ^ signature.charCodeAt(i);
    return diff === 0;
  },
};
"#;

pub(super) const MOD_CRYPTO_JS: &str = r#"const __almd_crypto = {
  random_bytes(n) {
    return Array.from(require("crypto").randomBytes(n));
  },
  random_hex(n) {
    return require("crypto").randomBytes(n).toString("hex");
  },
  hmac_sha256(key, data) {
    const h = require("crypto").createHmac("sha256", key);
    h.update(data);
    return h.digest("hex");
  },
  hmac_verify(key, data, signature) {
    const computed = __almd_crypto.hmac_sha256(key, data);
    if (computed.length !== signature.length) return false;
    return require("crypto").timingSafeEqual(Buffer.from(computed), Buffer.from(signature));
  },
};
"#;

// ──────────────────────────────── uuid ────────────────────────────────

pub(super) const MOD_UUID_TS: &str = r#"const __almd_uuid = {
  v4(): string {
    return crypto.randomUUID();
  },
  v5(namespace: string, name: string): string {
    // v5 requires SHA-1 — simplified implementation
    const data = namespace.replace(/-/g, "") + name;
    let hash = 0;
    for (let i = 0; i < data.length; i++) {
      hash = ((hash << 5) - hash + data.charCodeAt(i)) | 0;
    }
    const hex = Math.abs(hash).toString(16).padStart(32, "0").slice(0, 32);
    return `${hex.slice(0,8)}-${hex.slice(8,12)}-5${hex.slice(13,16)}-${(parseInt(hex.slice(16,18),16) & 0x3F | 0x80).toString(16)}${hex.slice(18,20)}-${hex.slice(20,32)}`;
  },
  parse(s: string): string {
    if (!__almd_uuid.is_valid(s)) throw new Error(`invalid UUID: ${s}`);
    return s.toLowerCase();
  },
  is_valid(s: string): boolean {
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s.trim());
  },
  nil(): string { return "00000000-0000-0000-0000-000000000000"; },
  version(s: string): number {
    if (!__almd_uuid.is_valid(s)) throw new Error(`invalid UUID: ${s}`);
    return parseInt(s.charAt(14), 16);
  },
};
"#;

pub(super) const MOD_UUID_JS: &str = r#"const __almd_uuid = {
  v4() {
    return require("crypto").randomUUID();
  },
  v5(namespace, name) {
    const data = namespace.replace(/-/g, "") + name;
    let hash = 0;
    for (let i = 0; i < data.length; i++) {
      hash = ((hash << 5) - hash + data.charCodeAt(i)) | 0;
    }
    const hex = Math.abs(hash).toString(16).padStart(32, "0").slice(0, 32);
    return `${hex.slice(0,8)}-${hex.slice(8,12)}-5${hex.slice(13,16)}-${(parseInt(hex.slice(16,18),16) & 0x3F | 0x80).toString(16)}${hex.slice(18,20)}-${hex.slice(20,32)}`;
  },
  parse(s) {
    if (!__almd_uuid.is_valid(s)) throw new Error("invalid UUID: " + s);
    return s.toLowerCase();
  },
  is_valid(s) {
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s.trim());
  },
  nil() { return "00000000-0000-0000-0000-000000000000"; },
  version(s) {
    if (!__almd_uuid.is_valid(s)) throw new Error("invalid UUID: " + s);
    return parseInt(s.charAt(14), 16);
  },
};
"#;

// ──────────────────────────────── log ────────────────────────────────

pub(super) const MOD_LOG_TS: &str = r#"const __almd_log = {
  debug(msg: string): void { console.error(`[DEBUG] ${msg}`); },
  info(msg: string): void { console.error(`[INFO] ${msg}`); },
  warn(msg: string): void { console.error(`[WARN] ${msg}`); },
  error(msg: string): void { console.error(`[ERROR] ${msg}`); },
  debug_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[DEBUG] ${msg} ${kv}`); },
  info_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[INFO] ${msg} ${kv}`); },
  warn_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[WARN] ${msg} ${kv}`); },
  error_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[ERROR] ${msg} ${kv}`); },
};
"#;

pub(super) const MOD_LOG_JS: &str = r#"const __almd_log = {
  debug(msg) { console.error("[DEBUG] " + msg); },
  info(msg) { console.error("[INFO] " + msg); },
  warn(msg) { console.error("[WARN] " + msg); },
  error(msg) { console.error("[ERROR] " + msg); },
  debug_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[DEBUG] " + msg + " " + kv); },
  info_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[INFO] " + msg + " " + kv); },
  warn_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[WARN] " + msg + " " + kv); },
  error_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[ERROR] " + msg + " " + kv); },
};
"#;
