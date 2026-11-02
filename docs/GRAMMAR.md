# Almide Grammar (EBNF)

A DESCRIPTIVE snapshot: the source of truth for accepted syntax is the
implementation parser ([crates/almide-syntax](https://github.com/almide/almide/tree/develop/crates/almide-syntax),
exercised by this repository's corpus and AST-parity gates), and the
machine-readable grammar lives in [almide/almide-grammar](https://github.com/almide/almide-grammar).
Written against the v0.34.x parser; amended since alongside syntax changes.
Terminals: `IDENT`
(lowercase-initial), `TYPENAME` (uppercase-initial), `INT`, `FLOAT`, `STRING`.
Keywords can be used as identifiers by backtick-escaping: `` `type` ``.

## Declarations

```ebnf
program     = module_decl? import* decl*
module_decl = "module" dotted_path                        (* legacy, optional *)
import      = "import" dotted_path
              ( "." "{" name ("," name)* ","? "}"         (* selective: import mod.{ A, b } *)
              | "as" IDENT )?                             (* alias: import self as app *)
dotted_path = IDENT ("." IDENT)*                          (* import pkg.submodule *)

decl        = type_decl | fn_decl | protocol_decl | top_let | test_decl

type_decl   = ("local" | "mod")? "type" TYPENAME generic_params?
              (":" TYPENAME ("," TYPENAME)*)?             (* conventions: type Name: Eq, Repr *)
              "=" type_body

fn_decl     = attr* "pub"? ("local" | "mod")? "effect"? "fn" fn_name
              generic_params? "(" params ")" "->" return_type ("=" fn_body)?
fn_name     = IDENT | TYPENAME "." IDENT                  (* convention method: fn Point.norm(self, ...) *)
fn_body     = expr | braceless_block                      (* body may start with let/var/guard directly *)
params      = ("self" ","?)? (param ("," param)* ","?)?
param       = attr* "mut"? IDENT ":" type ("=" expr)?     (* default args: once one has =, the rest must *)
attr        = "@" IDENT ("(" attr_arg ("," attr_arg)* ")")?   (* @extern(...), @export(...), @inline_rust *)

protocol_decl   = "protocol" TYPENAME generic_params? "{" protocol_method* "}"
protocol_method = "effect"? "fn" fn_name generic_params? "(" params ")" "->" return_type

top_let     = "pub"? ("local" | "mod")? ("let" | "var") name (":" type)? "=" expr
test_decl   = "test" STRING where_clause* block           (* where: table cases / binds / call stubs *)
generic_params = "[" gparam ("," gparam)* "]"
gparam      = TYPENAME (":" (TYPENAME ("+" TYPENAME)* | "{" fields "}"))?   (* bounds; structural bound *)
```

- Visibility: default is public; `pub` is an explicit no-op synonym. `mod` = same
  project, `local` = same file. `pub type` is not accepted (types take only
  `local`/`mod`).
- `effect fn` marks effect calls and has a fallible result channel; its error
  type is not necessarily String (`T!E` spells `Result[T, E]`). Postfix `!` is
  explicit, with propagation/unwrap behavior determined by the enclosing
  function or lambda context, including test-block rules. See the
  [context table](specs/result-option-effect.md), rather than treating every
  `!` as a Rust `?`. Calls without `!` keep their Result/Option values; E041
  and E042 cover the unannotated-binding and discard cases described in the
  diagnostic reference.

## Types

```ebnf
type_body   = record_type | variant_type | type            (* otherwise: alias *)
record_type = "{" field ("," field)* ","? "}"
field       = attr* name ("as" STRING)? ":" type ("=" expr)?  (* serialize alias; field default *)
variant_type= "|"? variant ("|" variant)+                  (* leading | optional *)
variant     = TYPENAME
            | TYPENAME "(" type ("," type)* ")"            (* tuple payload *)
            | TYPENAME "{" field ("," field)* "}"          (* record payload *)

type        = named_type "?"?                          (* Int, Float, String, Bool, Unit, List[T], ... *)
            | "{" field ("," field)* (",")? ".."? "}"      (* record / open record { a: Int, .. } *)
            | "(" ")"                                      (* Unit *)
            | "(" type ("," type)* ")" "?"?               (* grouped / tuple type; optional suffix *)
            | "effect"? fn_type                           (* function slot; optional effect prefix *)
            | INT                                          (* const literal: Array[Float, 128] *)
named_type  = (IDENT ".")* TYPENAME type_args?
fn_type     = ("fn" | "Fn")? "(" (type ("," type)*)? ")" "->" return_type
return_type = type ("!" named_type?)?                      (* T! = Result[T,String]; T!E = Result[T,E] *)
type_args   = "[" type ("," type)* "]"                     (* generics use [] not <> *)
```

`?` attaches to a named or parenthesized type atom; `(A) -> B?` returns
Option[B], while `((A) -> B)?` is an optional function. Nested Option uses
`(T?)?`, not `T??`. The `!E` return marker and its optional named error type
must be on the same line; it is not a general postfix marker on every type.

## Expressions

```ebnf
expr        = if_expr | if_let | match_expr | for_in | while_expr | fan_expr
            | lambda | binary | range | pipe | postfix | primary
block       = "{" (stmt ";"?)* expr? "}"                   (* trailing expr is the block value *)
stmt        = let_stmt | var_stmt | guard_stmt | assign | expr
let_stmt    = "let" ("_" | IDENT) (":" type)? "=" expr
            | "let" "(" tuple_pat ")" "=" expr             (* tuple destructuring, nestable *)
            | "let" "{" IDENT ("," IDENT)* ","? "}" "=" expr   (* record destructuring *)
var_stmt    = "var" IDENT (":" type)? "=" expr
            | "var" "(" tuple_pat ")" "=" expr             (* the let patterns; every bound name is a var *)
            | "var" "{" IDENT ("," IDENT)* ","? "}" "=" expr
tuple_pat   = (IDENT | "_" | "(" tuple_pat ("," tuple_pat)* ")") ("," tuple_pat)*
assign      = IDENT ("." name)* ("[" expr "]")? "=" expr (":" type)?
            (* targets: `x = v`, `s.f = v`, `o.inner.xs = v`, `xs[i] = v`, `o.m[k] = v`.
               An index can only be the LAST step: `ps[0].n = v` is refused at the
               target with the read-modify-store rewrite. The value's trailing
               ascription (`s.xs = []: List[Int]`) is optional — the target's own
               type already fixes an empty literal. *)
guard_stmt  = "guard" expr "else" expr
            | "guard" "let" IDENT "=" expr "else" expr     (* bind Option, else on none *)

if_expr     = "if" expr "then" if_branch ("else" if_branch)?  (* no else => Unit *)
if_branch   = assign | expr                                (* any `assign` target, unbraced:
                                                              `if c then o.ui.xs[i] = v`; an
                                                              assignment branch is Unit *)
if_let      = "if" "let" IDENT "=" expr block "else" block (* else required, branches braced *)
match_expr  = "match" expr "{" (arm ","?)* "}"
arm         = pattern ("|" pattern)* ("if" expr)? "=>" expr  (* or-alternatives + optional guard *)
for_in      = "for" (binder | "(" binder ("," binder)* ")") "in" expr block
binder      = IDENT | "_"
while_expr  = "while" expr block
fan_expr    = "fan" "{" (expr ","?)+ "}"                   (* concurrent arms → tuple; `,`/newline between, never `;` *)
            | "fan" "{" (IDENT ":" expr ","?)+ "}"         (* record form: concurrent fields → record (#1388) *)
lambda      = "(" (lparam ("," lparam)*)? ")" "=>" expr    (* the ONLY lambda form *)
lparam      = (IDENT | "_") (":" type)?
            | "(" (IDENT | "_") ("," (IDENT | "_"))* ")"   (* tuple-destructuring param *)

pipe        = expr "|>" expr                               (* also: expr |> match { arms } *)
range       = expr "..<" expr | expr "..." expr            (* exclusive / inclusive *)
postfix     = primary ( "(" args ")"                       (* call *)
                      | type_args "(" args ")"     (* explicit type args: f[Int](x) *)
                      | "[" expr "]"                       (* index *)
                      | "." IDENT | "." INT                (* field access; tuple index t.0 *)
                      | "!"                                (* context-dependent propagation/unwrap; see context table *)
                      | "?"                                (* Result -> Option *)
                      | "?." IDENT                         (* optional chaining *)
                      | "??" unary )*                      (* fallback; RHS binds at unary level *)
args        = ((expr | IDENT ":" expr) ",")* ...           (* named args; partial application is rejected as E046 *)
primary     = literal | IDENT | TYPENAME | "(" expr ")" | "(" expr ("," expr)+ ")"
            | "(" ")" | block | record_lit | "todo" "(" STRING ")"
            | "break" | "continue" | "_"                   (* _ = typed hole in expr position *)
            | "none" | "some" "(" expr ")" | "ok" "(" expr ")" | "err" "(" expr ")"
record_lit  = "{" (fieldinit ("," fieldinit)* ","?)? "}"   (* anonymous *)
            | (IDENT ".")? TYPENAME "{" fieldinit* "}"     (* named; module-qualified *)
            | (IDENT ".")? TYPENAME? "{" "..." expr ("," fieldinit)* "}"  (* spread: { ...base, x: 1 } *)
fieldinit   = name (":" expr)?                             (* shorthand: { x } = { x: x } *)
```

## Patterns

```ebnf
pattern     = IDENT "@" pattern                           (* bind the whole matched value *)
            | "_" | IDENT | INT | FLOAT | STRING | "-" (INT | FLOAT)
            | "true" | "false"
            | "none" | "some" "(" pattern ")" | "ok" "(" pattern ")" | "err" "(" pattern ")"
            | (IDENT ".")? TYPENAME                        (* unit constructor; module-qualified *)
            | (IDENT ".")? TYPENAME "(" pattern ("," pattern)* ")"
            | (IDENT ".")? TYPENAME "{" field_pat ("," field_pat)* (".." ","?)? "}"
            | "(" pattern ("," pattern)* ")"               (* tuple *)
            | "[" (pattern ("," pattern)* ","?)? "]"      (* fixed-length list *)
            | "[" (pattern ",")* ".." IDENT? "]"           (* tail rest, last position only *)
field_pat   = name (":" pattern)?                          (* shorthand binds the field name *)
```

Match arms support or-alternatives (`a | b => ...`), as-patterns
(`whole @ pattern`), and final list rest (`[h, ..tail]` or `[h, ..]`).
Range patterns, three-dot rest (`[h, ...tail]`), and `head :: tail` are not
accepted. Rest may not be followed by another pattern or a trailing comma.

## Literals

- Int: decimal with `_` separators (`1_000`), hex `0xFF`, binary `0b1010`,
  octal `0o17`; radix prefixes are case-insensitive and digits may use `_` separators.
- Float: `3.14`, scientific `1e9` / `2.5E-3`, `_` separators.
- Strings: `"..."` (escapes + `${expr}` interpolation), `'...'` (escapes only,
  no interpolation), `"""..."""` heredoc (interpolation + common-indent strip),
  `r"..."` / `r"""..."""` raw (no escapes, no interpolation).
  Escapes: `\n \t \r \\ \" \$ \xNN \u{...}`.
- Collections: list `[1, 2]` / `[]`; map `["a": 1]`, empty map `[:]`;
  tuple `(a, b)`; unit `()`.
- Comments: `//` line, `/* ... */` block (nestable).

## Operator precedence

Loosest to tightest. Comparison operators are non-associative — `a < b < c` is
a parse error (use `and`).

| Level | Operators | Associativity |
|---|---|---|
| 1 | `or` | left |
| 2 | `and` | left |
| 3 | `==` `!=` `<` `>` `<=` `>=` | non-assoc |
| 4 | `\|>` | left (asymmetric — see below) |
| 5 | `..<` `...` | — |
| 6 | `+` `-` | left |
| 7 | `*` `/` `%` | left |
| 8 | `^` (alias `**`) power | right |
| 9 | `>>` compose | left (tightest binary) |
| 10 | unary `-`, `not` | prefix |
| 11 | postfix: `()` `[]` `.` `!` `?` `?.` `??` | — |

**`|>` is asymmetric**: its right-hand side is a single postfix/compose chain.
Any other binary operator after the RHS applies to the whole pipe result:

- `xs |> list.map(f) + ys` = `(xs |> list.map(f)) + ys`
- `xs |> f >> g` = `xs |> (f >> g)` (only `>>` nests inside the RHS)
- `xs |> list.len > 5` = `(xs |> list.len) > 5`
- `a + b |> f` = `(a + b) |> f`

`x ?? fallback` takes only a unary expression as fallback: `x ?? 0 + 1` =
`(x ?? 0) + 1`.

## Stdlib imports

The current auto-import set is the union of the seed list in
`crates/almide-frontend/src/import_table.rs` and `AUTO_IMPORT_BUNDLED` in
`crates/almide-types/src/stdlib_info.rs`. Auto-import makes a module name
available, not all its functions as unqualified names. `prim` is restricted to
stdlib sources; it is not a public user import. The old `error` module is removed.

Use the [stdlib reference](stdlib/README.md) and [CHEATSHEET](CHEATSHEET.md)
for current APIs and import examples instead of maintaining another module list
in this syntax snapshot.

## Removed / rejected forms

These lex or parse only to produce a targeted diagnostic — they are not part of
the language:

- `++` — removed; `+` concatenates strings and lists
- `&&` / `||` / prefix `!` — use `and` / `or` / `not`
- `fn (x) => e` lambda — use `(x) => e`
- `impl` blocks — use convention methods: `fn Type.method(self, ...)`
- `deriving` — use the conventions clause: `type Name: Eq, Repr = ...`
- `do`, `while ... do ... done`, `let ... in`, `let mut`, `let rec`
- `return`, `class`, `null`, `unsafe`, `newtype`

## Notes

- Case aliases exist for exactly four keywords: `Ok`/`ok`, `Err`/`err`,
  `Some`/`some`, `None`/`none`. `true`/`false`/`todo` are lowercase-only.
- `ok/err/some/none/todo` are soft keywords: usable as field/member names, not
  as binding names.
- All errors via `Result[T, E]`, all optionals via `Option[T]`.
- `if` without `else` returns Unit; blocks evaluate to their trailing expression.
- Operators may start a continuation line; a line-initial `-` glued to its
  operand (`-1`) starts a new statement instead.
