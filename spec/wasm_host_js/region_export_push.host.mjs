// Driver of region_export_push.almd (#4354, ADR-0031 amendment A1 / A4
// push). The glue wraps a Region export as `[ptr, len]` and gives
// `regionRelease(ptr)`; the raw export is checked once for the width of its
// two results. The glue gives the host no instance handle, so the instance
// is captured as it is created.
import assert from "node:assert/strict";

let raw = null;
const instantiate = WebAssembly.instantiate;
WebAssembly.instantiate = async (...args) => {
  const made = await instantiate.apply(WebAssembly, args);
  raw = (made.instance ?? made).exports;
  return made;
};

const say = (line) => process.stdout.write(`${line}\n`);
const text = (ptr, len) => new TextDecoder().decode(new Uint8Array(raw.memory.buffer, ptr, len));

export async function after(m) {
  const release = m.regionRelease;

  // The raw export answers (ptr, len) in the address width: two i32 on
  // wasm32, which JS reads as plain numbers.
  const [p0, n0] = raw.inbox_open(16n);
  assert.equal(typeof p0, "number", "the base crosses as an i32");
  assert.equal(typeof n0, "number", "the length crosses as an i32");
  raw.__region_release(p0);

  // Push: the host writes straight into the Region the export handed over.
  const [ptr, len] = m.inbox_open(16);
  assert.equal(len, 16, "the length is the Region's own");
  new Uint8Array(raw.memory.buffer, ptr, len).set(new TextEncoder().encode("host wrote this!"));
  m.inbox_show();

  // A sub-Region crosses as its own address inside the parent's block.
  const [sub, subLen] = m.inbox_tail(5);
  assert.equal(sub, ptr + 5, "a slice's base is its parent's base plus its offset");
  assert.equal(subLen, 11);
  say(`tail at +${sub - ptr}: ${text(sub, subLen)}`);

  // Lend a range back: an export taking a Region gets (base, len).
  assert.equal(m.digest(sub, subLen), 11);

  // The program replaces its copy and the host gives the parent's address
  // back: the slice's share alone keeps the block, so its bytes stay put
  // across allocations that would reuse a freed block.
  const [next] = m.inbox_open(16);
  release(ptr);
  for (let i = 0; i < 64; i++) {
    const [p] = m.inbox_open(16);
    new Uint8Array(raw.memory.buffer, p, 16).fill(0x2a);
    release(p);
  }
  say(`tail after the parent's release: ${text(sub, subLen)}`);
  release(sub);
  assert.throws(() => release(sub), WebAssembly.RuntimeError, "a second release of an address traps");
  say("a second release traps");

  // An empty Region crosses as (0, 0); releasing 0 is a no-op.
  const [z, zLen] = m.inbox_tail(16);
  assert.deepEqual([z, zLen], [0, 0]);
  release(0);

  // Every share handed out and released: memory settles.
  release(next);
  const churn = (n) => {
    for (let i = 0; i < n; i++) {
      const [p] = m.inbox_open(4096);
      release(p);
    }
  };
  churn(200);
  const settled = raw.memory.buffer.byteLength;
  churn(4000);
  assert.equal(raw.memory.buffer.byteLength, settled, "memory grew: a released share leaks");
  say("released shares free their blocks");
}
