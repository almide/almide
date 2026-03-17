## Almide Examples — Option Handling (combinator style)

```almide
// Record + function
type User = { name: String, age: Int }

// Option handling with combinators
fn find_age(users: List[User], target: String) -> Int =
  list.find(users, (u) => u.name == target)
    |> option.map((u) => u.age)
    |> option.unwrap_or(-1)

// Pipeline with option.map
fn youngest_name(users: List[User]) -> String =
  users
    |> list.sort_by((u) => u.age)
    |> list.first()
    |> option.map((u) => u.name)
    |> option.unwrap_or("nobody")

// Chained lookups with option.map
fn greet_oldest(users: List[User]) -> String =
  users
    |> list.sort_by((u) => u.age)
    |> list.reverse()
    |> list.first()
    |> option.map((u) => "Hello, ${u.name}!")
    |> option.unwrap_or("No users")
```
