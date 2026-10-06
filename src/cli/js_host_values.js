/** Thrown by an exported effect fn that returned err: `message` is its error. */
export class AlmideError extends Error {
  constructor(message) {
    super(message);
    this.name = "AlmideError";
  }
}
/** The module's linear memory size in bytes — a leak check's probe: it stays flat while every block the host receives is released. */
export function memoryBytes() { ready(); return memory.buffer.byteLength; }
// Boundary values (#3354), read and built by the layout the module recorded.
// A shape `{ k }` is "int" | "float" | "bool" | "unit" (the slot holds the
// value) or a block the slot points at: "str", "bytes", "list" { el, stride },
// "opt" { el } (0 = none), "rec" { size, fields: [[name, offset, shape]] },
// "res" { ok, err } (tag at payload+0, 0 = ok; value at payload+8).
function isBlock(s) { return s.k !== "int" && s.k !== "float" && s.k !== "bool" && s.k !== "unit"; }
function slotSize(s) { return s.k === "int" || s.k === "float" ? 8 : 4; }
function loadSlot(a, s, what) {
  const v = view();
  if (s.k === "int") return fromI64(v.getBigInt64(a, true), what);
  if (s.k === "float") return v.getFloat64(a, true);
  if (s.k === "bool") return v.getUint32(a, true) !== 0;
  if (s.k === "unit") return undefined;
  return readBlock(v.getUint32(a, true), s, what);
}
function readBlock(h, s, what) {
  if (s.k === "opt") return h === 0 ? undefined : loadSlot(h + PAYLOAD, s.el, what);
  if (s.k === "str") return readString(h);
  const len = view().getUint32(h + 4, true);
  if (s.k === "bytes") return bytes().slice(h + PAYLOAD, h + PAYLOAD + len);
  if (s.k === "list") {
    const out = new Array(len / s.stride);
    for (let i = 0; i < out.length; i++) out[i] = loadSlot(h + PAYLOAD + i * s.stride, s.el, what);
    return out;
  }
  const o = {};
  for (const [name, off, fs] of s.fields) o[name] = loadSlot(h + PAYLOAD + off, fs, what);
  return o;
}
function childBlocks(h, s) {
  const v = view();
  const out = [];
  const at = (a, cs) => { if (isBlock(cs)) out.push([v.getUint32(a, true), cs]); };
  if (s.k === "opt") at(h + PAYLOAD, s.el);
  else if (s.k === "list") for (let a = h + PAYLOAD, end = a + v.getUint32(h + 4, true); a < end; a += s.stride) at(a, s.el);
  else if (s.k === "rec") for (const [, off, fs] of s.fields) at(h + PAYLOAD + off, fs);
  else if (s.k === "res") at(h + PAYLOAD + 8, v.getUint32(h + PAYLOAD, true) === 0 ? s.ok : s.err);
  return out;
}
// Release one credit on `h`. The module's release is flat: when this was the
// last credit, the credits the block held on its children are ours to release.
function drop(h, s) {
  if (h === 0) return;
  const kids = view().getUint32(h, true) === 1 ? childBlocks(h, s) : [];
  instance.exports.__release(h);
  for (const [c, cs] of kids) drop(c, cs);
}
function blockLen(s, x, what) {
  if (s.k === "opt") return slotSize(s.el);
  if (s.k === "rec") {
    if (x === null || typeof x !== "object") throw new TypeError(`almide: ${what} takes an object, got ${x}`);
    return s.size;
  }
  if (!(Array.isArray(x) || ArrayBuffer.isView(x))) throw new TypeError(`almide: ${what} takes an array, got ${typeof x}`);
  return x.length * s.stride;
}
function fillBlock(h, s, x, what) {
  if (s.k === "opt") storeSlot(h + PAYLOAD, s.el, x, what);
  else if (s.k === "list") for (let i = 0; i < x.length; i++) storeSlot(h + PAYLOAD + i * s.stride, s.el, x[i], what);
  else for (const [name, off, fs] of s.fields) {
    if (!(name in x)) throw new TypeError(`almide: ${what} needs the field \`${name}\``);
    storeSlot(h + PAYLOAD + off, fs, x[name], what);
  }
}
function putBlock(s, x, what) {
  if (s.k === "str") return allocString(x);
  if (s.k === "opt" && (x === undefined || x === null)) return 0;
  if (s.k === "bytes") {
    const b = x instanceof Uint8Array ? x : Uint8Array.from(x);
    const h = instance.exports.__alloc(b.length);
    bytes().set(b, h + PAYLOAD);
    return h;
  }
  const len = blockLen(s, x, what);
  const h = instance.exports.__alloc(len);
  // Zeroed first, so a throw midway leaves only null children to drop.
  bytes().fill(0, h + PAYLOAD, h + PAYLOAD + len);
  try {
    fillBlock(h, s, x, what);
  } catch (e) {
    drop(h, s);
    throw e;
  }
  return h;
}
function storeSlot(a, s, x, what) {
  if (s.k === "int") view().setBigInt64(a, toI64(x, what), true);
  else if (s.k === "float") view().setFloat64(a, Number(x), true);
  else if (s.k === "bool") view().setUint32(a, x ? 1 : 0, true);
  else if (s.k === "unit") view().setUint32(a, 0, true);
  else { const c = putBlock(s, x, what); view().setUint32(a, c, true); }
}
// A fallible import's answer (#3356): a Result block the module owns.
function okResult(s, x) {
  const h = instance.exports.__alloc(16);
  bytes().fill(0, h + PAYLOAD, h + PAYLOAD + 16);
  try {
    storeSlot(h + PAYLOAD + 8, s, x, "hook");
  } catch (e) {
    drop(h, { k: "res", ok: s, err: { k: "str" } });
    return errResult(e);
  }
  return h;
}
function errResult(e) {
  const msg = allocString(e instanceof Error ? e.message : String(e));
  const h = instance.exports.__alloc(16);
  bytes().fill(0, h + PAYLOAD, h + PAYLOAD + 16);
  view().setUint32(h + PAYLOAD, 1, true);
  view().setUint32(h + PAYLOAD + 8, msg, true);
  return h;
}
function takeBlock(h, s, what) {
  try { return readBlock(h, s, what); } finally { drop(h, s); }
}
function takeResult(h, s, what) {
  const ok = view().getUint32(h + PAYLOAD, true) === 0;
  let value;
  try { value = loadSlot(h + PAYLOAD + 8, ok ? s.ok : s.err, what); } finally { drop(h, s); }
  if (!ok) throw new AlmideError(value);
  return value;
}
