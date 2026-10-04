// Host-side checks for hook_errors.almd (#3356): a throw out of a fallible
// hook is an err the Almide caller propagates (the JS caller sees
// AlmideError with the thrown message), failing calls leave memory flat,
// and a throw out of an infallible hook abandons the instance until init().
import assert from "node:assert/strict";

const fail = (key) => key.startsWith("bad");
export const js = {
  get(key) {
    if (fail(key)) throw new Error("no " + key);
    return "v-" + key;
  },
  count(key) {
    if (fail(key)) throw "count refused " + key;
    return key.length;
  },
  touch(key) {
    if (fail(key)) throw new TypeError("cannot touch " + key);
  },
  peek(key) {
    if (fail(key)) throw new Error("peek failed");
    return key;
  },
};

export async function after(m) {
  const isErr = (msg) => (e) => e instanceof m.AlmideError && e.message === msg;
  assert.equal(m.lookup("k"), "k=v-k");
  assert.throws(() => m.lookup("bad1"), isErr("no bad1"));
  assert.equal(m.total(["a", "bb", "ccc"]), 6);
  assert.throws(() => m.total(["a", "bad2", "c"]), isErr("count refused bad2"));
  assert.equal(m.mark("x"), "marked x");
  assert.throws(() => m.mark("bad3"), isErr("cannot touch bad3"));
  assert.equal(m.peek_twice("ab"), "abab");
  // Failing calls, many times over: memory must not move.
  const round = () => {
    for (let i = 0; i < 5; i++) {
      assert.throws(() => m.lookup("bad" + i));
      assert.throws(() => m.total(["a".repeat(50), "bad" + i]));
      assert.throws(() => m.mark("bad" + i));
      assert.equal(m.lookup("ok" + i), "o=v-ok" + i);
    }
  };
  for (let i = 0; i < 200; i++) round();
  const settled = m.memoryBytes();
  for (let i = 0; i < 3000; i++) round();
  assert.equal(m.memoryBytes(), settled, "memory grew across 3000 rounds of failing hooks");
  // An infallible hook that throws abandons the instance, naming the fix.
  assert.throws(() => m.peek_twice("bad"), /hooks\.js\.peek threw, but its @extern is infallible.*effect fn.*peek failed/);
  assert.throws(() => m.lookup("k"), /abandoned after hooks\.js\.peek threw — call init\(\) again/);
  await m.init(undefined, { js });
  assert.equal(m.lookup("k"), "k=v-k", "init() gives a fresh instance");
  console.log("hook errors ok");
}
