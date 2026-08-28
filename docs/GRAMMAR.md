# Almide Grammar (EBNF)

Faithful to the v0.34.x parser (`crates/almide-syntax/`). Terminals: `IDENT`
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
              generic_params? "(" params ")" "->" type ("=" fn_body)?
fn_name     = IDENT | TYPENAME "." IDENT                  (* convention method: fn Point.norm(self, ...) *)
fn_body     = expr | braceless_block                      (* body may start with let/var/guard directly *)
params      = ("self" ","?)? (param ("," param)* ","?)?
param       = attr* "mut"? IDENT ":" type ("=" expr)?     (* default args: once one has =, the rest must *)
attr        = "@" IDENT ("(" attr_arg ("," attr_arg)* ")")?   (* @extern(...), @export(...), @inline_rust *)

protocol_decl   = "protocol" TYPENAME generic_params? "{" protocol_method* "}"
protocol_method = "effect"? "fn" fn_name generic_params? "(" params ")" "->" type

top_let     = "pub"? ("local" | "mod")? ("let" | "var") name (":" type)? "=" expr
test_decl   = "test" STRING where_clause* block           (* where: table cases / binds / call stubs *)
generic_params = "[" gparam ("," gparam)* "]"
gparam      = TYPENAME (":" (TYPENAME ("+" TYPENAME)* | "{" fields "}"))?   (* bounds; structural bound *)
```

- Visibility: default is public; `pub` is an explicit no-op synonym. `mod` = same
  project, `local` = same file. `pub type` is not accepted (types take only
  `local`/`mod`).
- `effect fn` marks side-effecting functions; on the Rust target it compiles to
  `Result<T, String>`. Propagation is always **explicit** (ADR-0008): the postfix
  `!` is what lowers to `?`. A fallible call with no `!` keeps its `Result` value
  (E041 when the binding is un-annotated, E042 in statement position).

## Types

```ebnf
type_body   = record_type | variant_type | type            (* otherwise: alias *)
record_type = "{" field ("," field)* ","? "}"
field       = attr* name ("as" STRING)? ":" type ("=" expr)?  (* serialize alias; field default *)
variant_type= "|"? variant ("|" variant)+                  (* leading | optional *)
variant     = TYPENAME
            | TYPENAME "(" type ("," type)* ")"            (* tuple payload *)
            | TYPENAME "{" field ("," field)* "}"          (* record payload *)

type        = TYPENAME type_args?                          (* Int, Float, String, Bool, Unit, List[T], ... *)
            | IDENT "." TYPENAME type_args?                (* module-qualified: mod.Name[T] *)
            | "{" field ("," field)* (",")? ".."? "}"      (* record / open record { a: Int, .. } *)
            | "(" ")"                                      (* Unit *)
            | "(" type ("," type)+ ")"                     (* tuple type *)
            | ("fn" | "Fn") "(" (type ("," type)*)? ")" "->" type   (* function type *)
            | "(" (type ("," type)*)? ")" "->" type        (* function type, paren form *)
            | INT                                          (* const literal: Array[Float, 128] *)
type_args   = "[" type ("," type)* "]"                     (* generics use [] not <> *)
```

## Expressions

```ebnf
expr        = if_expr | if_let | match_expr | for_in | while_expr | fan_expr
            | lambda | binary | range | pipe | postfix | primary
block       = "{" (stmt ";"?)* expr? "}"                   (* trailing expr is the block value *)
stmt        = let_stmt | var_stmt | guard_stmt | assign | expr
let_stmt    = "let" ("_" | IDENT) (":" type)? "=" expr
            | "let" "(" tuple_pat ")" "=" expr             (* tuple destructuring, nestable *)
            | "let" "{" IDENT ("," IDENT)* ","? "}" "=" expr   (* record destructuring *)
var_stmt    = "var" IDENT (":" type)? "=" expr             (* no destructuring for var *)
tuple_pat   = (IDENT | "_" | "(" tuple_pat ("," tuple_pat)* ")") ("," tuple_pat)*
assign      = IDENT "=" expr | postfix "[" expr "]" "=" expr | postfix "." IDENT "=" expr
guard_stmt  = "guard" expr "else" expr
            | "guard" "let" IDENT "=" expr "else" expr     (* bind Option, else on none *)

if_expr     = "if" expr "then" expr ("else" expr)?         (* no else => Unit *)
if_let      = "if" "let" IDENT "=" expr block "else" block (* else required, branches braced *)
match_expr  = "match" expr "{" (arm ","?)* "}"
arm         = pattern ("|" pattern)* ("if" expr)? "=>" expr  (* or-alternatives + optional guard *)
for_in      = "for" (binder | "(" binder ("," binder)* ")") "in" expr block
binder      = IDENT | "_"
while_expr  = "while" expr block
fan_expr    = "fan" "{" (expr ";"?)+ "}"                   (* concurrent; exprs only, no statements *)
lambda      = "(" (lparam ("," lparam)*)? ")" "=>" expr    (* the ONLY lambda form *)
lparam      = (IDENT | "_") (":" type)?
            | "(" (IDENT | "_") ("," (IDENT | "_"))* ")"   (* tuple-destructuring param *)

pipe        = expr "|>" expr                               (* also: expr |> match { arms } *)
range       = expr "..<" expr | expr "..." expr            (* exclusive / inclusive *)
postfix     = primary ( "(" args ")"                       (* call *)
                      | "[" type_args "]" "(" args ")"     (* explicit type args: f[Int](x) *)
                      | "[" expr "]"                       (* index *)
                      | "." IDENT | "." INT                (* field access; tuple index t.0 *)
                      | "!"                                (* unwrap, propagates err (effect fn) *)
                      | "?"                                (* Result -> Option *)
                      | "?." IDENT                         (* optional chaining *)
                      | "??" unary )*                      (* fallback; RHS binds at unary level *)
args        = ((expr | IDENT ":" expr | "_") ",")* ...     (* named args; _ = partial application *)
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
pattern     = "_" | IDENT | INT | FLOAT | STRING | "-" (INT | FLOAT)
            | "true" | "false"
            | "none" | "some" "(" pattern ")" | "ok" "(" pattern ")" | "err" "(" pattern ")"
            | (IDENT ".")? TYPENAME                        (* unit constructor; module-qualified *)
            | (IDENT ".")? TYPENAME "(" pattern ("," pattern)* ")"
            | (IDENT ".")? TYPENAME "{" field_pat ("," field_pat)* (".." ","?)? "}"
            | "(" pattern ("," pattern)* ")"               (* tuple *)
            | "[" (pattern ("," pattern)*)? "]"            (* list — fixed length only *)
field_pat   = name (":" pattern)?                          (* shorthand binds the field name *)
```

Not supported (each has a dedicated parse error): or-patterns (`a | b`),
range patterns, `@` bindings, list rest (`[h, ...t]` — use `list.first` /
`list.drop`), `head :: tail`.

## Literals

- Int: decimal with `_` separators (`1_000`), hex `0xFF` / `0xff_00`. No binary/octal.
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

Auto-imported (no `import` needed — union of the seed list in
`crates/almide-frontend/src/import_table.rs` and `AUTO_IMPORT_BUNDLED` in
`crates/almide-types/src/stdlib_info.rs`):

```
string, list, int, float, bytes, matrix, map, set, option, result, value, prim,
error, math, datetime, int8, int16, int32, uint8, uint16, uint32, uint64, float32
```

Explicit `import` required:

```
json, http, fs, process, regex, io, random, testing, env, net, zlib, base64,
hex, html, mem, args, path
```

See [stdlib/](./stdlib/) and [CHEATSHEET.md](./CHEATSHEET.md) for function
references.

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
