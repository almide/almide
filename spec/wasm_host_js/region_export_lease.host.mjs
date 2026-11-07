// Driver of region_export_lease.almd (ADR-0031 amendment A1d). The host is
// the downstream one: it reads each address-only export as one number, uses
// it at once, and never calls `__region_release`. Memory must settle anyway.
// The glue gives the host no instance handle, so the instance is captured as
// it is created.
import assert from "node:assert/strict";

let raw = null;
const instantiate = WebAssembly.instantiate;
WebAssembly.instantiate = async (...args) => {
  const made = await instantiate.apply(WebAssembly, args);
  raw = (made.instance ?? made).exports;
  return made;
};

const say = (line) => process.stdout.write(`${line}\n`);
const enc = new TextEncoder();
const dec = new TextDecoder();

// The downstream round trip, verbatim in shape: write the input at the
// in_alloc address, resolve, read the output at the out_ptr address.
const roundTrip = (s) => {
  const b = enc.encode(s);
  const p = raw.in_alloc(b.length);
  new Uint8Array(raw.memory.buffer, Number(p), b.length).set(b);
  const len = raw.resolve();
  const q = raw.out_ptr();
  return dec.decode(new Uint8Array(raw.memory.buffer, Number(q), Number(len)));
};

export async function after(m) {
  say(`round trip: ${roundTrip("hello")}`);

  // The lease: an address stays valid across calls of other exports, until
  // its own export's next call.
  const p = raw.in_alloc(5);
  new Uint8Array(raw.memory.buffer, p, 5).set(enc.encode("lease"));
  raw.resolve();
  raw.out_ptr();
  raw.resolve();
  assert.equal(dec.decode(new Uint8Array(raw.memory.buffer, p, 5)), "lease", "an address moved before its export's next call");
  say("an address holds until its export's next call");

  // The same address in the same block, handed out again: one share.
  const q1 = raw.out_ptr();
  const q2 = raw.out_ptr();
  assert.equal(q1, q2);
  say("an address handed out again holds one share");

  // Bounded memory with a host that never releases: warm up, then 10,000
  // more round trips of 4 KB each must not grow the memory.
  const big = "x".repeat(4096);
  for (let i = 0; i < 200; i++) assert.equal(roundTrip(big).length, 4101);
  const settled = raw.memory.buffer.byteLength;
  for (let i = 0; i < 10000; i++) roundTrip(big);
  const grown = raw.memory.buffer.byteLength - settled;
  process.stderr.write(`memory after warm-up ${settled} B, after 10000 more round trips ${settled + grown} B (grew ${grown} B)\n`);
  assert.equal(grown, 0, `memory grew ${grown} B over 10000 round trips: an unreleased address keeps its block`);
  say("10000 round trips without a release: memory settled");

  // A host may still end a lease early; the export then leases afresh.
  const r = m.in_alloc(8);
  m.regionRelease(r);
  const r2 = m.in_alloc(8);
  assert.notEqual(r2, 0);
  say("a released lease ends early, and the export leases again");

  // Last, since a trap ends the instance: an address its export superseded
  // is no longer the host's, so releasing it traps.
  m.in_alloc(8);
  assert.throws(() => m.regionRelease(r2), (e) => e instanceof m.AlmideExit || e instanceof WebAssembly.RuntimeError);
  say("releasing a superseded address traps");
}
