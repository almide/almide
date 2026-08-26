const out = new Array<i64>();
let i: i64 = 0;
while (i < 2000) {
  out.push(i);
  i += 1;
}
let acc: i64 = 0;
let r: i64 = 0;
while (r < 2000) {
  const m = out.map<i64>((x: i64): i64 => x * 3 + 1);
  const f = m.filter((x: i64): bool => x % 2 == 0);
  const v = f.reduce<i64>((a: i64, x: i64): i64 => (a + x) % 999983, 0);
  acc = (acc + v) % 999983;
  r += 1;
}
console.log(acc.toString());
