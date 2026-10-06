// Host-side checks for unmarked_promise.almd (#3371): a sync hook that
// returns a thenable is refused with the named fix, the instance is
// abandoned until init(), and a hook that answers directly is unaffected.
import assert from "node:assert/strict";

let unhandled = 0;
process.on("unhandledRejection", () => { unhandled++; });

export const js = {
  name_of: (id) => (id === 1 ? "ann" : Promise.resolve("bob")),
  load: (key) => (key === "now" ? "v-" + key : Promise.reject(new Error("late " + key))),
  ping: (n) => (n === 0 ? undefined : (async () => {})()),
};

let AlmideError = null;
const refused = (name) => (e) =>
  !(AlmideError && e instanceof AlmideError)
  && e.message === `almide: hooks.js.${name} returned a Promise; mark its @extern with returns: promise`;
const abandoned = (name) => new RegExp(`abandoned after hooks\\.js\\.${name} returned a Promise — call init\\(\\) again`);

export async function after(m) {
  AlmideError = m.AlmideError ?? null;
  assert.equal(m.greet(1), "hi ann");
  assert.equal(m.read("now"), "got v-now");
  m.poke(0);

  assert.throws(() => m.greet(2), refused("name_of"));
  assert.throws(() => m.greet(1), abandoned("name_of"));
  await m.init(undefined, { js });
  assert.equal(m.greet(1), "hi ann", "init() gives a fresh instance");

  // A fallible hook: the refusal passes through its catch, not into an err.
  assert.throws(() => m.read("later"), refused("load"));
  assert.throws(() => m.read("now"), abandoned("load"));
  await m.init(undefined, { js });

  // An async function hook on a Unit extern.
  assert.throws(() => m.poke(1), refused("ping"));
  await m.init(undefined, { js });
  m.poke(0);

  // The dropped promises (one rejects) raised no unhandled rejection.
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(unhandled, 0, "a refused promise surfaced as an unhandled rejection");
  console.log("unmarked promise ok");
}
