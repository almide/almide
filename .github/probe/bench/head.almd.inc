import json

let n = 50000

fn record(i: Int) -> Value = value.object([
  ("id", value.int(i)),
  ("name", value.string("user_${i}")),
  ("tags", value.array([value.string("t${i % 7}"), value.string("t${i % 11}")])),
  ("meta", value.object([
    ("score", value.int((i * 37) % 1000)),
    ("active", value.bool(i % 2 == 0)),
  ])),
])

fn score(r: Value) -> Int =
  value.get_int(value.get(r, "meta") ?? value.null(), "score") ?? 0
