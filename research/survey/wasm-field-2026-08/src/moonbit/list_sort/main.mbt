fn mk(n : Int64) -> Array[Int64] {
  let out : Array[Int64] = []
  let mut i : Int64 = 0
  while i < n {
    out.push(i * 7919 % 10007)
    i = i + 1
  }
  out
}

fn main {
  let xs = mk(2000)
  let mut acc : Int64 = 0
  let mut r : Int64 = 0
  while r < 300 {
    let s = xs.copy()
    s.sort()
    acc = acc + s[0] + s[1999]
    r = r + 1
  }
  println(acc)
}
