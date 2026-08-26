function go(n: i64, acc: i64): i64 {
  return n <= 0 ? acc : go(n - 1, (acc + n) % 999983);
}

let r: i64 = 0;
let acc: i64 = 0;
while (r < 30) {
  acc = (acc + go(1000000, 0)) % 999983;
  r += 1;
}
console.log(acc.toString());
