// Host-side matrix for marshal_blocks.almd (#3354): every block shape ×
// {parameter, return} × {fn, effect fn ok, effect fn err}, then a leak loop
// whose memory must stay flat — every block the host builds or receives is
// released, children included, and nothing the module still holds is.
import assert from "node:assert/strict";

const item = { name: "pen", tags: ["blue", "ink"], weight: 1.5, score: 3, ok: true, at: { x: 5, y: 6 } };
const bumped = { name: "PEN", tags: ["blue", "ink", "seen"], weight: 3, score: 4, ok: false, at: { x: 6, y: 5 } };

// [fn, input, expected output]
const cells = [
  ["bytes_rev", new Uint8Array([1, 2, 255]), new Uint8Array([255, 2, 1])],
  ["bytes_rev", new Uint8Array([]), new Uint8Array([])],
  ["ints_double", [1, -2, 2 ** 40], [2, -4, 2 ** 41]],
  ["ints_double", [], []],
  ["floats_half", [1, 3, -0.5], [0.5, 1.5, -0.25]],
  ["bools_not", [true, false], [false, true]],
  ["strs_upper", ["a", "héllo", ""], ["A", "HÉLLO", ""]],
  ["grid_flip", [[1, 2], [3, 4, 5], []], [[2, 1], [5, 4, 3], []]],
  ["opt_inc", 41, 42],
  ["opt_inc", undefined, undefined],
  ["opt_greet", "ada", "hi ada"],
  ["opt_greet", undefined, undefined],
  ["point_swap", { x: 1, y: -2 }, { x: -2, y: 1 }],
  ["item_bump", item, bumped],
  ["item_bump", { ...item, score: undefined, tags: [] }, { ...bumped, score: undefined, tags: ["seen"] }],
  ["points_shift", [{ x: 1, y: 1 }, { x: 2, y: 3 }], [{ x: 2, y: 2 }, { x: 3, y: 4 }]],
  ["points_shift", [], []],
];

export async function after(m) {
  for (const [f, input, expected] of cells) {
    assert.deepEqual(m[f](input), expected, `${f} (fn)`);
    assert.deepEqual(m[`e_${f}`](input, true), expected, `${f} (effect fn, ok)`);
    assert.throws(() => m[`e_${f}`](input, false), (e) => e instanceof m.AlmideError && e.message === "refused", `${f} (effect fn, err)`);
  }
  // The returned object is a fresh plain object: field order from the type.
  assert.deepEqual(Object.keys(m.item_bump(item)), ["name", "tags", "weight", "score", "ok", "at"]);
  assert.ok(m.bytes_rev(new Uint8Array([1])) instanceof Uint8Array);
  // Wrong shapes are refused before the call, not read as garbage.
  assert.throws(() => m.point_swap({ x: 1 }), TypeError);
  assert.throws(() => m.ints_double(5), TypeError);
  assert.throws(() => m.ints_double([1.5]), RangeError);
  // The leak loop: the same rounds, many times; memory must not move.
  const round = () => {
    for (const [f, input] of cells) {
      m[f](input);
      m[`e_${f}`](input, true);
      try { m[`e_${f}`](input, false); } catch {}
    }
    try { m.point_swap({ x: 1 }); } catch {}
    try { m.ints_double([1, 1.5]); } catch {}
  };
  for (let i = 0; i < 200; i++) round();
  const settled = m.memoryBytes();
  for (let i = 0; i < 3000; i++) round();
  assert.equal(m.memoryBytes(), settled, "memory grew across 3000 rounds: a block leaks");
  console.log("blocks ok");
}
