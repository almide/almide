// The hooks extern_js.almd declares. `js_log` writes through the same fd as
// the guest's println so the two interleave in program order.
export const js = {
  js_log: (msg) => process.stdout.write(msg + "\n"),
  js_add: (a, b) => a + b,
  js_upper: (s) => s.toUpperCase(),
};

export async function after(m) {
  const assert = (await import("node:assert/strict")).default;
  assert.equal(m.greet(10), 55);
}
