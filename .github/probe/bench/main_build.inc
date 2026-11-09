
fn main() -> Unit = {
  let doc = value.array(list.range(0, n) |> list.map(record))
  let k = (value.as_array(doc) ?? []) |> list.map(score) |> list.sum
  println("${k}")
}
