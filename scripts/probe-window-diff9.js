// Probe only: the nine instructions proofs/GuardWindow.v interprets, run by
// an independent WebAssembly engine (the one in Node) on the vectors
// probe-window-controls.py wrote. One unsigned result per line.
//
//   node probe-window-diff9.js <vectors.json>
//
// The module is assembled here by hand, one exported function per
// instruction: `local.get 0 [; local.get 1] ; <instruction> ; end`.
const fs = require('fs');

const OPS = [
  ['i32.add', 0x6a, 0], ['i32.mul', 0x6c, 0], ['i32.shl', 0x74, 0], ['i32.div_u', 0x6e, 0], ['i32.shr_u', 0x76, 0],
  ['i32.ge_u', 0x4f, 0], ['i64.ge_u', 0x5a, 1], ['i32.wrap_i64', 0xa7, 2], ['i64.extend_i32_u', 0xad, 3],
];
const I32 = 0x7f, I64 = 0x7e;
// (i32,i32)->i32, (i64,i64)->i32, (i64)->i32, (i32)->i64
const TYPES = [[[I32, I32], [I32]], [[I64, I64], [I32]], [[I64], [I32]], [[I32], [I64]]];

const vec = (xs) => [xs.length, ...xs.flat()];
const section = (id, body) => [id, body.length, ...body];
const str = (s) => [s.length, ...Buffer.from(s)];

const types = vec(TYPES.map(([p, r]) => [0x60, p.length, ...p, r.length, ...r]));
const funcs = vec(OPS.map(([, , t]) => [t]));
const exports_ = vec(OPS.map(([name], i) => [...str(name), 0x00, i]));
const code = vec(OPS.map(([, op, t]) => {
  const body = [0x00, 0x20, 0x00, ...(TYPES[t][0].length === 2 ? [0x20, 0x01] : []), op, 0x0b];
  return [body.length, ...body];
}));
const bytes = Uint8Array.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  ...section(1, types), ...section(3, funcs), ...section(7, exports_), ...section(10, code)]);
const inst = new WebAssembly.Instance(new WebAssembly.Module(bytes));

const kind = Object.fromEntries(OPS.map(([name, , t]) => [name, t]));
const out = [];
for (const [name, args] of JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))) {
  const t = kind[name];
  const wide = TYPES[t][0][0] === I64;
  const a = args.map((x) => (wide ? BigInt.asIntN(64, BigInt(x)) : Number(BigInt(x))));
  const r = inst.exports[name](...a);
  out.push(typeof r === 'bigint' ? BigInt.asUintN(64, r).toString() : (r >>> 0).toString());
}
console.log(out.join('\n'));
