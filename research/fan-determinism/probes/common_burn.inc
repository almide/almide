fn burn(n: Int) -> Int = {
  var acc = 0
  var i = 0
  while i < n {
    acc = (acc * 31 + i) % 1000003
    i = i + 1
  }
  acc
}
