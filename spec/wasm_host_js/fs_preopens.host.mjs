// Host side of fs_preopens.almd (#4438): the preopens, argv and environment
// `init()` hands the module, then the exports against them and `files()`.
import assert from "node:assert/strict";

export const init = {
  args: ["fs_preopens", "-o", "/out/納品", "--date", "2026-10-27"],
  env: { GREETING: "こんにちは" },
  preopens: {
    ".": { writable: true },
    "/in": { files: { "seed.txt": "seed", "sub/b.txt": new Uint8Array([1]), "sub/a.txt": "", sub: { c: {} } } },
  },
};

export async function after(m) {
  console.log(m.probe_in());
  console.log(m.probe_args());
  // What `main` wrote, read back; folders are paths, empty ones are not listed.
  const out = m.files(".");
  assert.deepEqual([...out.keys()], ["work/big.txt", "work/deep/b.txt"]);
  assert.equal(new TextDecoder().decode(out.get("work/deep/b.txt")), "héllo\nwörld\n");
  assert.equal(out.get("work/big.txt").length, 1048576);
  assert.deepEqual([...m.files("/in").keys()], ["seed.txt", "sub/a.txt", "sub/b.txt"]);
  assert.throws(() => m.files("/nope"), /no preopen named "\/nope"/);
  // A write never reaches the bytes the caller passed: the file is copied on its first write.
  const mine = new TextEncoder().encode("abc");
  await m.init(undefined, { preopens: { ".": { files: { "n.txt": mine }, writable: true } } });
  m.append_note("n.txt");
  assert.equal(new TextDecoder().decode(m.files(".").get("n.txt")), "abc!");
  assert.equal(new TextDecoder().decode(mine), "abc");
  // A tree the module could not see as given is refused by init(), before any run.
  for (const files of [{ "../x": "y" }, { "a/./b": "y" }, { a: "y", "a/b": "z" }, { a: 7 }]) {
    await assert.rejects(m.init(undefined, { preopens: { ".": { files } } }), TypeError, JSON.stringify(files));
  }
}
