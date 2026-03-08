# Control Flow Extensions

## while loop
Currently `do { guard cond else ok(()); ... }` is used as a workaround, but it is verbose.

```almide
// proposed
while n > 0 {
  n = n - 1
}

// current workaround
do {
  guard n > 0 else ok(())
  n = n - 1
}
```

## break / continue
Early exit from for-in loops is not supported.

```almide
for x in items {
  if x == target then break
  process(x)
}
```

guard can partially substitute, but there is no equivalent to continue.

## early return
Since function bodies are single expressions, returning mid-way requires nested guards or match.

```almide
// proposed
fn find(items: List[Int], target: Int) -> Option[Int] = {
  for i in items {
    if i == target then return some(i)
  }
  none
}
```

## for-range
Index-based loops are verbose.

```almide
// proposed
for i in 0..10 { ... }

// current: list.fold + counter or var + do loop
```

## Priority
while > for-range > break/continue > early return
