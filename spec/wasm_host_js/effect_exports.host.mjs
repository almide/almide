// Host-side matrix for effect_exports.almd (#3352): each marshalled return
// type × {fn, effect fn} × {ok, err}. An effect fn's ok is the plain value;
// its err throws AlmideError with the program's message; a plain fn never
// throws. Repeated calls run both exits through the module's release.
import assert from "node:assert/strict";

const cells = [
  ["string", "yes1"],
  ["int", 42],
  ["float", 0.25],
  ["bool", true],
  ["unit", undefined],
];

export async function after(m) {
  for (const [kind, okValue] of cells) {
    assert.deepEqual(m[`e_${kind}`](true), okValue, `e_${kind} ok`);
    assert.throws(() => m[`e_${kind}`](false), (e) => e instanceof m.AlmideError && e instanceof Error && e.message === `no ${kind}`, `e_${kind} err`);
    assert.deepEqual(m[`p_${kind}`](true), okValue, `p_${kind}`);
    assert.doesNotThrow(() => m[`p_${kind}`](false), `p_${kind} never throws`);
  }
  // A String in and out of an effect fn, both exits.
  assert.equal(m.e_echo("héllo"), "héllo/1");
  assert.throws(() => m.e_echo(""), { name: "AlmideError", message: "no empty" });
  // A declared Result[Int, String] return unwraps the same way.
  assert.equal(m.e_declared(true), 7);
  assert.throws(() => m.e_declared(false), { name: "AlmideError", message: "declared no" });
  // Both exits many times over: every block the module hands back is released.
  const big = "x".repeat(50000);
  const rounds = (n) => {
    for (let i = 0; i < n; i++) {
      assert.equal(m.e_echo(big + i), big + i + "/1");
      assert.throws(() => m.e_string(false), { message: "no string" });
    }
  };
  rounds(200);
  const settled = m.memoryBytes();
  rounds(2000);
  assert.equal(m.memoryBytes(), settled, "memory grew: a Result block or its payload leaks");
}
