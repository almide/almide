// Driver of region_export_address.almd (#4519, ADR-0031 amendment A1c). A
// `returns: address` export answers one number, the address, both as the raw
// export and through the glue, which gives `regionRelease(ptr)` as for the
// `(ptr, len)` form; each export's address is its lease (A1d). The glue gives the host no instance handle, so the
// instance is captured as it is created.
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
const enc = new TextEncoder();

export async function after(m) {
  const release = m.regionRelease;

  // The raw export answers the address alone, one i32 that JS reads as a
  // number. A host that knows the length it asked for writes straight there.
  const b = enc.encode("host wrote this!");
  const p = raw.in_alloc(b.length);
  assert.equal(typeof p, "number", "the address crosses as one i32");
  assert.notEqual(p, 0, "a non-empty Region has a nonzero address");
  new Uint8Array(raw.memory.buffer, Number(p), b.length).set(b);
  m.in_show();
  release(p);

  // The glue wraps it as the same one number.
  const q = m.in_alloc(5);
  assert.equal(typeof q, "number", "the glue answers one number");
  new Uint8Array(raw.memory.buffer, q, 5).set(enc.encode("again"));
  m.in_show();

  // A sub-Region crosses as its own address inside its parent's block.
  const t = m.in_tail(2);
  assert.equal(t, q + 2, "a slice's address is its parent's plus its offset");
  say(`tail at +${t - q}: ${text(t, 3)}`);

  // The program replaces its copy, and in_alloc's next call ends the
  // parent's lease: the slice's lease alone keeps the block, so its bytes
  // stay put across allocations that would reuse a freed block.
  m.in_alloc(16);
  for (let i = 0; i < 64; i++) {
    const r = m.in_alloc(5);
    new Uint8Array(raw.memory.buffer, r, 5).fill(0x2a);
    release(r);
  }
  say(`tail after the parent's release: ${text(t, 3)}`);
  release(t);

  // An empty Region crosses as 0; releasing 0 is a no-op.
  assert.equal(m.in_alloc(0), 0);
  assert.equal(raw.in_alloc(0), 0);
  release(0);
  say("an empty Region crosses as 0");

  // Every share handed out and released: memory settles.
  const churn = (n) => {
    for (let i = 0; i < n; i++) release(m.in_alloc(4096));
  };
  churn(200);
  const settled = raw.memory.buffer.byteLength;
  churn(4000);
  assert.equal(raw.memory.buffer.byteLength, settled, "memory grew: a released share leaks");
  say("released shares free their blocks");

  // Last, since a trap ends the instance: an effect fn's err has no slot in
  // the one result, so it traps (the WASI build reports it as exit 1).
  assert.throws(() => raw.in_alloc(-1), (e) => e instanceof m.AlmideExit || e instanceof WebAssembly.RuntimeError);
  say("an err traps");
}
