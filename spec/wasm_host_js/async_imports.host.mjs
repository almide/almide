// Host-side checks for async_imports.almd (#3353): the async hooks really
// await; the exports that reach them return Promises and the others stay
// synchronous; overlapping calls run one at a time in call order; an err
// surfaces as a rejection, a rejected infallible hook abandons the instance
// (#3356); memory stays flat over a
// long run; and without JSPI `init()` refuses with a message naming it.
import assert from "node:assert/strict";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let active = 0;
let maxActive = 0;
const order = [];
const logged = [];

export const js = {
  async kv_get(key) {
    active++;
    maxActive = Math.max(maxActive, active);
    order.push(key);
    await sleep(key.length % 3);
    active--;
    if (key === "boom") throw new Error("hook failed");
    return key === "none" ? "" : "v-" + key;
  },
  async kv_count(prefix) {
    await sleep(0);
    return prefix.length;
  },
  log_line(msg) {
    logged.push(msg);
  },
};

export async function after(m) {
  const p = m.lookup("a");
  assert.ok(p instanceof Promise, "an export reaching an async import is async");
  // The call enters on a later turn; once it is suspended in the hook, a
  // synchronous export refuses to enter.
  await new Promise((r) => setImmediate(r));
  assert.equal(active, 1, "the call is suspended in kv_get");
  assert.throws(() => m.width("abc"), /suspended/);
  assert.equal(await p, "value=v-a");
  assert.equal(m.width("abc"), 3, "an export that cannot reach one stays synchronous");
  m.log_twice("hi");
  assert.deepEqual(logged, ["hi", "hi!"]);
  // Overlapping calls: each gets its own answer, they enter one at a time,
  // in call order.
  order.length = 0;
  const keys = ["xyz", "q", "mm", "longer", "k"];
  assert.deepEqual(await Promise.all(keys.map((k) => m.lookup(k))), keys.map((k) => "value=v-" + k));
  assert.deepEqual(order, keys);
  assert.equal(maxActive, 1, "two calls were suspended in the module at once");
  // A closure inside the module reaches the async import too.
  assert.equal(await m.total(["a", "bb", "ccc"]), 6);
  // An effect fn: ok, and an err from Almide code.
  assert.equal(await m.must_get("k"), "v-k");
  await assert.rejects(m.must_get("none"), (e) => e instanceof m.AlmideError && e.message === "missing none");
  // A rejection out of the infallible `kv_get` abandons the instance
  // (#3356: its unwound frames kept their blocks); init() starts afresh.
  await assert.rejects(m.lookup("boom"), /hooks\.js\.kv_get threw, but its @extern is infallible.*hook failed/);
  await assert.rejects(m.lookup("after"), /abandoned/);
  await m.init(undefined, { js });
  assert.equal(await m.lookup("after"), "value=v-after");
  // A long run: every block released, memory flat.
  for (let i = 0; i < 200; i++) await m.must_get("w" + i);
  const settled = m.memoryBytes();
  for (let i = 0; i < 1000; i++) {
    assert.equal(await m.lookup("r" + i), "value=v-r" + i);
    await assert.rejects(m.must_get("none"));
  }
  assert.equal(m.memoryBytes(), settled, "memory grew across 1000 async rounds");
  // Without JSPI, init refuses and names what is missing.
  const saved = WebAssembly.Suspending;
  delete WebAssembly.Suspending;
  try {
    await assert.rejects(m.init(undefined, { js }), /JSPI.*WebAssembly\.Suspending.*\(this is Node \d+\.\d+\.\d+\).*Node >= 24\.20/);
  } finally {
    WebAssembly.Suspending = saved;
  }
  console.log("async ok");
}
