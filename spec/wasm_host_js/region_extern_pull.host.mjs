// The hooks region_extern_pull.almd declares (#4354, ADR-0031 amendment A4
// pull). Each receives a lent Region as (base, len): two plain numbers on
// wasm32, the length the Region holds. The glue gives hooks no memory handle,
// so the instance's memory is captured as it is created.
import assert from "node:assert/strict";

let memory = null;
const instantiate = WebAssembly.instantiate;
WebAssembly.instantiate = async (...args) => {
  const made = await instantiate.apply(WebAssembly, args);
  memory = (made.instance ?? made).exports.memory;
  return made;
};

let filled = null;

export const js = {
  js_fill: (base, len, first) => {
    assert.equal(typeof base, "number");
    assert.equal(typeof len, "number");
    const view = new Uint8Array(memory.buffer, base, len);
    for (let i = 0; i < len; i++) view[i] = first + i;
    filled = base;
    return len;
  },
  js_sum: (base, len) => {
    assert.equal(base, filled + 2, "a slice arrives as its own sub-range");
    return new Uint8Array(memory.buffer, base, len).reduce((a, x) => a + x, 0);
  },
};
