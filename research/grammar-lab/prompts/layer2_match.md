## Almide Examples — Option Handling (match style)

```almide
// Record + function
type User = { name: String, age: Int }

// Option handling with match
fn find_age(users: List[User], target: String) -> Int =
  match list.find(users, (u) => u.name == target) {
    some(u) => u.age,
    none => -1,
  }

// Pipe into match
fn youngest_name(users: List[User]) -> String =
  users
    |> list.sort_by((u) => u.age)
    |> list.first()
    |> match {
      some(u) => u.name,
      none => "nobody",
    }

// Nested match for chained lookups
fn greet_oldest(users: List[User]) -> String =
  match list.first(users |> list.sort_by((u) => u.age) |> list.reverse()) {
    some(u) => "Hello, ${u.name}!",
    none => "No users",
  }
```
