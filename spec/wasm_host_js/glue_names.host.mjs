// Host-side checks for glue_names.almd (#4943): init() took the node path
// (a `process` export no longer shadows the global it checks), and every
// export is the wrapper of its own pub fn and returns its value.
import assert from "node:assert/strict";

export async function after(m) {
  assert.equal(m.process("x"), "processed x");
  assert.equal(m.view(1), 2);
  assert.equal(m.hooks(1), 3);
  assert.equal(m.imports(1), 4);
  assert.equal(m.flush(1), 5);
  assert.equal(m.fetch(1), 7);
  assert.equal(m.instance(1), 8);
  assert.equal(m.ready(1), 9);
  assert.equal(m.readString("héllo"), 5);
  assert.equal(m.delete(10, 3), 7);
  // The platform globals the glue reads are the platform's.
  assert.equal(typeof globalThis.process.versions.node, "string");
  assert.equal(typeof globalThis.fetch, "function");
  console.log("host ok");
}
