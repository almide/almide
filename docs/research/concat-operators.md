# Concatenation Operators Across Languages

Research into how 13 languages handle string concatenation and list/array concatenation, with design rationale, tradeoffs, and community feedback.

## Summary Table

| Language | String Concat | List/Array Concat | Same Op? | Interpolation | Notable |
|---|---|---|---|---|---|
| Rust | `+`, `format!` | `extend`, `append`, `.concat()` | No | No native (format!) | `+` consumes LHS |
| TypeScript/JS | `+`, template literals | `concat()`, `[...a, ...b]` | Partial (`+` for strings only) | Yes (`${}`) | `+` overloaded with addition |
| Go | `+` | `append()` | No | No native (fmt.Sprintf) | `+` for strings only; slices use builtin |
| Python | `+` | `+` | Yes | Yes (f-strings) | Quadratic perf warning |
| Elm | `++` | `++` | Yes | No | Polymorphic over appendables |
| Haskell | `++`, `<>` | `++`, `<>` | Yes | No | `++` is list-level; `<>` is Semigroup |
| Gleam | `<>` | `list.append` | No | No | Operator for strings, function for lists |
| Kotlin | `+` | `+` | Yes | Yes (`${}`) | Returns new immutable collection |
| Swift | `+` | `+` | Yes | Yes (`\()`) | Works for String, Array, etc. |
| OCaml | `^` | `@` | No | No native (Printf.sprintf) | Distinct operators, no ambiguity |
| Elixir | `<>` | `++` | No | Yes (`#{}`) | `<>` is binary concat; `++` is list concat |
| Ruby | `+`, `<<` | `+`, `concat` | Yes (`+`) | Yes (`#{}`) | `<<` mutates; `+` copies |
| Clojure | `str` | `concat`, `into` | No (both functions) | No | No operators; everything is a function call |

## Detailed Analysis

---

### 1. Rust

**String concatenation:**
- `+` operator via `Add<&str> for String`. Consumes the left operand (ownership transfer), borrows the right.
  ```rust
  let s = s1 + &s2;   // s1 is moved, s2 is borrowed
  ```
- `+=` via `AddAssign<&str>` for in-place append.
- `push_str(&str)` for mutable append without operator syntax.
- `format!("{}{}", a, b)` for non-destructive concatenation (most idiomatic for multi-part).

**Vec concatenation:**
- `vec.extend(iter)` -- most flexible, works with any iterator.
- `vec1.append(&mut vec2)` -- drains vec2 into vec1.
- `[v1, v2].concat()` -- on slices of slices, produces a new Vec.
- No `+` operator for Vec.

**Same operator?** No. `+` works for String but not Vec. This is deliberate: Rust's ownership model makes generic `+` on collections semantically hazardous (which side is consumed?).

**Interpolation:** No native string interpolation. `format!()` macro is the standard substitute. Several RFCs have proposed interpolation (e.g., RFC #3475 "Unified String Literals", still open as of 2025) but none have been accepted.

**Design feedback:**
- The `+` operator for String is widely considered ergonomically poor. It consumes the left operand, so chaining like `s1 + &s2 + &s3` requires the first to be an owned String and all subsequent to be references. Beginners find this confusing.
- RFC #203 proposed a `Combine` trait with `++` operator for non-commutative concatenation (distinguishing from commutative `+`). It was closed in 2015 due to inactivity and concerns about importing too much abstract algebra into the type system.
- The community consensus is: use `format!()` for readability, `push_str` for performance, and avoid `+` except in simple cases.

---

### 2. TypeScript / JavaScript

**String concatenation:**
- `+` operator, overloaded with numeric addition. If either operand is a string, the other is coerced to string.
  ```js
  "hello" + " " + "world"   // "hello world"
  5 + "px"                   // "5px"
  ```
- Template literals (ES6+): `` `${expr}` `` -- the idiomatic modern approach.
- `"".concat(a, b)` -- rarely used but avoids `+`'s coercion via `valueOf()`.

**Array concatenation:**
- `arr1.concat(arr2)` -- returns new array, non-mutating.
- `[...arr1, ...arr2]` -- spread syntax (ES6+), more idiomatic in modern code.
- No `+` operator for arrays (`[1,2] + [3,4]` produces `"1,23,4"` -- a string! Both arrays are coerced to strings and concatenated).

**Same operator?** No. `+` is string-only. Arrays require `.concat()` or spread. The `+` on arrays is a well-known footgun.

**Interpolation:** Yes. Template literals (`` `text ${expr}` ``) have largely replaced `+` for string building in modern JS/TS. ESLint rule `prefer-template` enforces this in many codebases.

**Design feedback:**
- `+` being overloaded for both addition and concatenation is JavaScript's most criticized design decision. `"2" + 2 === "22"` but `"2" - 2 === 0`. The asymmetry produces endless bugs.
- MDN explicitly advises against `"" + x` for string coercion due to `valueOf()` vs `toString()` inconsistencies (e.g., `Temporal` objects throw on `+` but work with template literals).
- TypeScript mitigates some issues through type checking but cannot prevent all coercion surprises at the operator level.
- `[1,2] + [3,4] === "1,23,4"` is universally regarded as a language wart.

---

### 3. Go

**String concatenation:**
- `+` operator, defined for string types in the spec. Creates a new string (strings are immutable).
  ```go
  s := "hello" + " " + "world"
  ```
- `fmt.Sprintf("%s%s", a, b)` for formatted concatenation.
- `strings.Builder` for high-performance repeated concatenation (avoids quadratic allocation).
- `strings.Join(slice, sep)` for joining string slices.

**Slice concatenation:**
- `append(s1, s2...)` -- builtin function, variadic.
  ```go
  result := append(s1, s2...)
  ```
- No operator for slice concatenation. `+` is not defined for slices.
- `slices.Concat(s1, s2)` added in Go 1.22 (2024) as a cleaner alternative.

**Same operator?** No. `+` for strings only; `append` builtin for slices.

**Interpolation:** No native string interpolation. `fmt.Sprintf` is the standard approach. This is a deliberate design choice: Go avoids implicit complexity in string literals.

**Design feedback:**
- The lack of string interpolation is a frequent complaint. Multiple proposals have been discussed and rejected; the Go team considers `Sprintf` adequate and prefers explicit formatting.
- `append` returning a new slice (potentially with a new backing array) is a source of bugs for beginners who expect mutation.
- `slices.Concat` (Go 1.22) was added partly because `append(s1, s2...)` is verbose and the `...` is easy to forget.

---

### 4. Python

**String concatenation:**
- `+` operator. Both operands must be strings (no implicit coercion, unlike JS).
  ```python
  "hello" + " " + "world"
  ```
- `"".join(iterable)` -- idiomatic for multiple strings (O(n) vs O(n^2) for repeated `+`).
- f-strings (PEP 498, Python 3.6): `f"Hello {name}"` -- the modern standard.

**List concatenation:**
- `+` operator. Returns a new list.
  ```python
  [1, 2] + [3, 4]   # [1, 2, 3, 4]
  ```
- `list.extend(iterable)` -- in-place append.
- `[*a, *b]` -- unpack syntax (Python 3.5+).

**Same operator?** Yes. `+` works for both strings and lists (and tuples). It always returns a new object. Operands must be the same type (`"a" + [1]` is a TypeError).

**Interpolation:** Yes. f-strings have dramatically reduced the use of `+` for string building. PEP 498 motivation: "The existing ways of formatting are either error prone, inflexible, or cumbersome." Before f-strings, Python had `%`-formatting, `str.format()`, and `+`. The community strongly prefers f-strings for anything beyond trivial concatenation.

**Design feedback:**
- The official docs explicitly warn about quadratic runtime when building strings with repeated `+` in a loop. CPython has an optimization (in-place append when refcount == 1) but it's an implementation detail, not guaranteed.
- `+` requiring same types on both sides (no `"x" + 5`) is widely considered the right call -- it avoids JS-style coercion bugs.
- The Zen of Python ("There should be one obvious way to do it") is somewhat violated by having `+`, `join()`, f-strings, `format()`, and `%` all available. But each serves a distinct use case.

---

### 5. Elm

**String concatenation:**
- `++` operator. Works on `String` type (which is internally `appendable`).
  ```elm
  "hello" ++ " " ++ "world"
  ```

**List concatenation:**
- `++` operator. Works on `List a`.
  ```elm
  [1, 2] ++ [3, 4]   -- [1, 2, 3, 4]
  ```
- `::` (cons) for prepending a single element: `1 :: [2, 3]`.

**Same operator?** Yes. `++` is defined for the `appendable` type class, which includes both `String` and `List a`. This is one of Elm's constrained type classes (not user-extensible).

**Interpolation:** No. Elm has no string interpolation. You must use `++` or `String.concat` / `String.join`. This is consistent with Elm's philosophy of explicitness and simplicity.

**Design feedback:**
- `++` for both types is generally well-received. The `appendable` constraint means you can write generic functions over anything concatenable.
- The lack of interpolation is occasionally criticized but accepted as part of Elm's minimalist design. The `elm-format` tool normalizes `++` chains.
- Elm deliberately avoids `+` for strings to prevent the "is it addition or concatenation?" ambiguity.

---

### 6. Haskell

**String concatenation:**
- `++` operator. Since `String = [Char]`, this is just list append.
  ```haskell
  "hello" ++ " " ++ "world"
  ```
- `<>` operator (via `Semigroup` typeclass). Works on `String`, `Text`, `ByteString`, and any Semigroup.
  ```haskell
  "hello" <> " " <> "world"
  ```
- For performance: `Data.Text` or `Data.Text.Lazy` with `<>` or `Text.append`.

**List concatenation:**
- `++` operator (same as string, since strings are lists).
  ```haskell
  [1, 2] ++ [3, 4]
  ```
- `<>` also works on lists (List is a Semigroup/Monoid).
- `concat :: [[a]] -> [a]` for flattening nested lists.

**Same operator?** Yes. Both `++` and `<>` work for strings and lists. `++` is the traditional list operator; `<>` is the more general Semigroup operator that works on any Semigroup instance.

**Interpolation:** No native interpolation. Libraries like `string-interpolate` and `interpolatedstring-perl6` exist but require language extensions.

**Design feedback:**
- `++` on `String` ([Char]) has O(n) performance on the left operand, leading to O(n^2) for left-associated chains. This is a well-known issue.
- The community has largely moved to `<>` as the preferred concatenation operator because it's more general (works with Text, ByteString, etc.) and communicates "monoidal append" clearly.
- `String` as `[Char]` (linked list of characters) is widely considered Haskell's biggest historical design mistake. The 4-words-per-character overhead is enormous. `Data.Text` is the de facto standard for real applications.
- The existence of both `++` and `<>` creates choice paralysis for beginners, though `<>` is now the clear recommendation.

---

### 7. Gleam

**String concatenation:**
- `<>` operator (borrowed from Elixir/Erlang heritage).
  ```gleam
  "hello" <> " " <> "world"
  ```

**List concatenation:**
- `list.append(list1, list2)` -- function call, no operator.
  ```gleam
  list.append([1, 2], [3, 4])   // [1, 2, 3, 4]
  ```
- `list.flatten([[1, 2], [3, 4]])` for nested lists.
- `[x, ..rest]` spread syntax for prepending (in patterns and expressions).

**Same operator?** No. `<>` is string-only; lists use stdlib functions. This is a deliberate asymmetry: Gleam's type system doesn't support ad-hoc polymorphism (no typeclasses), so the operator can only be defined for one type.

**Interpolation:** No. Gleam has no string interpolation. Concatenation with `<>` or `string.concat` / `string.join` is the only way.

**Design feedback:**
- `<>` being string-only (unlike Elixir where `<>` is binary concat and `++` is list concat) is occasionally questioned, but it follows from Gleam's "no typeclasses, no overloading" philosophy.
- The lack of a list concat operator means `list.append(a, b)` is more verbose than Elixir's `a ++ b`. This is accepted as the cost of simplicity.
- No string interpolation is the most frequently requested missing feature in community discussions.

---

### 8. Kotlin

**String concatenation:**
- `+` operator. First operand must be a String; the other is converted via `toString()`.
  ```kotlin
  "hello" + " " + "world"
  "value: " + 42   // "value: 42"
  ```
- String templates: `"Hello $name"` or `"result: ${expr}"` -- strongly preferred.

**List concatenation:**
- `+` operator. Returns a new read-only collection.
  ```kotlin
  listOf(1, 2) + listOf(3, 4)   // [1, 2, 3, 4]
  listOf(1, 2) + 3              // [1, 2, 3]
  ```
- `-` operator for element removal (returns new collection).
- `+=` for `var` reassignment or mutable collection mutation.

**Same operator?** Yes. `+` works for both String and List (and Set, Map). Returns a new immutable collection in all cases.

**Interpolation:** Yes. String templates (`$var`, `${expr}`) are the idiomatic approach. The official docs state: "In most cases using string templates or multiline strings is preferable to string concatenation."

**Design feedback:**
- Kotlin's `+` for collections is well-regarded because it always returns a new immutable collection, making the semantics predictable.
- The `+` for strings requiring the first operand to be a String (`42 + "px"` does not compile) avoids JavaScript-style coercion surprises.
- String templates have largely eliminated the need for `+` in string building. IntelliJ even suggests converting `+` chains to templates automatically.

---

### 9. Swift

**String concatenation:**
- `+` operator (via conformance to `StringProtocol`).
  ```swift
  let s = "hello" + " " + "world"
  ```
- `+=` for in-place append.
- `append(_:)` method.
- String interpolation: `"Hello \(name)"` -- the primary mechanism.

**Array concatenation:**
- `+` operator (via `RangeReplaceableCollection` conformance).
  ```swift
  let combined = [1, 2] + [3, 4]   // [1, 2, 3, 4]
  ```
- `+=` for in-place extend.
- `append(contentsOf:)` method.

**Same operator?** Yes. `+` works for String, Array, and any `RangeReplaceableCollection`. This is enabled by Swift's protocol-oriented design -- `+` is defined on the protocol, not individual types.

**Interpolation:** Yes. `"\(expr)"` is deeply integrated. Custom interpolation via `ExpressibleByStringInterpolation` protocol allows domain-specific formatting. The community strongly prefers interpolation over `+` for string building.

**Design feedback:**
- `+` for arrays is considered natural and well-designed. The protocol-based approach means it extends to custom collection types automatically.
- String `+` can cause quadratic behavior in loops (same as Python). The community recommends `joined()` or building with arrays.
- Swift's custom string interpolation (SE-0228) is considered a standout feature, allowing type-safe formatting without concatenation.

---

### 10. OCaml

**String concatenation:**
- `^` operator.
  ```ocaml
  "hello" ^ " " ^ "world"
  ```
- `String.concat sep list` for joining with separator.

**List concatenation:**
- `@` operator.
  ```ocaml
  [1; 2] @ [3; 4]   (* [1; 2; 3; 4] *)
  ```
- `List.append l1 l2` (same semantics as `@`).
- `::` (cons) for prepending: `1 :: [2; 3]`.

**Same operator?** No. `^` for strings, `@` for lists. OCaml has no operator overloading (without enabling modular implicits), so distinct operators are required.

**Interpolation:** No native string interpolation. `Printf.sprintf` or `Format.asprintf` is used. The `ppx_string_interpolation` preprocessor provides `{%string|...|}`-style interpolation as a syntax extension.

**Design feedback:**
- The distinct operators (`^`, `@`) are appreciated for clarity: you always know whether you're concatenating strings or lists.
- `@` is O(n) in the length of the left list (must copy the entire left list). This is standard for immutable linked lists.
- The lack of operator overloading means OCaml avoids the "what does `+` mean here?" problem entirely. This is seen as both a strength (clarity) and weakness (verbosity) depending on perspective.
- Modular implicits (a long-discussed OCaml extension) would potentially allow a unified concat operator, but it remains unmerged as of 2025.

---

### 11. Elixir

**String concatenation:**
- `<>` operator (binary concatenation).
  ```elixir
  "hello" <> " " <> "world"
  ```
- Works on binaries (which includes strings). Can also be used in pattern matching:
  ```elixir
  "he" <> rest = "hello"   # rest = "llo"
  ```

**List concatenation:**
- `++` operator.
  ```elixir
  [1, 2] ++ [3, 4]   # [1, 2, 3, 4]
  ```
- `--` for list subtraction (counterpart).
- `Enum.concat/1` for flattening lists of lists.

**Same operator?** No. `<>` for binaries/strings, `++` for lists. This is because strings (binaries) and lists (linked lists) are fundamentally different data structures in Erlang/BEAM.

**Interpolation:** Yes. `"Hello #{name}"` is the idiomatic approach. Reduces need for `<>` in most string-building scenarios.

**Design feedback:**
- The `<>` / `++` split is well-understood in the community because it mirrors the binary/list duality of the BEAM VM.
- `++` is right-associative, which matters for performance: `a ++ b ++ c` is evaluated as `a ++ (b ++ c)`, avoiding redundant copying of the middle list.
- `<>` being usable in pattern matching is a powerful feature that pure concatenation operators in other languages lack.
- The Erlang heritage means these operators are deeply ingrained and rarely questioned.

---

### 12. Ruby

**String concatenation:**
- `+` operator -- returns new string (non-mutating).
  ```ruby
  "hello" + " " + "world"
  ```
- `<<` operator -- mutates the receiver in place (more efficient).
  ```ruby
  s = "hello"
  s << " world"    # s is now "hello world"
  ```
- `concat(*args)` -- mutates in place, accepts multiple arguments.
- String interpolation: `"Hello #{name}"` -- the idiomatic approach.

**Array concatenation:**
- `+` operator -- returns new array.
  ```ruby
  [1, 2] + [3, 4]   # [1, 2, 3, 4]
  ```
- `concat(*arrays)` -- mutates the receiver.
- `<<` for single element push.
- `flatten` for nested arrays.

**Same operator?** Yes for `+` (both strings and arrays). `<<` has different semantics: mutating append for strings, single-element push for arrays.

**Interpolation:** Yes. `"Hello #{expr}"` is the strongly preferred approach. RuboCop (the standard linter) enforces interpolation over `+` by default.

**Design feedback:**
- The `+` vs `<<` distinction (immutable vs mutable) is well-designed but occasionally confuses beginners.
- `<<` on strings accepting integers (codepoints) is a surprising edge case: `s << 33` appends `"!"`.
- String interpolation is so natural in Ruby that `+` for string building is considered non-idiomatic. Seeing `"Hello " + name` in a codebase is a code smell.
- `+` for arrays creating a new array (not mutating) is consistent with string `+` semantics, which is appreciated.

---

### 13. Clojure

**String concatenation:**
- `str` function -- variadic, coerces all arguments to strings and concatenates.
  ```clojure
  (str "hello" " " "world")   ; "hello world"
  (str "value: " 42)           ; "value: 42"
  ```
- No operator. Everything is a function call.

**List/Sequence concatenation:**
- `concat` function -- returns a lazy sequence.
  ```clojure
  (concat [1 2] [3 4])   ; (1 2 3 4)
  ```
- `into` function -- eager, returns concrete collection matching the target type.
  ```clojure
  (into [1 2] [3 4])     ; [1 2 3 4]
  (into #{} [1 2 3])     ; #{1 2 3}
  ```
- `conj` for adding elements (position depends on collection type).

**Same operator?** N/A. Clojure has no operators at all; everything is a function. `str` and `concat` serve different types.

**Interpolation:** No native string interpolation. `str` is the primary mechanism. Some libraries provide interpolation, but idiomatic Clojure uses `str` or `format`.

**Design feedback:**
- Stuart Sierra's "Clojure Don'ts" warns against `(reduce concat ...)` which can cause StackOverflowError due to deeply nested lazy sequences. Prefer `(apply concat ...)` or `(into [] cat colls)`.
- `concat` returning a lazy sequence (not a vector) surprises beginners: `(conj (concat [1] [2]) 3)` produces `(3 1 2)` because `conj` prepends to sequences.
- `into` is preferred over `concat` when you need a concrete collection type, as it avoids laziness pitfalls.
- The absence of operators is a deliberate Lisp design choice: no special syntax means no operator precedence rules to memorize.

---

## Cross-Cutting Themes

### 1. The `+` Overloading Problem

The most debated design question: should `+` be used for concatenation?

**Arguments against `+` for concatenation:**
- `+` is mathematically commutative (`a + b = b + a`); concatenation is not. This semantic mismatch causes confusion.
- When `+` is overloaded for both addition and concatenation (JS, Python), type coercion bugs emerge (`"2" + 2`).
- Rust's RFC #203 explicitly proposed `++` to separate commutative addition from non-commutative concatenation.

**Arguments for `+` for concatenation:**
- Familiar to most programmers from early languages (C++, Java, Python).
- Reduces the number of operators to learn.
- When types are distinct and enforced (Python, Kotlin, Swift), the overloading is safe.

**Observed pattern:** Languages designed after 2010 tend to avoid `+` for concatenation (Elm uses `++`, Gleam uses `<>`, Elixir separates `<>` and `++`). Older languages that use `+` rely heavily on interpolation to minimize its use.

### 2. Same vs Different Operators for Strings and Lists

**Same operator** (Python, Elm, Haskell, Kotlin, Swift, Ruby):
- Pro: Conceptual unity. "Concatenation is concatenation regardless of container."
- Pro: Enables generic programming over appendable types.
- Con: Can obscure performance characteristics (string `+` may be O(n^2) while list `+` is O(n)).

**Different operators** (Rust, Go, OCaml, Elixir, Gleam, Clojure):
- Pro: Makes the underlying data structure explicit. You know what you're operating on.
- Pro: Allows each operator to have semantics tailored to its type (e.g., Elixir `<>` works in pattern matching; `++` doesn't).
- Con: More operators to learn. May feel inconsistent.

**Observed pattern:** Languages with strong type systems and no operator overloading (OCaml, Gleam) are forced into different operators. Languages with typeclasses or protocols (Haskell, Elm, Swift, Kotlin) can unify them. Languages where strings and lists are fundamentally different structures (Elixir, Go) naturally use different operations.

### 3. Interpolation as Escape Valve

Every language that has string interpolation sees dramatically reduced use of string concatenation operators:

| Language | Interpolation Syntax | Impact on Concat |
|---|---|---|
| TypeScript | `` `${expr}` `` | Template literals are now default |
| Python | `f"{expr}"` | f-strings replaced most `+` usage |
| Kotlin | `"$var ${expr}"` | Docs recommend templates over `+` |
| Swift | `"\(expr)"` | Interpolation is the primary mechanism |
| Ruby | `"#{expr}"` | RuboCop flags `+` as non-idiomatic |
| Elixir | `"#{expr}"` | Reduces `<>` usage significantly |

Languages **without** interpolation (Rust, Go, Elm, Haskell, OCaml, Gleam, Clojure) rely more heavily on either concatenation operators or format functions. This is a meaningful language design coupling: if you skip interpolation, your concat operator gets heavier use and its ergonomics matter more.

### 4. Mutation vs Immutability

A cross-cutting concern in concatenation design:

- **Immutable return** (Python `+`, Kotlin `+`, Swift `+`, OCaml `^`/`@`): Returns new value, originals unchanged. Predictable but potentially expensive.
- **Mutable in-place** (Ruby `<<`, Rust `push_str`, Go `strings.Builder`): Modifies the receiver. More efficient but requires understanding of ownership/reference semantics.
- **Ownership transfer** (Rust `+`): Consumes the left operand. Unique approach that prevents accidental aliasing but confuses beginners.

### 5. Performance Pitfalls

Nearly every language with an immutable string concat operator has the same warning:

> Repeated concatenation in a loop is O(n^2). Use a builder/join pattern instead.

This applies to: Python (`+`), Java (`+`), Go (`+`), Rust (`+`), Swift (`+`), Haskell (`++`), OCaml (`^`).

Languages that address this:
- Python: `"".join(list)` -- O(n)
- Go: `strings.Builder` -- amortized O(n)
- Rust: `String::with_capacity` + `push_str` -- O(n)
- Java: `StringBuilder` -- O(n)
- Haskell: `Data.Text.Builder` or difference lists -- O(n)

---

## Recent Design Discussions (2024-2026)

### Rust RFC #3475: Unified String Literals (2023, still open 2025)
Proposes a unified syntax for string interpolation in Rust, which would reduce reliance on `format!()` and the awkward `+` operator. The RFC remains open with active discussion about syntax (`f"..."` vs `$"..."`) and interaction with borrowing.

### Rust RFC #3830: Dedented String Literals (2025)
Addresses multiline string formatting, tangentially related to concat: well-formatted multiline literals reduce the need for concatenation to build multi-line strings.

### Go `slices.Concat` (Go 1.22, February 2024)
Added as a standard library function to address the verbosity of `append(s1, s2...)`. This is notable because Go very rarely adds new stdlib functions. The addition acknowledges that slice concatenation was unnecessarily awkward.

### Gleam Community (ongoing)
String interpolation remains the most-requested missing feature. The absence forces heavy use of `<>` chains, which the community finds verbose for complex string building. Louis (Gleam's creator) has discussed this but no RFC exists as of early 2026.

### Python PEP 750: Template Strings (2025, under discussion)
Proposes t-strings (`t"Hello {name}"`) as a complement to f-strings, providing deferred/lazy interpolation. This would further reduce concatenation needs by enabling safe SQL/HTML templating without concat.

---

## Design Space Map

Positioning languages on two axes: **operator unification** (same operator for strings and lists) and **interpolation support** (whether interpolation reduces concat operator pressure).

```
                    Has Interpolation
                          |
           Kotlin    Swift|   Ruby
           Python         |   Elixir
                          |
  Same ───────────────────┼─────────────── Different
  Operator                |                Operator
                          |
           Elm     Haskell|   OCaml
                          |   Gleam    Go
                          |   Clojure  Rust
                          |
                   No Interpolation
```

Languages in the **bottom-right quadrant** (different operators, no interpolation) place the most burden on their concatenation syntax. These languages tend to rely on format functions (`format!`, `fmt.Sprintf`, `Printf.sprintf`) as a substitute for both interpolation and ergonomic concat.

Languages in the **top-left quadrant** (same operator, has interpolation) have the least friction: `+` exists for simple cases, interpolation handles complex ones, and the operator works uniformly across types.

---

## Implications for Almide

Almide currently uses `++` for both string and list concatenation (Elm-style). Key observations:

1. **`++` is a strong choice.** It avoids the `+` overloading problem, clearly communicates "concatenation" rather than "addition", and works uniformly across appendable types. Elm, Haskell, and Elixir validate this operator choice.

2. **Interpolation coupling.** If Almide has string interpolation, `++` pressure is low -- it mainly serves list concat and the occasional string join. If Almide lacks interpolation, `++` ergonomics for string building become critical.

3. **The unified operator works when types are enforced.** Python's `+` for both strings and lists works because it rejects mixed types. Almide's type system similarly prevents `"a" ++ [1]`, so the unification is safe.

4. **No language regrets `++`.** Among languages using `++` (Elm, Haskell, Elixir), none has proposed changing or removing it. The only debates are about adding alternatives (`<>` in Haskell), not replacing `++`.
