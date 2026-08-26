fn main {
  let out : Array[Int64] = []
  let mut i : Int64 = 0
  while i < 2000 {
    out.push(i)
    i = i + 1
  }
  let mut acc : Int64 = 0
  let mut r : Int64 = 0
  while r < 2000 {
    let m = out.map(fn(x) { x * 3 + 1 })
    let f = m.filter(fn(x) { x % 2 == 0 })
    let v = f.fold(init=0L, fn(a, x) { (a + x) % 999983 })
    acc = (acc + v) % 999983
    r = r + 1
  }
  println(acc)
}
