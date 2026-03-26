# Almide Grammar (EBNF)

```ebnf
program     = import* decl*
import      = "import" path ("as" IDENT)?                (* import json, import self as app *)
decl        = type_decl | fn_decl | protocol_decl | top_let | strict_decl | test_decl
protocol_decl = "protocol" IDENT "{" protocol_method* "}"
protocol_method = "effect"? "fn" IDENT "(" params ")" "->" type
type_decl   = "type" IDENT type_params? (":" convention+)? "=" type_body
type_body   = record_body | variant_body | type
record_body = "{" field ("," field)* "}"
variant_body= "|"? variant ("|" variant)*
variant     = IDENT | IDENT "(" type ("," type)* ")" | IDENT "{" field ("," field)* "}"
fn_decl     = visibility? "effect"? "fn" IDENT type_params? "(" params ")" "->" type "=" expr
visibility  = "local" | "mod"                             (* default is public *)
top_let     = "let" IDENT (":" type)? "=" expr            (* module-scope constant *)
strict_decl = "strict" IDENT                              (* strict mode directive *)
test_decl   = "test" STRING block
type        = "Int" | "Float" | "String" | "Bool" | "Unit" | "Path"
              | IDENT | IDENT "[" type ("," type)* "]"    (* generics use [] not <> *)
              | "(" type ("," type)+ ")"                  (* tuple type *)
              | "Fn" "(" type* ")" "->" type              (* function type *)
expr        = block | if_expr | match_expr | for_in | while_expr
              | fan_expr | guard | let | var | assign | binary | pipe | call
              | lambda | literal | range
block       = "{" stmt* expr? "}"
if_expr     = "if" expr "then" expr ("else" expr)?        (* else is optional; omitting returns Unit *)
match_expr  = "match" expr "{" arm ("," arm)* "}"
arm         = pattern ("if" expr)? "=>" expr              (* optional guard *)
for_in      = "for" (IDENT | "(" IDENT "," IDENT ")") "in" expr block
while_expr  = "while" expr block                          (* condition-based loop *)
fan_expr    = "fan" "{" expr+ "}"                         (* concurrent execution *)
guard       = "guard" expr "else" expr                    (* early exit / loop break *)
let         = "let" IDENT (":" type)? "=" expr
var         = "var" IDENT (":" type)? "=" expr
assign      = IDENT "=" expr
binary      = expr OP expr    (* OP: + - * / % ^ == != < > <= >= and or not >> *)
                               (* + for string/list concat, ^ for power (right-assoc) *)
                               (* not for boolean neg, >> for function composition *)
                               (* comparison operators are non-associative: a < b < c is an error *)
                               (* ** is accepted as an alias for ^ *)
pipe        = expr "|>" expr                              (* pipe operator *)
range       = expr ".." expr | expr "..=" expr            (* exclusive / inclusive range *)
call        = expr "(" args ")" | expr "." IDENT "(" args ")"
              | expr "[" expr "]"                         (* index access *)
args        = (expr | IDENT ":" expr) ("," (expr | IDENT ":" expr))*  (* named args supported *)
lambda      = "(" params ")" "=>" expr                    (* shorthand *)
              | "fn" "(" params ")" "=>" expr             (* explicit *)
pattern     = "_" | IDENT | LITERAL | "-" LITERAL         (* negative numeric literals *)
              | "true" | "false"
              | "some" "(" pattern ")" | "none"
              | "ok" "(" pattern ")" | "err" "(" pattern ")"
              | TYPENAME "(" pattern ("," pattern)* ")"   (* constructor *)
              | TYPENAME "{" field_pat ("," field_pat)* ("..")? "}"  (* record *)
              | "(" pattern "," pattern ("," pattern)* ")"           (* tuple *)
list_lit    = "[" (expr ("," expr)*)? "]"                 (* [1, 2, 3] or [] *)
map_lit     = "[" expr ":" expr ("," expr ":" expr)* "]"  (* ["a": 1, "b": 2] *)
              | "[" ":" "]"                               (* empty map: [:] *)
literal     = INT | FLOAT | STRING | SINGLE_STRING | "true" | "false"
              | "ok" "(" expr ")" | "err" "(" expr ")"
              | list_lit | map_lit | record_lit
              (* double-quote strings: "hello ${name}" — interpolation + escapes *)
              (* single-quote strings: 'hello' — escapes only, no interpolation *)
              (* heredoc: """...""" or r"""...""" (raw) *)
              (* numeric literals support underscores: 1_000, 0xFF *)
```

## Stdlib (summary — see STDLIB-SPEC.md for full reference)

```
Auto-imported: string, list, map, int, float, option, result, env, io, process
Import required: json, math, random, datetime, regex, fs, http, log, testing,
                 error, crypto, uuid, set, value
```

See [STDLIB-SPEC.md](./STDLIB-SPEC.md) for the complete stdlib function reference (381 functions across 22 modules).

## Notes

- `string`, `list`, `map`, `int`, `float`, `option`, `result`, `env`, `io`, `process` are auto-imported — no `import` needed
- No `return`, `class`, `null`, `!` — use Almide alternatives (`not` for negation)
- `for x in xs { ... }` for iterating lists; `for (k, v) in m { ... }` for maps
- `while cond { ... }` for condition-based loops
- `fan { a; b }` for structured concurrent execution
- `import module` or `import self as alias` or `import pkg.submodule`
- Map literal: `["key": value]`, empty map: `[:]` (with type annotation)
- Single-quote strings `'hello'` for literal strings (escapes only, no interpolation)
- `if` without `else` returns Unit (useful for side-effect-only branches)
- `effect fn` marks functions with side effects
- `fn` visibility: `pub` (default), `mod` (same project), `local` (same file)
- Default arguments and named arguments are supported
- Case-insensitive aliases: `Ok`/`ok`, `Err`/`err`, `Some`/`some`, `None`/`none`
- All errors via `Result[T, E]`, all optionals via `Option[T]`
- Operators (high to low): `. ()` > `not -` > `^` (power, right-assoc) > `* / %` > `+ - ++` > `..` `..=` > `== != < > <= >=` (non-assoc) > `and` > `or` > `|>` `>>`
- See [CHEATSHEET.md](./CHEATSHEET.md) for syntax details and examples
