
fn main() -> Unit = {
  let doc = value.array(list.range(0, n) |> list.map(record))
  let text = json.stringify(doc)
  let back = json.parse(text) ?? value.null()
  let sum = (value.as_array(back) ?? []) |> list.map(score) |> list.sum
  println("${sum} ${string.len(text)}")
}
