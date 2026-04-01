/// Hint catalog: lists all available hints for discoverability and documentation.
///
/// Each entry describes one hint pattern the compiler can detect and suggest a fix for.

pub struct HintEntry {
    pub module: &'static str,
    pub trigger: &'static str,
    pub message: &'static str,
    pub hint: &'static str,
}

/// Returns the complete catalog of all hints the parser can produce.
pub fn all_hints() -> Vec<HintEntry> {
    vec![
        // ---- missing_comma ----
        HintEntry {
            module: "missing_comma",
            trigger: "Expression token where ',' or ']' expected in list literal",
            message: "Missing ',' between list elements",
            hint: "Add a comma after the previous element. Example: [a, b, c]",
        },
        HintEntry {
            module: "missing_comma",
            trigger: "Expression token where ',' or ']' expected in map literal",
            message: "Missing ',' between map entries",
            hint: "Add a comma after the previous element. Example: [\"a\": 1, \"b\": 2]",
        },
        HintEntry {
            module: "missing_comma",
            trigger: "Expression token where ',' or ')' expected in call arguments",
            message: "Missing ',' between function arguments",
            hint: "Add a comma after the previous element. Example: f(a, b, c)",
        },
        HintEntry {
            module: "missing_comma",
            trigger: "Expression token where ',' or ')' expected in function parameters",
            message: "Missing ',' between function parameters",
            hint: "Add a comma after the previous element. Example: fn f(a: Int, b: Int)",
        },

        // ---- operator ----
        HintEntry {
            module: "operator",
            trigger: "'=' where 'then' expected (if condition)",
            message: "Expected 'then'",
            hint: "Did you mean '=='? Use '==' for comparison. Write: if x == 5 then ...",
        },
        HintEntry {
            module: "operator",
            trigger: "Non-'then' token where 'then' expected",
            message: "Expected 'then'",
            hint: "if requires 'then'. Write: if condition then expr else expr",
        },
        HintEntry {
            module: "operator",
            trigger: "Missing 'else' branch",
            message: "Expected 'else'",
            hint: "if expressions MUST have an else branch. Use 'guard ... else' for early returns instead.",
        },
        HintEntry {
            module: "operator",
            trigger: "'=' where '->' expected (return type)",
            message: "Expected '->'",
            hint: "Use '->' for return type, not '='. Write: fn name() -> Type = body",
        },
        HintEntry {
            module: "operator",
            trigger: "'<' where ')' expected (generics)",
            message: "Expected ')'",
            hint: "Use [] for generics, not <>. Example: List[String], Result[T, E]",
        },
        HintEntry {
            module: "operator",
            trigger: "'||' in expression",
            message: "'||' is not valid in Almide",
            hint: "Use 'or' for logical OR. Example: if a or b then ...",
        },
        HintEntry {
            module: "operator",
            trigger: "'&&' in expression",
            message: "'&&' is not valid in Almide",
            hint: "Use 'and' for logical AND. Example: if a and b then ...",
        },
        HintEntry {
            module: "operator",
            trigger: "'!' in expression",
            message: "'!' is not valid in Almide",
            hint: "Use 'not x' for boolean negation, not '!x'.",
        },
        HintEntry {
            module: "operator",
            trigger: "'|' followed by identifier (closure syntax)",
            message: "'|x|' closure syntax is not valid in Almide",
            hint: "Use '(x) => expr' for lambdas. Example: list.map(xs, (x) => x + 1)",
        },
        HintEntry {
            module: "operator",
            trigger: "';' in expression",
            message: "Semicolons are not used in Almide",
            hint: "Remove the ';'. Almide uses newlines to separate statements.",
        },

        // ---- keyword_typo ----
        HintEntry {
            module: "keyword_typo",
            trigger: "'function'/'def'/'func'/'fun'/'proc' at top level",
            message: "'<keyword>' is not a keyword in Almide",
            hint: "Use 'fn name(...) -> Type = expr' or 'effect fn name(...) -> Result[T, E] = expr'.",
        },
        HintEntry {
            module: "keyword_typo",
            trigger: "'class'/'struct'/'enum'/'data'/'sealed'/'union' at top level",
            message: "'<keyword>' is not a keyword in Almide",
            hint: "Use 'type Name = { field: Type, ... }' for records, or 'type Name = | Case1 | Case2' for variants.",
        },
        HintEntry {
            module: "keyword_typo",
            trigger: "'interface'/'protocol'/'abstract' at top level",
            message: "'<keyword>' is not a keyword in Almide",
            hint: "Use 'trait Name { ... }' for traits.",
        },
        HintEntry {
            module: "keyword_typo",
            trigger: "'const'/'val' at top level",
            message: "'<keyword>' is not a keyword in Almide",
            hint: "Use 'let NAME = value' for top-level constants.",
        },
        HintEntry {
            module: "keyword_typo",
            trigger: "'while'/'for'/'loop' at top level",
            message: "'<keyword>' cannot appear at the top level",
            hint: "Define a function with 'fn' or 'effect fn'.",
        },
        HintEntry {
            module: "keyword_typo",
            trigger: "'return' at top level",
            message: "'return' is not needed in Almide",
            hint: "Almide functions return the last expression.",
        },
        HintEntry {
            module: "keyword_typo",
            trigger: "'import' after declarations",
            message: "Unexpected 'import'",
            hint: "All imports must come before other declarations.",
        },

        // ---- delimiter ----
        HintEntry {
            module: "delimiter",
            trigger: "Missing ')'",
            message: "Expected ')'",
            hint: "Missing ')'. Check for an unclosed '(' earlier in this expression",
        },
        HintEntry {
            module: "delimiter",
            trigger: "Missing ']'",
            message: "Expected ']'",
            hint: "Missing ']'. Check for an unclosed '[' earlier in this expression",
        },
        HintEntry {
            module: "delimiter",
            trigger: "Missing '}'",
            message: "Expected '}'",
            hint: "Missing '}'. Check for an unclosed '{' earlier in this block",
        },
        HintEntry {
            module: "delimiter",
            trigger: "Missing '=' before value",
            message: "Expected '='",
            hint: "Missing '=' before value. Write: let x = value",
        },

        // ---- syntax_guide ----
        HintEntry {
            module: "syntax_guide",
            trigger: "'return' in expression/block",
            message: "'return' is not needed in Almide",
            hint: "The last expression in a block is the return value. Use 'guard ... else' for early returns.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'null'/'nil' in expression",
            message: "'null'/'nil' does not exist in Almide",
            hint: "Use Option[T] with 'some(v)' / 'none'.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'throw' in expression",
            message: "'throw' is not valid in Almide",
            hint: "Use Result[T, E] with 'ok(v)' / 'err(e)'.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'catch'/'except' in expression",
            message: "'catch'/'except' is not valid in Almide",
            hint: "Use 'match' on Result values instead.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'loop' in expression",
            message: "'loop' is not valid in Almide",
            hint: "Use 'while true { ... }' or 'do { guard COND else ok(()) ... }'.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'print' in expression",
            message: "'print' is not a function in Almide",
            hint: "Use 'println(s)' instead.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'let mut' binding",
            message: "'let mut' is not valid in Almide",
            hint: "Use 'var' for mutable variables. Example: var x = 0",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'self'/'this' in expression",
            message: "'self'/'this' is not valid in Almide",
            hint: "Pass the value as the first parameter. Example: fn greet(user: User) -> String",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'new' in expression",
            message: "'new' is not needed in Almide",
            hint: "Construct records directly: Type { field: value }",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'void' in expression",
            message: "'void' does not exist in Almide",
            hint: "Use 'Unit' for functions that return nothing.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'undefined' in expression",
            message: "'undefined' does not exist in Almide",
            hint: "Use Option[T] with 'some(v)' / 'none'.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'switch' in expression",
            message: "'switch' is not valid in Almide",
            hint: "Use 'match' for pattern matching.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'elif'/'elsif'/'elseif' in expression",
            message: "'elif'/'elsif' is not valid in Almide",
            hint: "Use nested 'if/then/else'.",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'extends'/'implements' in expression",
            message: "'extends'/'implements' is not valid in Almide",
            hint: "Almide uses structural typing. Use open records: { field: Type, .. }",
        },
        HintEntry {
            module: "syntax_guide",
            trigger: "'lambda' in expression",
            message: "'lambda' is not valid in Almide",
            hint: "Use '(x) => expr' for lambdas.",
        },
    ]
}
