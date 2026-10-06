// Host-side checks for fan_async_overlap.almd (#3383). The hooks answer
// after a per-key delay, the first arms slowest, so the order the values
// come back in is the reverse of arm order. Everything printed is the same
// under both lowerings (the gate compares the two runs with .expected);
// what differs is asserted here, by the lowering the script built:
//   - overlapped (the default on --host js): every hook of a fan is in flight
//     at once, and a fan takes about its slowest hook, not the sum;
//   - ALMIDE_FAN_SEQUENTIAL=1: one hook at a time, and the fan takes the sum.
// The timing margins are wide (overlapped: under the slowest hook plus half
// of the rest; sequential: at least 90% of the sum): the in-flight count is
// the exact witness, the clock the coarse one.
import assert from "node:assert/strict";

const sequential = !!process.env.ALMIDE_FAN_SEQUENTIAL && process.env.ALMIDE_FAN_SEQUENTIAL !== "0";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const DELAY = { k0: 80, k1: 70, k2: 60, k3: 50, k4: 40, k5: 30, k6: 20, k7: 10, bad1: 30, bad2: 5 };
const delayOf = (key) => DELAY[key] ?? 1;
let active = 0;
let maxActive = 0;
let calls = 0;
async function served(key) {
  calls++;
  active++;
  maxActive = Math.max(maxActive, active);
  await sleep(delayOf(key));
  active--;
}

export const js = {
  async fetch(key) {
    await served(key);
    if (key.startsWith("bad")) throw new Error("no " + key);
    return "v-" + key;
  },
  async size(key) {
    await served(key);
    if (key.startsWith("bad")) throw "size refused " + key;
    return key.length;
  },
  async peek(key) {
    await served(key);
    if (key.startsWith("bad")) throw new Error("peek failed on " + key);
    return key;
  },
};

// One measured call: its value (or error message), the time it took, and
// the most hooks it had in flight at once.
async function measured(f) {
  calls = 0;
  maxActive = 0;
  const t0 = performance.now();
  let out;
  try {
    out = await f();
  } catch (e) {
    out = "err: " + e.message;
  }
  return { out, ms: performance.now() - t0, inFlight: maxActive, calls };
}

function expectLowering(what, r, n, sum, max) {
  const mode = sequential ? "sequential" : "overlapped";
  console.error(`${what}: ${mode} ${r.ms.toFixed(1)} ms over ${n} hooks (sum of delays ${sum} ms), at most ${r.inFlight} in flight`);
  if (sequential) {
    assert.equal(r.inFlight, 1, `${what}: the sequential lowering waits on one hook at a time`);
    assert.ok(r.ms >= sum * 0.9, `${what}: sequential took ${r.ms} ms, under the ${sum} ms its hooks sum to`);
  } else {
    assert.equal(r.inFlight, n, `${what}: every element's hook is in flight at once`);
    const bound = max + (sum - max) / 2;
    assert.ok(r.ms < bound, `${what}: overlapped took ${r.ms} ms, not under ${bound} ms (slowest hook ${max} ms, sum ${sum} ms)`);
  }
}

export async function after(m) {
  const keys = ["k0", "k1", "k2", "k3", "k4", "k5", "k6", "k7"];
  const sum = keys.reduce((a, k) => a + delayOf(k), 0);
  const max = Math.max(...keys.map(delayOf));

  // fan.map over a fallible hook: values in arm order.
  const all = await measured(() => m.fetch_all(keys));
  expectLowering("fan.map", all, keys.length, sum, max);
  console.log("fetch_all: " + all.out.join(","));

  // A rejection in a middle arm: every element runs (D1), and the first err
  // in arm order is the result (D6) — bad2 rejects first, bad1 wins.
  const mixed = await measured(() => m.fetch_all(["k0", "bad1", "k2", "bad2", "k4"]));
  console.log(`fetch_all mixed: ${mixed.out} (calls=${mixed.calls})`);
  console.log("fetch_all empty: [" + (await m.fetch_all([])).join(",") + "]");

  // A Result[Int, String] hook.
  console.log("sizes: " + (await m.sizes(["a", "bb", "ccc"])).join(","));
  console.log("sizes mixed: " + (await measured(() => m.sizes(["a", "bad1", "c"]))).out);

  // An infallible hook wrapped in ok(...).
  const peeked = await measured(() => m.peek_all(keys));
  expectLowering("fan.map ok(peek)", peeked, keys.length, sum, max);
  console.log("peek_all: " + peeked.out.join(","));

  // fan.any: the first ok in arm order wins; all-fail is the ledger Err.
  console.log("first_found: " + (await measured(() => m.first_found(["bad1", "k1", "k0"]))).out);
  console.log("first_found none: " + (await measured(() => m.first_found(["bad1", "bad2"]))).out);

  // A fan block: one hook per arm, a tuple in arm order. Its first err
  // aborts through the main-error path after every arm ran (C-005).
  const both = await measured(() => m.pair("k0", "k1"));
  expectLowering("fan { }", both, 2, delayOf("k0") + delayOf("k1"), delayOf("k0"));
  console.log("pair: " + both.out);
  console.log("pair err: " + (await measured(() => m.pair("bad1", "k1"))).out);

  // Memory stays flat over rounds that take, skip and drop slots.
  const round = async () => {
    for (let i = 0; i < 3; i++) {
      await m.fetch_all(["a" + i, "b" + i]);
      await assert.rejects(m.fetch_all(["a", "bad" + i, "c"]));
      await m.first_found(["bad", "x" + i, "y"]);
      await m.peek_all(["p" + i]);
      await m.pair("q" + i, "r");
    }
  };
  for (let i = 0; i < 20; i++) await round();
  const settled = m.memoryBytes();
  for (let i = 0; i < 200; i++) await round();
  assert.equal(m.memoryBytes(), settled, "memory grew across 200 rounds of overlapped fans");
  console.log("memory flat");

  // A rejected infallible hook abandons the instance (#3356), overlapped or not.
  await assert.rejects(m.peek_all(["x", "bad1", "y"]), /hooks\.js\.peek threw, but its @extern is infallible.*peek failed on bad1/);
  await assert.rejects(m.fetch_all(["k7"]), /abandoned after hooks\.js\.peek threw/);
  await m.init(undefined, { js });
  console.log("after init: " + (await m.fetch_all(["k7"])).join(","));
  console.log("fan overlap ok");
}
