## Almide Examples

```almide
// Record + function
type User = { name: String, age: Int }

fn greeting(user: User) -> String =
  "Hello, ${user.name}! Age: ${int.to_string(user.age)}."

// List operations with pipe
fn active_names(users: List[User]) -> List[String] =
  users
    |> list.filter((u) => u.age >= 18)
    |> list.map((u) => u.name)
    |> list.sort()

// Pattern matching
fn describe(opt: Option[Int]) -> String = match opt {
  some(n) => "Got ${int.to_string(n)}",
  none => "Nothing",
}
```
