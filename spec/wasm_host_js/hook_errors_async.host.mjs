// Host-side checks for hook_errors_async.almd (#3356): the async twin of
// hook_errors — a rejection out of a fallible hook is an err the Almide
// caller propagates, failing calls leave memory flat, and a rejection out
// of an infallible hook abandons the instance until init().
import assert from "node:assert/strict";

const fail = (key) => key.startsWith("bad");
const tick = () => new Promise((r) => setTimeout(r, 0));
export const js = {
  async get(key) {
    await tick();
    if (fail(key)) throw new Error("no " + key);
    return "v-" + key;
  },
  async count(key) {
    await tick();
    if (fail(key)) throw "count refused " + key;
    return key.length;
  },
  async touch(key) {
    await tick();
    if (fail(key)) throw new TypeError("cannot touch " + key);
  },
  async peek(key) {
    await tick();
    if (fail(key)) throw new Error("peek failed");
    return key;
  },
};

export async function after(m) {
  const isErr = (msg) => (e) => e instanceof m.AlmideError && e.message === msg;
  assert.equal(await m.lookup("k"), "k=v-k");
  await assert.rejects(m.lookup("bad1"), isErr("no bad1"));
  assert.equal(await m.total(["a", "bb", "ccc"]), 6);
  await assert.rejects(m.total(["a", "bad2", "c"]), isErr("count refused bad2"));
  assert.equal(await m.mark("x"), "marked x");
  await assert.rejects(m.mark("bad3"), isErr("cannot touch bad3"));
  assert.equal(await m.peek_twice("ab"), "abab");
  const round = async () => {
    for (let i = 0; i < 3; i++) {
      await assert.rejects(m.lookup("bad" + i));
      await assert.rejects(m.total(["a".repeat(50), "bad" + i]));
      await assert.rejects(m.mark("bad" + i));
      assert.equal(await m.lookup("ok" + i), "o=v-ok" + i);
    }
  };
  for (let i = 0; i < 100; i++) await round();
  const settled = m.memoryBytes();
  for (let i = 0; i < 600; i++) await round();
  assert.equal(m.memoryBytes(), settled, "memory grew across 600 rounds of rejecting hooks");
  await assert.rejects(m.peek_twice("bad"), /hooks\.js\.peek threw, but its @extern is infallible.*peek failed/);
  await assert.rejects(m.lookup("k"), /abandoned after hooks\.js\.peek threw/);
  await m.init(undefined, { js });
  assert.equal(await m.lookup("k"), "k=v-k");
  console.log("hook errors ok");
}
