/// Core computation modules: string, int, float, math, random.

// ──────────────────────────────── string ────────────────────────────────

pub(super) const MOD_STRING_TS: &str = r#"const __almd_string = {
  trim(s: string): string { return s.trim(); },
  split(s: string, sep: string): string[] { return s.split(sep); },
  join(arr: string[], sep: string): string { return arr.join(sep); },
  len(s: string): number { return s.length; },
  pad_left(s: string, n: number, ch: string): string { return s.padStart(n, ch); },
  starts_with(s: string, prefix: string): boolean { return s.startsWith(prefix); },
  slice(s: string, start: number, end?: number): string { return end !== undefined ? s.slice(start, end) : s.slice(start); },
  to_bytes(s: string): number[] { return Array.from(new TextEncoder().encode(s)); },
  contains(s: string, sub: string): boolean { return s.includes(sub); },
  capitalize(s: string): string { return s.length === 0 ? "" : s[0].toUpperCase() + s.slice(1); },
  to_upper(s: string): string { return s.toUpperCase(); },
  to_lower(s: string): string { return s.toLowerCase(); },
  to_int(s: string): number { const n = parseInt(s, 10); if (isNaN(n)) throw new Error("invalid integer: " + s); return n; },
  replace(s: string, from: string, to: string): string { return s.split(from).join(to); },
  char_at(s: string, i: number): string | null { return i < s.length ? s[i] : null; },
  lines(s: string): string[] { return s.split("\n").filter(l => l.length > 0); },
  chars(s: string): string[] { return [...s]; },
  index_of(s: string, needle: string): number | null { const i = s.indexOf(needle); return i >= 0 ? i : null; },
  repeat(s: string, n: number): string { return s.repeat(n); },
  from_bytes(bytes: number[]): string { return new TextDecoder().decode(new Uint8Array(bytes)); },
  is_digit(s: string): boolean { return s.length > 0 && /^[0-9]+$/.test(s); },
  is_alpha(s: string): boolean { return s.length > 0 && /^[a-zA-Z]+$/.test(s); },
  is_alphanumeric(s: string): boolean { return s.length > 0 && /^[a-zA-Z0-9]+$/.test(s); },
  is_whitespace(s: string): boolean { return s.length > 0 && /^\s+$/.test(s); },
  replace_first(s: string, from: string, to: string): string { const i = s.indexOf(from); return i < 0 ? s : s.slice(0, i) + to + s.slice(i + from.length); },
  last_index_of(s: string, needle: string): number | null { const i = s.lastIndexOf(needle); return i >= 0 ? i : null; },
  to_float(s: string): number { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float number: " + s); return n; },
  pad_right(s: string, n: number, ch: string): string { return s.padEnd(n, ch); },
  trim_start(s: string): string { return s.trimStart(); },
  trim_end(s: string): string { return s.trimEnd(); },
  count(s: string, sub: string): number { if (!sub) return 0; let c = 0, i = 0; while ((i = s.indexOf(sub, i)) >= 0) { c++; i += sub.length; } return c; },
  is_empty(s: string): boolean { return s.length === 0; },
  reverse(s: string): string { return [...s].reverse().join(""); },
  strip_prefix(s: string, prefix: string): string | null { return s.startsWith(prefix) ? s.slice(prefix.length) : null; },
  strip_suffix(s: string, suffix: string): string | null { return s.endsWith(suffix) ? s.slice(0, -suffix.length) : null; },
  ends_with(s: string, suffix: string): boolean { return s.endsWith(suffix); },
  is_upper(s: string): boolean { return s.length > 0 && s === s.toUpperCase() && s !== s.toLowerCase(); },
  is_lower(s: string): boolean { return s.length > 0 && s === s.toLowerCase() && s !== s.toUpperCase(); },
};
"#;

pub(super) const MOD_STRING_JS: &str = r#"const __almd_string = {
  trim(s) { return s.trim(); },
  split(s, sep) { return s.split(sep); },
  join(arr, sep) { return arr.join(sep); },
  len(s) { return s.length; },
  pad_left(s, n, ch) { return s.padStart(n, ch); },
  starts_with(s, prefix) { return s.startsWith(prefix); },
  slice(s, start, end) { return end !== undefined ? s.slice(start, end) : s.slice(start); },
  to_bytes(s) { return Array.from(new TextEncoder().encode(s)); },
  contains(s, sub) { return s.includes(sub); },
  capitalize(s) { return s.length === 0 ? "" : s[0].toUpperCase() + s.slice(1); },
  to_upper(s) { return s.toUpperCase(); },
  to_lower(s) { return s.toLowerCase(); },
  to_int(s) { const n = parseInt(s, 10); if (isNaN(n)) throw new Error("invalid integer: " + s); return n; },
  replace(s, from, to) { return s.split(from).join(to); },
  char_at(s, i) { return i < s.length ? s[i] : null; },
  lines(s) { return s.split("\n").filter(l => l.length > 0); },
  chars(s) { return [...s]; },
  index_of(s, needle) { const i = s.indexOf(needle); return i >= 0 ? i : null; },
  repeat(s, n) { return s.repeat(n); },
  from_bytes(bytes) { return new TextDecoder().decode(new Uint8Array(bytes)); },
  is_digit(s) { return s.length > 0 && /^[0-9]+$/.test(s); },
  is_alpha(s) { return s.length > 0 && /^[a-zA-Z]+$/.test(s); },
  is_alphanumeric(s) { return s.length > 0 && /^[a-zA-Z0-9]+$/.test(s); },
  is_whitespace(s) { return s.length > 0 && /^\s+$/.test(s); },
  replace_first(s, from, to) { const i = s.indexOf(from); return i < 0 ? s : s.slice(0, i) + to + s.slice(i + from.length); },
  last_index_of(s, needle) { const i = s.lastIndexOf(needle); return i >= 0 ? i : null; },
  to_float(s) { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float number: " + s); return n; },
  pad_right(s, n, ch) { return s.padEnd(n, ch); },
  trim_start(s) { return s.trimStart(); },
  trim_end(s) { return s.trimEnd(); },
  count(s, sub) { if (!sub) return 0; let c = 0, i = 0; while ((i = s.indexOf(sub, i)) >= 0) { c++; i += sub.length; } return c; },
  is_empty(s) { return s.length === 0; },
  reverse(s) { return [...s].reverse().join(""); },
  strip_prefix(s, prefix) { return s.startsWith(prefix) ? s.slice(prefix.length) : null; },
  strip_suffix(s, suffix) { return s.endsWith(suffix) ? s.slice(0, -suffix.length) : null; },
  ends_with(s, suffix) { return s.endsWith(suffix); },
  is_upper(s) { return s.length > 0 && s === s.toUpperCase() && s !== s.toLowerCase(); },
  is_lower(s) { return s.length > 0 && s === s.toLowerCase() && s !== s.toUpperCase(); },
};
"#;

// ──────────────────────────────── int ────────────────────────────────

pub(super) const MOD_INT_TS: &str = r#"const __almd_int = {
  to_hex(n: bigint): string { return (n >= 0n ? n : n + (1n << 64n)).toString(16); },
  to_string(n: number): string { return String(n); },
  band(a: number, b: number): number { return (a & b) >>> 0; },
  bor(a: number, b: number): number { return (a | b) >>> 0; },
  bxor(a: number, b: number): number { return (a ^ b) >>> 0; },
  bshl(a: number, n: number): number { return (a << n) >>> 0; },
  bshr(a: number, n: number): number { return a >>> n; },
  bnot(a: number): number { return ~a; },
  wrap_add(a: number, b: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return ((a + b) & mask) >>> 0; },
  wrap_mul(a: number, b: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return (Math.imul(a, b) & mask) >>> 0; },
  rotate_right(a: number, n: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return (((v >>> n) | (v << (bits - n))) & mask) >>> 0; },
  rotate_left(a: number, n: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return (((v << n) | (v >>> (bits - n))) & mask) >>> 0; },
  to_u32(a: number): number { return a >>> 0; },
  to_u8(a: number): number { return a & 0xFF; },
  clamp(n: number, lo: number, hi: number): number { return Math.max(lo, Math.min(hi, n)); },
  parse(s: string): number { const n = parseInt(s, 10); if (isNaN(n) || !/^-?\d+$/.test(s.trim())) throw new Error("invalid digit found in string"); return n; },
  parse_hex(s: string): number { const h = s.startsWith("0x") || s.startsWith("0X") ? s.slice(2) : s; const n = parseInt(h, 16); if (isNaN(n)) throw new Error("invalid digit found in string"); return n; },
  abs(n: number): number { return Math.abs(n); },
  min(a: number, b: number): number { return Math.min(a, b); },
  max(a: number, b: number): number { return Math.max(a, b); },
  to_float(n: number): number { return n; },
};
"#;

pub(super) const MOD_INT_JS: &str = r#"const __almd_int = {
  to_hex(n) { return (typeof n === "bigint" ? (n >= 0n ? n : n + (1n << 64n)).toString(16) : n.toString(16)); },
  to_string(n) { return String(n); },
  band(a, b) { return (a & b) >>> 0; },
  bor(a, b) { return (a | b) >>> 0; },
  bxor(a, b) { return (a ^ b) >>> 0; },
  bshl(a, n) { return (a << n) >>> 0; },
  bshr(a, n) { return a >>> n; },
  bnot(a) { return ~a; },
  wrap_add(a, b, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return ((a + b) & mask) >>> 0; },
  wrap_mul(a, b, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return (Math.imul(a, b) & mask) >>> 0; },
  rotate_right(a, n, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return (((v >>> n) | (v << (bits - n))) & mask) >>> 0; },
  rotate_left(a, n, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return (((v << n) | (v >>> (bits - n))) & mask) >>> 0; },
  to_u32(a) { return a >>> 0; },
  to_u8(a) { return a & 0xFF; },
  clamp(n, lo, hi) { return Math.max(lo, Math.min(hi, n)); },
  parse(s) { var n = parseInt(s, 10); if (isNaN(n) || !/^-?\d+$/.test(s.trim())) throw new Error("invalid digit found in string"); return n; },
  parse_hex(s) { var h = s.startsWith("0x") || s.startsWith("0X") ? s.slice(2) : s; var n = parseInt(h, 16); if (isNaN(n)) throw new Error("invalid digit found in string"); return n; },
  abs(n) { return Math.abs(n); },
  min(a, b) { return Math.min(a, b); },
  max(a, b) { return Math.max(a, b); },
  to_float(n) { return n; },
};
"#;

// ──────────────────────────────── float ────────────────────────────────

pub(super) const MOD_FLOAT_TS: &str = r#"const __almd_float = {
  to_string(n: number): string { const s = String(n); return s.includes('.') || s.includes('e') ? s : s + '.0'; },
  to_int(n: number): number { return Math.trunc(n); },
  round(n: number): number { return Math.round(n); },
  floor(n: number): number { return Math.floor(n); },
  ceil(n: number): number { return Math.ceil(n); },
  abs(n: number): number { return Math.abs(n); },
  sqrt(n: number): number { return Math.sqrt(n); },
  parse(s: string): number { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float: " + s); return n; },
  from_int(n: number): number { return n; },
  min(a: number, b: number): number { return Math.min(a, b); },
  max(a: number, b: number): number { return Math.max(a, b); },
  clamp(n: number, lo: number, hi: number): number { return Math.max(lo, Math.min(hi, n)); },
  to_fixed(n: number, decimals: number): string { return n.toFixed(decimals); },
};
"#;

pub(super) const MOD_FLOAT_JS: &str = r#"const __almd_float = {
  to_string(n) { const s = String(n); return s.includes('.') || s.includes('e') ? s : s + '.0'; },
  to_int(n) { return Math.trunc(n); },
  round(n) { return Math.round(n); },
  floor(n) { return Math.floor(n); },
  ceil(n) { return Math.ceil(n); },
  abs(n) { return Math.abs(n); },
  sqrt(n) { return Math.sqrt(n); },
  parse(s) { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float: " + s); return n; },
  from_int(n) { return n; },
  min(a, b) { return Math.min(a, b); },
  max(a, b) { return Math.max(a, b); },
  clamp(n, lo, hi) { return Math.max(lo, Math.min(hi, n)); },
  to_fixed(n, decimals) { return n.toFixed(decimals); },
};
"#;

// ──────────────────────────────── math ────────────────────────────────

pub(super) const MOD_MATH_TS: &str = r#"const __almd_math = {
  min(a: number, b: number): number { return Math.min(a, b); },
  max(a: number, b: number): number { return Math.max(a, b); },
  abs(n: number): number { return Math.abs(n); },
  pow(base: number, exp: number): number { return Math.pow(base, exp); },
  pi(): number { return Math.PI; },
  e(): number { return Math.E; },
  sin(x: number): number { return Math.sin(x); },
  cos(x: number): number { return Math.cos(x); },
  tan(x: number): number { return Math.tan(x); },
  log(x: number): number { return Math.log(x); },
  exp(x: number): number { return Math.exp(x); },
  sqrt(x: number): number { return Math.sqrt(x); },
  factorial(n: number): number { let r = 1; for (let i = 2; i <= n; i++) r *= i; return r; },
  choose(n: number, k: number): number { if (k < 0 || k > n) return 0; if (k === 0 || k === n) return 1; k = Math.min(k, n - k); let r = 1; for (let i = 0; i < k; i++) { r = r * (n - i) / (i + 1); } return Math.round(r); },
  log_gamma(x: number): number { if (x <= 0) return Infinity; if (x < 0.5) return Math.log(Math.PI / Math.sin(Math.PI * x)) - __almd_math.log_gamma(1 - x); x -= 1; const c = [76.18009172947146, -86.50532032941677, 24.01409824083091, -1.231739572450155, 0.1208650973866179e-2, -0.5395239384953e-5]; let sum = 1.000000000190015; let y = x; for (const ci of c) { y += 1; sum += ci / y; } const t = x + 5.5; return -t + (x + 0.5) * Math.log(t) + Math.log(2.5066282746310005 * sum / (x + 1)); },
};
"#;

pub(super) const MOD_MATH_JS: &str = r#"const __almd_math = {
  min(a, b) { return Math.min(a, b); },
  max(a, b) { return Math.max(a, b); },
  abs(n) { return Math.abs(n); },
  pow(base, exp) { return Math.pow(base, exp); },
  pi() { return Math.PI; },
  e() { return Math.E; },
  sin(x) { return Math.sin(x); },
  cos(x) { return Math.cos(x); },
  tan(x) { return Math.tan(x); },
  log(x) { return Math.log(x); },
  exp(x) { return Math.exp(x); },
  sqrt(x) { return Math.sqrt(x); },
  factorial(n) { let r = 1; for (let i = 2; i <= n; i++) r *= i; return r; },
  choose(n, k) { if (k < 0 || k > n) return 0; if (k === 0 || k === n) return 1; k = Math.min(k, n - k); let r = 1; for (let i = 0; i < k; i++) { r = r * (n - i) / (i + 1); } return Math.round(r); },
  log_gamma(x) { if (x <= 0) return Infinity; if (x < 0.5) return Math.log(Math.PI / Math.sin(Math.PI * x)) - __almd_math.log_gamma(1 - x); x -= 1; const c = [76.18009172947146, -86.50532032941677, 24.01409824083091, -1.231739572450155, 0.1208650973866179e-2, -0.5395239384953e-5]; let sum = 1.000000000190015; let y = x; for (const ci of c) { y += 1; sum += ci / y; } const t = x + 5.5; return -t + (x + 0.5) * Math.log(t) + Math.log(2.5066282746310005 * sum / (x + 1)); },
};
"#;

// ──────────────────────────────── random ────────────────────────────────

pub(super) const MOD_RANDOM_TS: &str = r#"const __almd_random = {
  int(min: number, max: number): number { return Math.floor(Math.random() * (max - min + 1)) + min; },
  float(): number { return Math.random(); },
  choice<T>(xs: T[]): T | null { return xs.length > 0 ? xs[Math.floor(Math.random() * xs.length)] : null; },
  shuffle<T>(xs: T[]): T[] { const a = [...xs]; for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; },
};
"#;

pub(super) const MOD_RANDOM_JS: &str = r#"const __almd_random = {
  int(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; },
  float() { return Math.random(); },
  choice(xs) { return xs.length > 0 ? xs[Math.floor(Math.random() * xs.length)] : null; },
  shuffle(xs) { const a = [...xs]; for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; },
};
"#;
