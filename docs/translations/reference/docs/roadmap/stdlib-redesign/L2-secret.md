<a id="l2l3-secret"></a>

# L2/L3: secret

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-secret.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-secret.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-secret.zh-CN.md)

> Non-normative reader translation of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-secret.md), pinned at 35297a612fb29e3f78f6587aba253aa91d51f466. This snapshot is not automatically synchronized. The original remains authoritative. Safety, security, comparative, implementation and measurement claims are retained as statements of that design-time source, without independent execution, timing measurement, audit or certification. They are not new guarantees about current releases. Code blocks and comments remain verbatim; metadata is an inactive reader note.

> Source description (inactive metadata): Stdlib redesign L2/L3 — secret: Secret[A] that every renderer prints as Secret(***), constant-time ==, Codec decode only

> Last updated: 2026-10-24
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-31), #4107
> Sources: Rust https://docs.rs/secrecy ・ Python https://docs.pydantic.dev/latest/api/types/#pydantic.types.SecretStr ・ Go https://pkg.go.dev/log/slog#LogValuer ・ Kotlin https://kotlinlang.org/docs/data-classes.html ・ Swift https://developer.apple.com/documentation/os/oslogprivacy ・ TS/JS https://nodejs.org/api/util.html#utilinspectcustom ・ Elixir https://hexdocs.pm/elixir/Inspect.html ・ https://hexdocs.pm/ecto/Ecto.Schema.html#field/3 ・ Java https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/lang/Record.html#toString()
> Measured: `almide 0.68.0 (dev, fcb2e6827)`, native and `--target wasm`.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust (secrecy) | Python (pydantic) | Go | Kotlin | Swift | TS/JS | Elixir | Java |
|---|---|---|---|---|---|---|---|---|---|
| Type that wraps a secret | `Secret[A]`, `secret.new` | `SecretBox<T>` | `SecretStr`, `Secret[T]` | — (write `String()` by hand) | — (write `toString` by hand) | — (`os.Logger`'s `<private>`) | — | — (`@derive {Inspect, except:}`) | — |
| Extract contents | `secret.reveal` | `expose_secret()` | `get_secret_value()` | — | — | `privacy: .public` | — | Read the field | — |
| Display | Always `Secret(***)` | `[REDACTED]` | `'**********'` | Handwritten | Handwritten | `<private>` | Handwritten | `**redacted**` | Handwritten |
| Write (encode) | Reject (E023) | Reject (`SerializableSecret` required) | Write redacted text | Write unchanged | Write unchanged | Write unchanged | Depends on `toJSON` | Separate configuration | Write unchanged |
| Read (decode) | Derive | Derive | Derive | — | — | — | — | — | — |
| `==` | Constant time (String / Bytes); otherwise E103 | Absent | Value comparison | — | — | — | — | — | — |

All 8/8 display every record field by default. Redaction is an explicit choice in each. Dedicated types exist in 2/8 (Rust secrecy and pydantic); neither implicitly exposes the contents.

<a id="完全性の規則"></a>

### Completeness rules

1. The two paths are `secret.new` into `Secret[A]` and `secret.reveal` out. Outside the module, field access and record literals are E033.
2. Display reaches every path from a single declaration (`Secret.repr`): interpolation `"${…}"`; inside enclosing records, lists, Option, Result, tuples, and Map; failure blocks of `assert_eq` / `assert_ne`; abort messages; and `almide test` reports. Every path produces `Secret(***)`.
3. Only `Secret[String]` and `Secret[Bytes]` have `==`, comparing in constant time. `Secret` of other `A`, and variants or Maps containing `Secret`, have no `==` (E103).
4. Codec: fields of `Secret[A]` decode in the shape of A, and encode is not derived (E023). `Secret[Bytes]` is E023, just like `Bytes`.
5. **Intentional omissions**: no Hash or Ord (Map keys or sorting keys). Avoid the very possibility of observing contents display or the presence of comparison in `map` / `set` (not yet rejected at the type level).

<a id="l2-の判定"></a>

### L2 verdict

- No missing roles present in 5 or more languages. Only 2 languages have dedicated types, but only a type meets #4107's requirement to “derive every display path from a single declaration” (a field attribute protects only one display path of one record).
- Measurement: `"${l}"` for `Login { user: "u", pw: secret.new("CANARY") }`, `[l]`, `some(s)`, `(1, s)`, a variant payload, and `map.from_list([("k", s)])` all contained `Secret(***)` and not the canary on both native and wasm.

<a id="案"></a>

## Proposals

<a id="r-secret-01-secretnew-と-secretreveal-を足す---採用"></a>

### R-secret-01 Add `secret.new` and `secret.reveal` — ○ Accepted

- Content: `fn secret.new[A](x: A) -> Secret[A]`, `fn secret.reveal[A](s: Secret[A]) -> A`. Requires `import secret` (not automatically imported).
- Supporting L0 rules: R-L0-00 (the type form found in 2/8), R-L0-03 (subject first), R-L0-30 (element value is `x`; subject name is `s`).
- Spelling: a constructor without a subject is `new` (R-L0-02, as with `map.new` and `regex.new`). There is no majority spelling for extraction (Rust secrecy `expose_secret`, pydantic `get_secret_value`; Go, Swift, and Kotlin have none). `to_string` is the name in `Repr` (R-L0-38) and returns `Secret(***)` for Secret, so it cannot be used (`html.to_string` returns the contents of `SafeHtml`, but SafeHtml is not a hiding type). Almide has no `unwrap` (unified under `??`, R-L0-06). Choose `reveal` as a verb with no other meaning, making extraction sites discoverable with grep.
- protocol (R-L0-37): `Repr` (always `Secret(***)`), `Eq` only when `A` is `String` or `Bytes` (conditional conformance; otherwise E103), no `Hash` or `Ord`, and only the decode half of `Codec`. It will be placed in the type table in [protocol-view](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) after conditional conformance enters the language (envisioned design).
- (b) Pure. The same on both targets.
- (c) Local variables named `secret` are common, so automatic import would collide. This is opt-in with a visible import line.
- issue: #4107

<a id="r-secret-02-表示は-secret宣言-1-か所---採用"></a>

### R-secret-02 Display as `Secret(***)`, with one declaration — ○ Accepted

- Content: interpolation, container contents, assert / abort, and test reports all read the type's `repr` declaration (`Secret.repr`). Values whose types do not declare repr retain structural display as before.
- issue: #4107

<a id="r-secret-03--は定数時間string--bytes-以外は-e103---採用"></a>

### R-secret-03 Constant-time `==`; E103 except for String / Bytes — ○ Accepted

- Content: `==` / `!=` for `Secret[String]` / `Secret[Bytes]` compare using `string.is_equal_constant_time` / `bytes.is_equal_constant_time`. The same applies to `Secret` inside records, tuples, lists, Option, and Result.
- issue: #4107 (uses #4106's constant-time comparison)

<a id="r-secret-04-codec-は-decode-だけ導出encode-は-e023---採用"></a>

### R-secret-04 Derive only Codec decode; encode is E023 — ○ Accepted

- Content: adopt Rust secrecy's “reject” column. Writing redacted text (pydantic) silently inserts `***` into databases or outgoing requests, causing failures far from the cause.
- issue: #4107

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `new` | `secret.new[A](x: A) -> Secret[A]` | Rust `SecretBox::new` / pydantic `SecretStr(x)` | Add | R-secret-01 |
| `reveal` | `secret.reveal[A](s: Secret[A]) -> A` | Rust `expose_secret()` / pydantic `get_secret_value()` | Add | R-secret-01 |

## Source caveats

The source combines accepted additions, reported canary measurements, and an envisioned conditional-conformance design. Its explicit statement that Hash/Ord-related uses are not yet rejected at the type level is retained. Display redaction and constant-time comparisons are claims of this pinned design record, not a new security certification, independent timing analysis, or assurance about every information-leak path.
