# secret

A value every renderer hides: `Secret[A]`. `import secret` (never
auto-imported, so a local named `secret` stays yours elsewhere).

A password, an API key or a session token put in a `Secret` keeps its
protection through every binding, function, list and record: interpolation,
`println` of an enclosing value, an `assert_eq` / `assert_ne` failure block, an
abort message and the `almide test` report all print `Secret(***)`, whatever
`A` is. The only way to read the value back is a visible `secret.reveal(s)`.

- **Opaque.** Outside this module a Secret's field cannot be read and its
  record literal cannot be written (E033); `secret.new` and `secret.reveal`
  are the way in and out.
- **Equality.** `==` / `!=` on `Secret[String]` and `Secret[Bytes]` compare in
  constant time (`string.is_equal_constant_time` /
  `bytes.is_equal_constant_time`), including a Secret field inside a record,
  tuple, list, option or result being compared. A Secret of any other type has
  no `==` (E103): compare `secret.reveal(a) == secret.reveal(b)` explicitly. A
  variant or map holding a Secret has no `==` either.
- **Codec.** A `: Codec` record with a `Secret[A]` field derives `decode` (the
  field reads A's wire form) and never derives `encode`: `T.encode(x)` and
  `json.encode(x)` are E023. Build the outgoing `Value` by hand and reveal the
  one field you mean to send. `Secret[Bytes]` fields are E023 like `Bytes`.

Every value prints structurally unless its type declares a repr
(`fn T.repr`); `Secret` is the stdlib type that declares one.

Every function is pure, so it behaves identically on both targets.

### `secret.new[A](x: A) -> Secret[A]`

Wrap `x`. From here on it prints as `Secret(***)`.

```almd check
import secret

type Login = { user: String, password: Secret[String] }

fn main() -> Unit = {
  let l = Login { user: "ana", password: secret.new("hunter2") }
  println("${l}")
}
```

### `secret.reveal[A](s: Secret[A]) -> A`

The wrapped value. Every read of a secret goes through this call, so a review
(or a grep) finds each place one leaves its wrapper.

```almd check
import secret

fn main() -> Unit = {
  let key = secret.new("k-123")
  let given = secret.new("k-123")
  println(if key == given then "accepted" else "refused")
  println(string.len(secret.reveal(key)))
}
```

### Decoding into a Secret

```almd check
import json
import secret

type LoginInput: Codec = { user: String, password: Secret[String] }

effect fn main() -> Unit = {
  let input = LoginInput.decode(json.parse("{\"user\":\"ana\",\"password\":\"hunter2\"}")!)!
  println("${input}")
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (2 functions)

```
// x wrapped so that every renderer prints it as `Secret(***)`.
// @since unreleased
secret.new(x: A) -> Secret[A]

// The value s wraps.
// @since unreleased
secret.reveal(s: Secret[A]) -> A
```

<!-- END GENERATED SIGNATURE INDEX -->
