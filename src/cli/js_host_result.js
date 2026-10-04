/** Thrown by an exported effect fn that returned err: `message` is its error. */
export class AlmideError extends Error {
  constructor(message) {
    super(message);
    this.name = "AlmideError";
  }
}
function takeResult(h, kind, what) {
  const v = view();
  const slot = h + PAYLOAD + 8;
  const tag = v.getUint32(h + PAYLOAD, true);
  const sole = v.getUint32(h, true) === 1;
  const inner = (kind === "string" || tag !== 0) ? v.getUint32(slot, true) : 0;
  let raw;
  if (tag !== 0 || kind === "string") raw = readString(inner);
  else if (kind === "int") raw = v.getBigInt64(slot, true);
  else if (kind === "float") raw = v.getFloat64(slot, true);
  else if (kind === "bool") raw = v.getUint32(slot, true) !== 0;
  instance.exports.__release(h);
  if (sole && inner !== 0) instance.exports.__release(inner);
  if (tag !== 0) throw new AlmideError(raw);
  if (kind === "int") return fromI64(raw, what);
  if (kind === "unit") return undefined;
  return raw;
}
