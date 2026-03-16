function __bigop(op, a, b) {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    var r;
    switch(op) {
      case "^": r = ba ^ bb; break;
      case "*": r = ba * bb; break;
      case "%": r = ba % bb; break;
      case "+": r = ba + bb; break;
      case "-": r = ba - bb; break;
      default: r = ba;
    }
    return BigInt.asIntN(64, r);
  }
  switch(op) {
    case "^": return a ^ b; case "*": return a * b; case "%": return a % b;
    case "+": return a + b; case "-": return a - b; default: return a;
  }
}
function __div(a, b) {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    return BigInt.asIntN(64, ba / bb);
  }
  const r = a / b;
  return (Number.isInteger(a) && Number.isInteger(b)) ? Math.trunc(r) : r;
}
function println(s) { console.log(s); }
function eprintln(s) { console.error(s); }
class __Err { constructor(message, value) { this.message = message; this.value = value !== undefined ? value : message; } }
function __deep_eq(a, b) {
  if (a === b) return true;
  if (a instanceof __Err && b instanceof __Err) return __deep_eq(a.value, b.value);
  if (a instanceof __Err || b instanceof __Err) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) { if (!__deep_eq(a[i], b[i])) return false; }
    return true;
  }
  if (a instanceof Map && b instanceof Map) {
    if (a.size !== b.size) return false;
    for (const [k, v] of a) { if (!b.has(k) || !__deep_eq(v, b.get(k))) return false; }
    return true;
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) { if (!__deep_eq(a[k], b[k])) return false; }
    return true;
  }
  return false;
}
function assert_eq(a, b, msg) { if (!__deep_eq(a, b)) { var m = msg ? msg + ": " : ""; throw new Error(m + "assert_eq failed\n  expected: " + JSON.stringify(b) + "\n       got: " + JSON.stringify(a)); } }
function assert_ne(a, b, msg) { if (__deep_eq(a, b)) { var m = msg ? msg + ": " : ""; throw new Error(m + "assert_ne failed\n  both are: " + JSON.stringify(a)); } }
function assert(c, msg) { if (!c) throw new Error(msg ? msg : "assertion failed"); }
function unwrap_or(x, d) { return x !== null ? x : d; }
function __concat(a, b) { return typeof a === "string" ? a + b : [...a, ...b]; }
function __throw(msg) { throw new Error(msg); }
function __unwrap(r) { if (r.ok) return r.value; throw new Error(String(r.error)); }
function __assert_throws(fn, expectedMsg) {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
