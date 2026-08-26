function mk(n: i64): Array<i64> {
  const out = new Array<i64>();
  let i: i64 = 0;
  while (i < n) {
    out.push((i * 7919) % 10007);
    i += 1;
  }
  return out;
}

const xs = mk(2000);
let acc: i64 = 0;
let r: i64 = 0;
while (r < 300) {
  const s = xs.slice(0);
  s.sort();
  acc = acc + s[0] + s[1999];
  r += 1;
}
console.log(acc.toString());
