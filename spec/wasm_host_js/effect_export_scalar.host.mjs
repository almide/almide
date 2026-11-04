// Driver of effect_export_scalar.almd (#4376). Each scalar effect export is
// called raw, as any wasm host calls it, and through the glue: the raw
// result is the ok value itself (an Int as a BigInt, a Float as a number, a
// Bool as an i32, nothing for Unit), and the glue reads it as a plain fn's.
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

// What the module writes to stderr while `f` runs.
function stderrOf(f) {
  const write = process.stderr.write;
  let text = "";
  process.stderr.write = (chunk, ...rest) => {
    text += typeof chunk === "string" ? chunk : new TextDecoder().decode(chunk);
    return true;
  };
  try {
    f();
  } finally {
    process.stderr.write = write;
  }
  return text;
}

const traps = (e) => e instanceof WebAssembly.RuntimeError || (e && e.name === "AlmideExit");

export async function after(m) {
  // The issue's repro: the host writes 42 into the Region `put` handed out,
  // and the raw `first()` answers 42, not a Result block address.
  const [ptr, len] = raw.put(4n);
  new Uint8Array(raw.memory.buffer, ptr, len).set([42, 1, 2, 3]);
  const v = raw.first();
  assert.equal(typeof v, "bigint", "an Int crosses as the i64 itself");
  assert.equal(v, 42n, "the raw export answers the value");
  assert.equal(m.first(), 42);
  say(`raw first: ${v}`);

  // Every scalar, raw and through the glue.
  assert.equal(raw.scaled(5n), 15n);
  assert.equal(m.scaled(5), 15);
  assert.equal(raw.half(5n), 2.5);
  assert.equal(m.half(5), 2.5);
  assert.equal(raw.even(4n), 1, "a Bool crosses as an i32");
  assert.equal(raw.even(3n), 0);
  assert.equal(m.even(4), true);
  assert.equal(raw.note(3n), undefined, "a Unit export has no result");
  assert.equal(m.note(4), undefined);

  // A Region the host lends as (base, len), and an Int back.
  assert.equal(raw.total(ptr, len), 48n);
  assert.equal(m.total(ptr, len), 48);
  say(`lent total: ${m.total(ptr, len)}`);
  raw.__region_release(ptr);

  // Each call frees its Result block: memory settles.
  const churn = (n) => {
    for (let i = 0; i < n; i++) {
      raw.scaled(BigInt(i % 100));
      raw.half(7n);
      m.even(i % 100);
    }
  };
  churn(500);
  const settled = raw.memory.buffer.byteLength;
  churn(20000);
  assert.equal(raw.memory.buffer.byteLength, settled, "memory grew: a scalar export leaks its Result block");
  say("scalar exports free their Result blocks");

  // Last, since a trap ends the instance: an err writes its message to
  // stderr and traps, on the raw export and through the glue alike.
  const rawErr = stderrOf(() => assert.throws(() => raw.scaled(101n), traps));
  assert.match(rawErr, /over 100: 101/, `the raw err's message: ${JSON.stringify(rawErr)}`);
  const glueErr = stderrOf(() => assert.throws(() => m.half(102), traps));
  assert.match(glueErr, /over 100: 102/, `the glue err's message: ${JSON.stringify(glueErr)}`);
  say("an err traps with its message on stderr");
}
