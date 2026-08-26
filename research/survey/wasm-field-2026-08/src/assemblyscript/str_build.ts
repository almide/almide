let i: i64 = 0;
let n: i64 = 0;
while (i < 3000000) {
  const s = "ab" + (i % 100).toString();
  n += s.length;
  i += 1;
}
console.log(n.toString());
