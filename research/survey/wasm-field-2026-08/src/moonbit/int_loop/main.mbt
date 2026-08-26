fn main {
  let mut i : Int64 = 0
  let mut acc : Int64 = 0
  while i < 30000000 {
    acc = (acc + i * 7) % 999983
    i = i + 1
  }
  println(acc)
}
