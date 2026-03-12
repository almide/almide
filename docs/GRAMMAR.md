# Almide Grammar (EBNF)

```ebnf
program     = import* decl*
import      = "import" IDENT
decl        = type_decl | fn_decl | top_let
type_decl   = "type" IDENT "=" "|" variant ("|" variant)* "deriving" "From"
variant     = IDENT "(" type ("," type)* ")"
fn_decl     = ["effect"] "fn" IDENT "(" params ")" "->" type "=" expr
top_let     = "let" IDENT [":" type] "=" expr       (* module-scope constant *)
type        = "Int" | "String" | "Bool" | "Unit" | IDENT | IDENT "[" type ("," type)* "]"
expr        = block | if_expr | match_expr | for_in | while_expr | do_expr | guard | let | var | assign | binary | call | literal
for_in      = "for" IDENT "in" expr "{" stmt* "}"  (* iterate over list/collection *)
while_expr  = "while" expr "{" stmt* "}"            (* loop while condition is true *)
block       = "{" stmt* expr "}"
if_expr     = "if" expr "then" expr "else" expr       (* else is MANDATORY *)
match_expr  = "match" expr "{" arm ("," arm)* "}"
arm         = pattern "=>" expr
pattern     = "some" "(" pattern ")" | "none" | "ok" "(" pattern ")" | "err" "(" pattern ")" | IDENT | LITERAL | "_"
do_expr     = "do" "{" (guard | stmt)* "}"            (* loop: use guard to break *)
guard       = "guard" expr "else" expr                 (* early exit / loop break *)
let         = "let" IDENT "=" expr
var         = "var" IDENT "=" expr
assign      = IDENT "=" expr
binary      = expr OP expr    (* OP: + - * / % ^ == != < > <= >= ++ and or *)
                               (* ++ for string/list concat, ^ for XOR, not for boolean neg *)
call        = IDENT "(" args ")" | IDENT "." IDENT "(" args ")"
lambda      = "fn" "(" params ")" "=>" expr
literal     = INT | STRING | "true" | "false" | "ok" "(" expr ")" | "err" "(" expr ")"
                               (* string interpolation: "hello ${name}" *)
```

## Stdlib (summary — see CHEATSHEET.md for full reference)

```
Auto-imported: string, list, map, int, float, fs, path, env, process, io
Import required: json, math, random, time, regex, encoding, args, hash, csv, bitwise, http
```

See [CHEATSHEET.md](./CHEATSHEET.md) for the complete stdlib function reference (203 functions across 14+ modules).

## Notes

- `int`, `string`, `list`, `map`, `path`, and `env` are auto-imported — no `import` needed. `fs` and `json` require explicit import.
- No `return`, `class`, `null`, `!` — use Almide alternatives
- `for x in xs { ... }` for iterating lists; `while cond { ... }` for condition-based loops; `do { guard ... }` for dynamic break with values
- `if` always requires `else`
- `effect fn` marks functions with side effects
- All errors via `Result[T, E]`, all optionals via `Option[T]`
