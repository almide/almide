fn main {
  let mut i : Int64 = 0
  let mut n : Int64 = 0
  while i < 3000000 {
    let s = "ab" + (i % 100).to_string()
    n = n + s.length().to_int64()
    i = i + 1
  }
  println(n)
}
