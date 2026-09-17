// The hooks extern_scalars.almd declares — one per host-ABI signature class.
export const js = {
  js_half: (x) => x / 2,
  js_not: (b) => !b,
  js_note: (tag, n) => process.stdout.write(`${tag}=${n}\n`),
  js_repeat: (s, n) => s.repeat(n),
};

export async function after(m) {
  const assert = (await import("node:assert/strict")).default;
  assert.equal(m.twice("z"), "zz");
}
