let i: i64 = 0;
let acc: i64 = 0;
while (i < 30000000) {
  acc = (acc + i * 7) % 999983;
  i += 1;
}
console.log(acc.toString());
