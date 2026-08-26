fn go(n : Int64, acc : Int64) -> Int64 {
  if n <= 0 {
    acc
  } else {
    go(n - 1, (acc + n) % 999983)
  }
}

fn main {
  let mut r : Int64 = 0
  let mut acc : Int64 = 0
  while r < 30 {
    acc = (acc + go(1000000, 0)) % 999983
    r = r + 1
  }
  println(acc)
}
