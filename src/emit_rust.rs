use crate::ast::*;
use crate::stdlib;

const JSON_RUNTIME: &str = r#"
#[derive(Debug, Clone, PartialEq)]
enum AlmideJson {
    JNull,
    JBool(bool),
    JInt(i64),
    JStr(String),
    JArray(Vec<AlmideJson>),
    JObject(HashMap<String, AlmideJson>),
}
use AlmideJson::*;

impl std::fmt::Display for AlmideJson {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", almide_json_stringify(self))
    }
}

fn almide_json_parse(input: &str) -> Result<AlmideJson, String> {
    let b = input.as_bytes();
    let (val, pos) = jparse_value(b, jskip_ws(b, 0))?;
    let pos = jskip_ws(b, pos);
    if pos != b.len() { return Err(format!("trailing data at position {}", pos)); }
    Ok(val)
}

fn jskip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') { i += 1; }
    i
}

fn jparse_value(b: &[u8], i: usize) -> Result<(AlmideJson, usize), String> {
    if i >= b.len() { return Err("unexpected end of input".into()); }
    match b[i] {
        b'"' => jparse_string(b, i),
        b'{' => jparse_object(b, i),
        b'[' => jparse_array(b, i),
        b't' => { if b.get(i..i+4) == Some(b"true") { Ok((JBool(true), i+4)) } else { Err(format!("invalid value at {}", i)) } }
        b'f' => { if b.get(i..i+5) == Some(b"false") { Ok((JBool(false), i+5)) } else { Err(format!("invalid value at {}", i)) } }
        b'n' => { if b.get(i..i+4) == Some(b"null") { Ok((JNull, i+4)) } else { Err(format!("invalid value at {}", i)) } }
        b'-' | b'0'..=b'9' => jparse_number(b, i),
        c => Err(format!("unexpected character '{}' at {}", c as char, i)),
    }
}

fn jparse_string(b: &[u8], i: usize) -> Result<(AlmideJson, usize), String> {
    let (s, pos) = jparse_string_raw(b, i)?;
    Ok((JStr(s), pos))
}

fn jparse_string_raw(b: &[u8], mut i: usize) -> Result<(String, usize), String> {
    if b.get(i) != Some(&b'"') { return Err("expected '\"'".into()); }
    i += 1;
    let mut s = String::new();
    while i < b.len() {
        match b[i] {
            b'"' => return Ok((s, i + 1)),
            b'\\' => {
                i += 1;
                if i >= b.len() { return Err("unexpected end in string escape".into()); }
                match b[i] {
                    b'"' => s.push('"'), b'\\' => s.push('\\'), b'/' => s.push('/'),
                    b'n' => s.push('\n'), b'r' => s.push('\r'), b't' => s.push('\t'),
                    b'u' => {
                        if i + 4 >= b.len() { return Err("incomplete unicode escape".into()); }
                        let hex = std::str::from_utf8(&b[i+1..i+5]).map_err(|_| "invalid unicode")?;
                        let cp = u32::from_str_radix(hex, 16).map_err(|_| "invalid unicode")?;
                        if let Some(c) = char::from_u32(cp) { s.push(c); } else { s.push('?'); }
                        i += 4;
                    }
                    c => { s.push('\\'); s.push(c as char); }
                }
                i += 1;
            }
            c => { s.push(c as char); i += 1; }
        }
    }
    Err("unterminated string".into())
}

fn jparse_number(b: &[u8], mut i: usize) -> Result<(AlmideJson, usize), String> {
    let start = i;
    if i < b.len() && b[i] == b'-' { i += 1; }
    while i < b.len() && b[i].is_ascii_digit() { i += 1; }
    // Skip fractional and exponent parts (treat as integer by truncation)
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') { i += 1; }
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
    }
    let s = std::str::from_utf8(&b[start..i]).map_err(|_| "invalid number")?;
    if s.contains('.') || s.contains('e') || s.contains('E') {
        let f: f64 = s.parse().map_err(|_| format!("invalid number: {}", s))?;
        Ok((JInt(f as i64), i))
    } else {
        let n: i64 = s.parse().map_err(|_| format!("number out of range: {}", s))?;
        Ok((JInt(n), i))
    }
}

fn jparse_array(b: &[u8], mut i: usize) -> Result<(AlmideJson, usize), String> {
    i += 1; // skip [
    i = jskip_ws(b, i);
    let mut items = Vec::new();
    if i < b.len() && b[i] == b']' { return Ok((JArray(items), i + 1)); }
    loop {
        i = jskip_ws(b, i);
        let (val, pos) = jparse_value(b, i)?;
        items.push(val);
        i = jskip_ws(b, pos);
        if i >= b.len() { return Err("unterminated array".into()); }
        if b[i] == b']' { return Ok((JArray(items), i + 1)); }
        if b[i] != b',' { return Err(format!("expected ',' or ']' at {}", i)); }
        i += 1;
    }
}

fn jparse_object(b: &[u8], mut i: usize) -> Result<(AlmideJson, usize), String> {
    i += 1; // skip {
    i = jskip_ws(b, i);
    let mut map = HashMap::new();
    if i < b.len() && b[i] == b'}' { return Ok((JObject(map), i + 1)); }
    loop {
        i = jskip_ws(b, i);
        let (key, pos) = jparse_string_raw(b, i)?;
        i = jskip_ws(b, pos);
        if i >= b.len() || b[i] != b':' { return Err(format!("expected ':' at {}", i)); }
        i = jskip_ws(b, i + 1);
        let (val, pos) = jparse_value(b, i)?;
        map.insert(key, val);
        i = jskip_ws(b, pos);
        if i >= b.len() { return Err("unterminated object".into()); }
        if b[i] == b'}' { return Ok((JObject(map), i + 1)); }
        if b[i] != b',' { return Err(format!("expected ',' or '}}' at {}", i)); }
        i += 1;
    }
}

fn almide_json_stringify(j: &AlmideJson) -> String {
    match j {
        JNull => "null".into(),
        JBool(b) => if *b { "true".into() } else { "false".into() },
        JInt(n) => n.to_string(),
        JStr(s) => {
            let mut out = String::from('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
        JArray(items) => {
            let inner: Vec<String> = items.iter().map(|v| almide_json_stringify(v)).collect();
            format!("[{}]", inner.join(","))
        }
        JObject(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys.iter().map(|k| format!("{}:{}", almide_json_stringify(&JStr(k.to_string())), almide_json_stringify(map.get(*k).unwrap()))).collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

fn almide_json_get(j: &AlmideJson, key: &str) -> Option<AlmideJson> {
    if let JObject(m) = j { m.get(key).cloned() } else { None }
}
fn almide_json_get_string(j: &AlmideJson, key: &str) -> Option<String> {
    match almide_json_get(j, key) { Some(JStr(s)) => Some(s), _ => None }
}
fn almide_json_get_int(j: &AlmideJson, key: &str) -> Option<i64> {
    match almide_json_get(j, key) { Some(JInt(n)) => Some(n), _ => None }
}
fn almide_json_get_bool(j: &AlmideJson, key: &str) -> Option<bool> {
    match almide_json_get(j, key) { Some(JBool(b)) => Some(b), _ => None }
}
fn almide_json_get_array(j: &AlmideJson, key: &str) -> Option<Vec<AlmideJson>> {
    match almide_json_get(j, key) { Some(JArray(a)) => Some(a), _ => None }
}
fn almide_json_keys(j: &AlmideJson) -> Vec<String> {
    if let JObject(m) = j { let mut v: Vec<String> = m.keys().cloned().collect(); v.sort(); v } else { vec![] }
}
fn almide_json_to_string(j: &AlmideJson) -> Option<String> {
    if let JStr(s) = j { Some(s.clone()) } else { None }
}
fn almide_json_to_int(j: &AlmideJson) -> Option<i64> {
    if let JInt(n) = j { Some(*n) } else { None }
}
"#;

struct Emitter {
    out: String,
    indent: usize,
    /// Track if we're inside an effect function (for ? operator)
    in_effect: bool,
    /// Names of effect functions in the program (auto-wrapped, no explicit Result return)
    effect_fns: Vec<String>,
    /// Names of all functions that return Result (for do-block auto-unwrap)
    result_fns: Vec<String>,
    /// Track if we're inside a do block (for auto-unwrap of Result calls)
    in_do_block: std::cell::Cell<bool>,
    /// Names of user-defined modules (for module call dispatch)
    user_modules: Vec<String>,
    /// Track if we're inside a test function
    in_test: bool,
}

impl Emitter {
    fn new() -> Self {
        Self { out: String::new(), indent: 0, in_effect: false, effect_fns: Vec::new(), result_fns: Vec::new(), in_do_block: std::cell::Cell::new(false), user_modules: Vec::new(), in_test: false }
    }

    fn emit_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    fn emitln(&mut self, s: &str) {
        self.emit_indent();
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn emit_program(&mut self, prog: &Program, modules: &[(String, Program)]) {
        // Collect effect function names (auto-wrapped: effect fn without explicit Result return)
        for decl in &prog.decls {
            if let Decl::Fn { name, effect, return_type, .. } = decl {
                if effect.unwrap_or(false) {
                    let ret_str = self.gen_type(return_type);
                    if !ret_str.starts_with("Result<") {
                        self.effect_fns.push(name.clone());
                    }
                }
            }
        }
        // Also collect effect fns from imported modules
        for (_, mod_prog) in modules {
            for decl in &mod_prog.decls {
                if let Decl::Fn { name, effect, return_type, .. } = decl {
                    if effect.unwrap_or(false) {
                        let ret_str = self.gen_type(return_type);
                        if !ret_str.starts_with("Result<") {
                            self.effect_fns.push(name.clone());
                        }
                    }
                }
            }
        }
        // Collect ALL functions that return Result (for do-block auto-unwrap)
        for decl in &prog.decls {
            if let Decl::Fn { name, return_type, effect, .. } = decl {
                let ret_str = self.gen_type(return_type);
                if ret_str.starts_with("Result<") || effect.unwrap_or(false) {
                    self.result_fns.push(name.clone());
                }
            }
        }
        for (_, mod_prog) in modules {
            for decl in &mod_prog.decls {
                if let Decl::Fn { name, return_type, effect, .. } = decl {
                    let ret_str = self.gen_type(return_type);
                    if ret_str.starts_with("Result<") || effect.unwrap_or(false) {
                        self.result_fns.push(name.clone());
                    }
                }
            }
        }
        self.user_modules = modules.iter().map(|(n, _)| n.clone()).collect();

        self.emitln("#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]");
        self.emitln("");
        self.emit_runtime();
        self.emitln("");

        // Emit imported modules as `mod name { ... }`
        for (mod_name, mod_prog) in modules {
            self.emit_user_module(mod_name, mod_prog);
            self.emitln("");
        }

        for decl in &prog.decls {
            self.emit_decl(decl);
            self.emitln("");
        }

        let main_decl = prog.decls.iter().find(|d| matches!(d, Decl::Fn { name, .. } if name == "main"));
        if let Some(Decl::Fn { params, effect, return_type, .. }) = main_decl {
            let has_args = !params.is_empty();
            let is_effect = effect.unwrap_or(false);
            let ret_str = self.gen_type(return_type);
            let returns_result = ret_str.starts_with("Result<") || is_effect;

            self.emitln("fn main() {");
            self.indent += 1;
            self.emitln("let t = std::thread::Builder::new().stack_size(8 * 1024 * 1024).spawn(|| {");
            self.indent += 1;
            if has_args {
                self.emitln("let args: Vec<String> = std::env::args().collect();");
            }
            let call = if has_args { "almide_main(args)" } else { "almide_main()" };
            if returns_result {
                self.emitln(&format!("if let Err(e) = {} {{", call));
                self.indent += 1;
                self.emitln("eprintln!(\"{}\", e);");
                self.emitln("std::process::exit(1);");
                self.indent -= 1;
                self.emitln("}");
            } else {
                self.emitln(&format!("{};", call));
            }
            self.indent -= 1;
            self.emitln("}).unwrap();");
            self.emitln("t.join().unwrap();");
            self.indent -= 1;
            self.emitln("}");
        }
    }

    fn emit_user_module(&mut self, name: &str, prog: &Program) {
        self.emitln(&format!("mod {} {{", name));
        self.indent += 1;
        self.emitln("use super::*;");
        self.emitln("");

        for decl in &prog.decls {
            match decl {
                Decl::Fn { name: fn_name, params, return_type, body, effect, .. } => {
                    // Emit with pub visibility
                    let is_effect = effect.unwrap_or(false);
                    let params_str: Vec<String> = params.iter()
                        .map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty)))
                        .collect();
                    let ret_str = self.gen_type(return_type);

                    let actual_ret = if is_effect && !ret_str.starts_with("Result<") {
                        if ret_str == "()" {
                            "Result<(), String>".to_string()
                        } else {
                            format!("Result<{}, String>", ret_str)
                        }
                    } else {
                        ret_str.clone()
                    };

                    self.emitln(&format!("pub fn {}({}) -> {} {{", fn_name, params_str.join(", "), actual_ret));
                    self.indent += 1;
                    let prev_effect = self.in_effect;
                    self.in_effect = is_effect;
                    let body_code = self.gen_expr(body);

                    if is_effect {
                        if ret_str.starts_with("Result<") {
                            self.emitln(&body_code);
                        } else if ret_str == "()" {
                            self.emitln(&format!("{};", body_code));
                            self.emitln("Ok(())");
                        } else {
                            self.emitln(&format!("Ok({})", body_code));
                        }
                    } else {
                        self.emitln(&body_code);
                    }

                    self.in_effect = prev_effect;
                    self.indent -= 1;
                    self.emitln("}");
                    self.emitln("");
                }
                Decl::Type { name: type_name, ty, deriving } => {
                    // Emit type with pub
                    self.emit_indent();
                    self.out.push_str("pub ");
                    // Remove the indent since emit_type_decl adds its own
                    self.emit_type_decl(type_name, ty, deriving);
                }
                _ => {}
            }
        }

        self.indent -= 1;
        self.emitln("}");
    }

    fn emit_runtime(&mut self) {
        self.emitln("use std::collections::HashMap;");
        self.emitln("trait AlmideConcat<Rhs> { type Output; fn concat(self, rhs: Rhs) -> Self::Output; }");
        self.emitln("impl AlmideConcat<String> for String { type Output = String; fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<&str> for String { type Output = String; fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<String> for &str { type Output = String; fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<&str> for &str { type Output = String; fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl<T: Clone> AlmideConcat<Vec<T>> for Vec<T> { type Output = Vec<T>; fn concat(self, rhs: Vec<T>) -> Vec<T> { let mut r = self; r.extend(rhs); r } }");
        self.emitln("macro_rules! almide_eq { ($a:expr, $b:expr) => { ($a) == ($b) }; }");
        self.emitln("macro_rules! almide_ne { ($a:expr, $b:expr) => { ($a) != ($b) }; }");
        self.emitln("");
        self.out.push_str(JSON_RUNTIME);
        self.emitln("");
    }

    fn emit_decl(&mut self, decl: &Decl) {
        match decl {
            Decl::Module { path } => {
                self.emitln(&format!("// module: {}", path.join(".")));
            }
            Decl::Import { path, .. } => {
                self.emitln(&format!("// import: {}", path.join(".")));
            }
            Decl::Type { name, ty, deriving } => {
                self.emit_type_decl(name, ty, deriving);
            }
            Decl::Fn { name, params, return_type, body, effect, .. } => {
                self.emit_fn_decl(name, params, return_type, body, effect.unwrap_or(false));
            }
            Decl::Impl { trait_, for_, methods } => {
                self.emitln(&format!("// impl {} for {}", trait_, for_));
                for m in methods {
                    self.emit_decl(m);
                }
            }
            Decl::Test { name, body } => {
                self.emitln("#[test]");
                let safe_name = name.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>();
                let prev_effect = self.in_effect;
                let prev_test = self.in_test;
                self.in_effect = true;
                self.in_test = true;
                let expr = self.gen_expr(body);
                let has_question = expr.contains("?");
                if has_question {
                    self.emitln(&format!("fn test_{}() -> Result<(), String> {{", safe_name));
                    self.indent += 1;
                    self.emitln(&format!("{};", expr));
                    self.emitln("Ok(())");
                } else {
                    self.emitln(&format!("fn test_{}() {{", safe_name));
                    self.indent += 1;
                    self.emitln(&format!("{};", expr));
                }
                self.in_effect = prev_effect;
                self.in_test = prev_test;
                self.indent -= 1;
                self.emitln("}");
            }
            _ => {}
        }
    }

    fn emit_type_decl(&mut self, name: &str, ty: &TypeExpr, _deriving: &Option<Vec<String>>) {
        match ty {
            TypeExpr::Record { fields } => {
                self.emitln("#[derive(Debug, Clone, PartialEq)]");
                self.emitln(&format!("struct {} {{", name));
                self.indent += 1;
                for f in fields {
                    let ty_str = self.gen_type(&f.ty);
                    self.emitln(&format!("{}: {},", f.name, ty_str));
                }
                self.indent -= 1;
                self.emitln("}");
            }
            TypeExpr::Simple { .. } | TypeExpr::Generic { .. } => {
                let ty_str = self.gen_type(ty);
                self.emitln(&format!("type {} = {};", name, ty_str));
            }
            TypeExpr::Newtype { inner } => {
                let ty_str = self.gen_type(inner);
                self.emitln(&format!("struct {}({});", name, ty_str));
            }
            TypeExpr::Variant { cases } => {
                self.emitln("#[derive(Debug, Clone, PartialEq)]");
                self.emitln(&format!("enum {} {{", name));
                self.indent += 1;
                for case in cases {
                    match case {
                        VariantCase::Unit { name: cname } => {
                            self.emitln(&format!("{},", cname));
                        }
                        VariantCase::Tuple { name: cname, fields } => {
                            let fs: Vec<String> = fields.iter().map(|f| self.gen_type(f)).collect();
                            self.emitln(&format!("{}({}),", cname, fs.join(", ")));
                        }
                        VariantCase::Record { name: cname, fields } => {
                            let fs: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, self.gen_type(&f.ty))).collect();
                            self.emitln(&format!("{} {{ {} }},", cname, fs.join(", ")));
                        }
                    }
                }
                self.indent -= 1;
                self.emitln("}");
                // impl Display for error types (so they work with .to_string())
                self.emitln(&format!("impl std::fmt::Display for {} {{", name));
                self.emitln(&format!("    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {{ write!(f, \"{{:?}}\", self) }}"));
                self.emitln("}");
                // Allow using variant names without prefix
                self.emitln(&format!("use {}::*;", name));
            }
            _ => {
                self.emitln(&format!("// type {} (unsupported)", name));
            }
        }
    }

    fn emit_fn_decl(&mut self, name: &str, params: &[Param], ret_type: &TypeExpr, body: &Expr, is_effect: bool) {
        let fn_name = if name == "main" { "almide_main".to_string() } else { name.replace('?', "_qm_") };
        let ret_str = self.gen_type(ret_type);
        let is_unit_ret = ret_str == "()";

        let actual_ret = if is_effect {
            if ret_str.starts_with("Result<") {
                // Already a Result type, don't double-wrap
                ret_str.clone()
            } else if is_unit_ret {
                "Result<(), String>".to_string()
            } else {
                format!("Result<{}, String>", ret_str)
            }
        } else {
            ret_str.clone()
        };

        let params_str: Vec<String> = params.iter()
            .filter(|p| p.name != "self")
            .map(|p| {
                let ty = self.gen_type(&p.ty);
                format!("{}: {}", p.name, ty)
            })
            .collect();

        self.emitln(&format!("fn {}({}) -> {} {{", fn_name, params_str.join(", "), actual_ret));
        self.indent += 1;

        let prev_effect = self.in_effect;
        // Treat fn as effect if explicitly marked OR if it returns Result
        self.in_effect = is_effect || ret_str.starts_with("Result<");

        match body {
            Expr::Block { stmts, expr: final_expr } => {
                self.emit_stmts(stmts);
                let ret_is_result = ret_str.starts_with("Result<");
                if is_effect {
                    if ret_is_result {
                        // Return type is already Result - check if final expr already returns Result
                        if let Some(fe) = final_expr {
                            let already_result = matches!(fe.as_ref(),
                                Expr::Ok { .. } | Expr::Err { .. } | Expr::Match { .. } | Expr::If { .. }
                            );
                            let e = self.gen_expr(fe);
                            if already_result {
                                self.emitln(&e);
                            } else {
                                self.emitln(&format!("Ok({})", e));
                            }
                        } else {
                            self.emitln("Ok(())");
                        }
                    } else if let Some(fe) = final_expr {
                        if is_unit_ret {
                            let e = self.gen_expr(fe);
                            self.emitln(&format!("{};", e));
                            self.emitln("Ok(())");
                        } else {
                            let e = self.gen_expr(fe);
                            self.emitln(&format!("Ok({})", e));
                        }
                    } else {
                        self.emitln("Ok(())");
                    }
                } else {
                    if let Some(fe) = final_expr {
                        let e = self.gen_expr(fe);
                        self.emitln(&e);
                    }
                }
            }
            _ => {
                let expr = self.gen_expr(body);
                if is_effect {
                    let ret_is_result = ret_str.starts_with("Result<");
                    if ret_is_result {
                        // Already returns Result, don't wrap
                        self.emitln(&expr);
                    } else {
                        self.emitln(&format!("Ok({})", expr));
                    }
                } else {
                    self.emitln(&expr);
                }
            }
        }

        self.in_effect = prev_effect;
        self.indent -= 1;
        self.emitln("}");
    }

    fn emit_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            let s = self.gen_stmt(stmt);
            self.emitln(&s);
        }
    }

    fn gen_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Simple { name } => match name.as_str() {
                "Int" => "i64".to_string(),
                "Float" => "f64".to_string(),
                "String" => "String".to_string(),
                "Bool" => "bool".to_string(),
                "Unit" => "()".to_string(),
                "IoError" => "String".to_string(),
                "Path" => "String".to_string(),
                "Json" => "AlmideJson".to_string(),
                other => other.to_string(),
            },
            TypeExpr::Generic { name, args } => match name.as_str() {
                "List" => format!("Vec<{}>", self.gen_type(&args[0])),
                "Option" => format!("Option<{}>", self.gen_type(&args[0])),
                "Result" => format!("Result<{}, String>", self.gen_type(&args[0])),
                "Map" => format!("HashMap<{}, {}>", self.gen_type(&args[0]), self.gen_type(&args[1])),
                other => format!("{}<{}>", other, args.iter().map(|a| self.gen_type(a)).collect::<Vec<_>>().join(", ")),
            },
            TypeExpr::Record { fields } => {
                let fs: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, self.gen_type(&f.ty))).collect();
                format!("{{ {} }}", fs.join(", "))
            }
            TypeExpr::Fn { params, ret } => {
                let ps: Vec<String> = params.iter().map(|p| self.gen_type(p)).collect();
                format!("fn({}) -> {}", ps.join(", "), self.gen_type(ret))
            }
            TypeExpr::Newtype { inner } => self.gen_type(inner),
            TypeExpr::Variant { cases: _ } => "/* variant */".to_string(),
        }
    }

    /// Generate expression as function argument — clone Idents to avoid move
    fn gen_arg(&self, expr: &Expr) -> String {
        match expr {
            Expr::Ident { .. } => format!("{}.clone()", self.gen_expr(expr)),
            _ => self.gen_expr(expr),
        }
    }

    fn gen_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::Int { raw, .. } => {
                if let Ok(n) = raw.parse::<u128>() {
                    if n > i64::MAX as u128 {
                        // Wrap to i64 — Almide Int is i64, large literals wrap automatically
                        format!("{}i64", n as i64)
                    } else {
                        format!("{}i64", raw)
                    }
                } else {
                    raw.clone()
                }
            }
            Expr::Float { value } => format!("{:?}f64", value),
            Expr::String { value } => format!("{:?}.to_string()", value),
            Expr::InterpolatedString { value } => {
                // Convert ${expr} to Rust format!("{}", expr) style
                let mut fmt = String::new();
                let mut args = Vec::new();
                let mut chars = value.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '$' && chars.peek() == Some(&'{') {
                        chars.next(); // skip {
                        let mut expr_str = String::new();
                        let mut depth = 1;
                        while let Some(ch) = chars.next() {
                            if ch == '{' { depth += 1; }
                            if ch == '}' { depth -= 1; if depth == 0 { break; } }
                            expr_str.push(ch);
                        }
                        fmt.push_str("{}");
                        // Parse the interpolated expression and re-emit as Rust
                        let tokens = crate::lexer::Lexer::tokenize(&expr_str);
                        let mut parser = crate::parser::Parser::new(tokens);
                        if let Ok(parsed_expr) = parser.parse_single_expr() {
                            args.push(self.gen_expr(&parsed_expr));
                        } else {
                            args.push(expr_str);
                        }
                    } else if c == '{' {
                        fmt.push_str("{{");
                    } else if c == '}' {
                        fmt.push_str("}}");
                    } else {
                        fmt.push(c);
                    }
                }
                if args.is_empty() {
                    format!("\"{}\".to_string()", fmt)
                } else {
                    format!("format!(\"{}\", {})", fmt, args.join(", "))
                }
            }
            Expr::Bool { value } => format!("{}", value),
            Expr::Ident { name } => name.replace('?', "_qm_"),
            Expr::TypeName { name } => name.clone(),
            Expr::Unit => "()".to_string(),
            Expr::None => "None".to_string(),
            Expr::Some { expr } => format!("Some({})", self.gen_expr(expr)),
            Expr::Ok { expr } => {
                if self.in_do_block.get() {
                    // In do blocks, ok(expr) just unwraps to expr (since do auto-wraps in Ok)
                    self.gen_expr(expr)
                } else if self.in_effect {
                    format!("Ok({})", self.gen_expr(expr))
                } else if matches!(expr.as_ref(), Expr::Unit) {
                    "()".to_string()
                } else {
                    format!("Ok({})", self.gen_expr(expr))
                }
            }
            Expr::Err { expr } => {
                let msg = self.gen_expr(expr);
                if self.in_effect && !self.in_test && !self.in_do_block.get() {
                    format!("return Err({}.to_string())", msg)
                } else {
                    format!("Err({}.to_string())", msg)
                }
            }

            Expr::List { elements } => {
                let elems: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                format!("vec![{}]", elems.join(", "))
            }
            Expr::Record { fields } => {
                let fs: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value))).collect();
                format!("{{ {} }}", fs.join(", "))
            }

            Expr::Binary { op, left, right } => self.gen_binary(op, left, right),
            Expr::Unary { op, operand } => {
                let o = self.gen_expr(operand);
                match op.as_str() {
                    "not" => format!("!({})", o),
                    _ => format!("{}{}", op, o),
                }
            }

            Expr::If { cond, then, else_ } => {
                let c = self.gen_expr(cond);
                let t = self.gen_expr(then);
                let e = self.gen_expr(else_);
                format!("if {} {{ {} }} else {{ {} }}", c, t, e)
            }

            Expr::Call { callee, args } => self.gen_call(callee, args),

            Expr::Member { object, field } => {
                let obj = self.gen_expr(object);
                format!("{}.{}", obj, field)
            }

            Expr::Pipe { left, right } => {
                let l = self.gen_expr(left);
                match right.as_ref() {
                    Expr::Call { callee, args } => {
                        // Reconstruct as a full call with pipe-left as first arg
                        let mut all_args = Vec::new();
                        all_args.push(left.as_ref().clone());
                        all_args.extend(args.iter().cloned());
                        let full_call = Expr::Call {
                            callee: callee.clone(),
                            args: all_args,
                        };
                        self.gen_expr(&full_call)
                    }
                    _ => {
                        let r = self.gen_expr(right);
                        format!("{}({})", r, l)
                    }
                }
            }

            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                let b = self.gen_expr(body);
                format!("|{}| {{ {} }}", ps.join(", "), b)
            }

            Expr::Match { subject, arms } => self.gen_match(subject, arms),

            Expr::Block { stmts, expr } => self.gen_block(stmts, expr.as_deref()),
            Expr::DoBlock { stmts, expr } => self.gen_do_block(stmts, expr.as_deref()),
            Expr::ForIn { var, iterable, body } => {
                let iter_str = self.gen_expr(iterable);
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_stmt(s)))
                    .collect();
                format!("for {var} in ({iter_str}).clone() {{\n{}\n}}", stmts_str.join("\n"))
            }

            Expr::Paren { expr } => format!("({})", self.gen_expr(expr)),
            Expr::Try { expr } => {
                // In effect fn: use ?, otherwise just eval
                if self.in_effect {
                    format!("({}?)", self.gen_expr(expr))
                } else {
                    self.gen_expr(expr)
                }
            }
            Expr::Hole => "todo!()".to_string(),
            Expr::Todo { message } => format!("todo!(\"{}\")", message),
            Expr::Placeholder => "_".to_string(),

            _ => format!("todo!(/* unsupported */)")
        }
    }

    /// Generate expression in u64 wrapping context (for hash/BigInt arithmetic)
    /// Almide's % 2^64 pattern maps to u64 wrapping arithmetic
    fn gen_expr_u64_wrapping(&self, expr: &Expr) -> String {
        match expr {
            Expr::Binary { op, left, right } if op == "^" || op == "*" || op == "%" => {
                // Check if % 2^64 — this is a no-op in u64 arithmetic
                if op == "%" && self.is_pow2_64(right) {
                    return self.gen_expr_u64_wrapping(left);
                }
                let l = self.gen_expr_u64_wrapping(left);
                let r = self.gen_expr_u64_wrapping(right);
                match op.as_str() {
                    "*" => format!("(({}).wrapping_mul({}))", l, r),
                    "^" => format!("(({}) ^ ({}))", l, r),
                    "%" => format!("(({}) % ({}))", l, r),
                    _ => format!("(({}) {} ({}))", l, op, r),
                }
            }
            Expr::Paren { expr: inner } => self.gen_expr_u64_wrapping(inner),
            Expr::Int { raw, .. } => {
                if let Ok(n) = raw.parse::<u128>() {
                    if n > u64::MAX as u128 {
                        format!("{}u64", n % (u64::MAX as u128 + 1))
                    } else {
                        format!("{}u64", raw)
                    }
                } else {
                    raw.clone()
                }
            }
            _ => {
                let e = self.gen_expr(expr);
                format!("(({}) as u64)", e)
            }
        }
    }

    fn is_pow2_64(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Int { raw, .. } => raw == "18446744073709551616",
            Expr::Paren { expr: inner } => self.is_pow2_64(inner),
            _ => false,
        }
    }

    fn is_bigint_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Int { raw, .. } => {
                if let Ok(n) = raw.parse::<u128>() { n > i64::MAX as u128 } else { false }
            }
            Expr::Binary { op, left, right } if op == "^" || op == "*" || op == "%" => {
                self.is_bigint_expr(left) || self.is_bigint_expr(right)
            }
            Expr::Paren { expr: inner } => self.is_bigint_expr(inner),
            _ => false,
        }
    }

    fn gen_binary(&self, op: &str, left: &Expr, right: &Expr) -> String {
        match op {
            "++" => {
                let l = self.gen_arg(left);
                let r = self.gen_arg(right);
                format!("AlmideConcat::concat({}, {})", l, r)
            }
            "^" | "%" => {
                let left_big = self.is_bigint_expr(left);
                let right_big = self.is_bigint_expr(right);
                let needs_wrapping = left_big || right_big;
                if needs_wrapping {
                    if op == "%" && self.is_pow2_64(right) {
                        let inner = self.gen_expr_u64_wrapping(left);
                        return format!("(({}) as i64)", inner);
                    }
                    let l = self.gen_expr_u64_wrapping(left);
                    let r = self.gen_expr_u64_wrapping(right);
                    format!("(({} {} {}) as i64)", l, op, r)
                } else {
                    let l = self.gen_expr(left);
                    let r = self.gen_expr(right);
                    format!("({} {} {})", l, op, r)
                }
            }
            "*" => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                format!("(({}).wrapping_mul({}))", l, r)
            }
            "==" => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                format!("almide_eq!({}, {})", l, r)
            }
            "!=" => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                format!("almide_ne!({}, {})", l, r)
            }
            "and" => { let l = self.gen_expr(left); let r = self.gen_expr(right); format!("({} && {})", l, r) }
            "or" => { let l = self.gen_expr(left); let r = self.gen_expr(right); format!("({} || {})", l, r) }
            _ => { let l = self.gen_expr(left); let r = self.gen_expr(right); format!("({} {} {})", l, op, r) }
        }
    }

    fn resolve_ufcs_module(method: &str) -> Option<&'static str> {
        stdlib::resolve_ufcs_module(method)
    }

    fn gen_call(&self, callee: &Expr, args: &[Expr]) -> String {
        // Handle module calls
        if let Expr::Member { object, field } = callee {
            if let Expr::Ident { name: module } = object.as_ref() {
                let is_stdlib = stdlib::is_stdlib_module(module);
                let is_user_module = self.user_modules.contains(module);
                if is_stdlib || is_user_module {
                    return self.gen_module_call(module, field, args);
                }
                // UFCS: variable.method(args) => module.method(variable, args)
                if let Some(resolved) = Self::resolve_ufcs_module(field) {
                    let mut new_args = vec![Expr::Ident { name: module.clone() }];
                    new_args.extend(args.iter().cloned());
                    return self.gen_module_call(resolved, field, &new_args);
                }
            } else {
                // Non-ident receiver: expr.method(args) => module.method(expr, args)
                if let Some(resolved) = Self::resolve_ufcs_module(field) {
                    let mut new_args = vec![object.as_ref().clone()];
                    new_args.extend(args.iter().cloned());
                    return self.gen_module_call(resolved, field, &new_args);
                }
            }
        }

        // Handle built-in functions
        if let Expr::Ident { name } = callee {
            match name.as_str() {
                "println" => {
                    let arg = self.gen_expr(&args[0]);
                    return format!("println!(\"{{}}\", {})", arg);
                }
                "eprintln" => {
                    let arg = self.gen_expr(&args[0]);
                    return format!("eprintln!(\"{{}}\", {})", arg);
                }
                "err" => {
                    let msg = self.gen_expr(&args[0]);
                    return format!("return Err(({}).to_string())", msg);
                }
                "assert_eq" => {
                    let a = self.gen_expr(&args[0]);
                    let b = self.gen_expr(&args[1]);
                    // If one side is an empty list, use .is_empty() check instead
                    if matches!(&args[1], Expr::List { elements } if elements.is_empty()) {
                        return format!("assert!(({}).is_empty(), \"expected empty list but got {{:?}}\", {})", a, a);
                    }
                    if matches!(&args[0], Expr::List { elements } if elements.is_empty()) {
                        return format!("assert!(({}).is_empty(), \"expected empty list but got {{:?}}\", {})", b, b);
                    }
                    return format!("assert_eq!({}, {})", a, b);
                }
                "assert" => {
                    let a = self.gen_expr(&args[0]);
                    return format!("assert!({})", a);
                }
                "unwrap_or" => {
                    let a = self.gen_expr(&args[0]);
                    let b = self.gen_expr(&args[1]);
                    return format!("({}).unwrap_or({})", a, b);
                }
                _ => {}
            }
        }

        let callee_str = self.gen_expr(callee);
        let args_str: Vec<String> = args.iter().map(|a| self.gen_arg(a)).collect();
        let call = format!("{}({})", callee_str, args_str.join(", "));
        // Auto-propagate ? for effect fn calls within effect context
        if self.in_effect {
            if let Expr::Ident { name } = callee {
                if self.effect_fns.contains(name) {
                    return format!("{}?", call);
                }
                // In do blocks, also auto-unwrap calls to Result-returning functions
                if self.in_do_block.get() && self.result_fns.contains(name) {
                    return format!("{}?", call);
                }
            }
        }
        call
    }

    fn gen_module_call(&self, module: &str, func: &str, args: &[Expr]) -> String {
        let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
        match module {
            "fs" => match func {
                "read_text" => format!("std::fs::read_to_string(&*{}).map_err(|e| e.to_string())?", args_str[0]),
                "write" => format!("std::fs::write(&*{}, &*{}).map_err(|e| e.to_string())?", args_str[0], args_str[1]),
                "write_bytes" => format!("std::fs::write(&*{}, &{}).map_err(|e| e.to_string())?", args_str[0], args_str[1]),
                "read_bytes" => format!("std::fs::read(&*{}).map_err(|e| e.to_string())?", args_str[0]),
                "exists?" | "exists_qm_" => format!("std::path::Path::new(&*{}).exists()", args_str[0]),
                "mkdir_p" => format!("std::fs::create_dir_all(&*{}).map_err(|e| e.to_string())?", args_str[0]),
                "append" => format!("{{ let prev = std::fs::read_to_string(&*{}).unwrap_or_default(); std::fs::write(&*{}, format!(\"{{}}{{}}\", prev, {})).map_err(|e| e.to_string())?; }}", args_str[0], args_str[0], args_str[1]),
                "read_lines" => format!("{{ let s = std::fs::read_to_string(&*{}).map_err(|e| e.to_string())?; s.split('\\n').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect::<Vec<String>>() }}", args_str[0]),
                "remove" => format!("std::fs::remove_file(&*{}).map_err(|e| e.to_string())?", args_str[0]),
                "list_dir" => format!("{{ let mut v: Vec<String> = std::fs::read_dir(&*{}).map_err(|e| e.to_string())?.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect(); v.sort(); v }}", args_str[0]),
                _ => format!("/* fs.{} */ todo!()", func),
            },
            "string" => match func {
                "trim" => format!("({}).trim().to_string()", args_str[0]),
                "split" => format!("{{ let __delim = &*{}; if __delim.is_empty() {{ ({}).chars().map(|c| c.to_string()).collect::<Vec<String>>() }} else {{ ({}).split(__delim).map(|s| s.to_string()).collect::<Vec<String>>() }} }}", args_str[1], args_str[0], args_str[0]),
                "join" => format!("({}).join(&*{})", args_str[0], args_str[1]),
                "len" => format!("(({}).len() as i64)", args_str[0]),
                "contains" => format!("({}).contains(&*{})", args_str[0], args_str[1]),
                "starts_with?" | "starts_with_qm_" | "starts_with" => format!("({}).starts_with(&*{})", args_str[0], args_str[1]),
                "ends_with?" | "ends_with_qm_" | "ends_with" => format!("({}).ends_with(&*{})", args_str[0], args_str[1]),
                "slice" => {
                    if args_str.len() == 3 {
                        format!("({}).chars().skip({} as usize).take(({} - {}) as usize).collect::<String>()", args_str[0], args_str[1], args_str[2], args_str[1])
                    } else {
                        format!("({}).chars().skip({} as usize).collect::<String>()", args_str[0], args_str[1])
                    }
                }
                "pad_left" => format!("format!(\"{{:0>width$}}\", {}, width = {} as usize)", args_str[0], args_str[1]),
                "to_bytes" => format!("({}).as_bytes().iter().map(|&b| b as i64).collect::<Vec<i64>>()", args_str[0]),
                "to_upper" => format!("({}).to_uppercase()", args_str[0]),
                "to_lower" => format!("({}).to_lowercase()", args_str[0]),
                "to_int" => format!("({}).parse::<i64>().map_err(|e| e.to_string())?", args_str[0]),
                "replace" => format!("({}).replace(&*{}, &*{})", args_str[0], args_str[1], args_str[2]),
                "char_at" => format!("({}).chars().nth({} as usize).map(|c| c.to_string())", args_str[0], args_str[1]),
                "lines" => format!("({}).split('\\n').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect::<Vec<String>>()", args_str[0]),
                _ => format!("/* string.{} */ todo!()", func),
            },
            "list" => {
                // For list operations, inline the lambda body directly to avoid type annotation issues
                let inline_lambda = |lambda_arg: &Expr, arity: usize| -> (Vec<String>, String) {
                    if let Expr::Lambda { params, body } = lambda_arg {
                        let names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                        let body_str = self.gen_expr(body);
                        (names, body_str)
                    } else {
                        let f = self.gen_expr(lambda_arg);
                        if arity == 1 {
                            (vec!["__x".to_string()], format!("({})((__x).clone())", f))
                        } else {
                            (vec!["__a".to_string(), "__b".to_string()], format!("({})(__a, __b.clone())", f))
                        }
                    }
                };
                match func {
                    "len" => format!("(({}).len() as i64)", args_str[0]),
                    "get" => format!("({}).get({} as usize).cloned()", args_str[0], args_str[1]),
                    "get_or" => format!("({}).get({} as usize).cloned().unwrap_or({})", args_str[0], args_str[1], args_str[2]),
                    "sort" => format!("{{ let mut v = ({}).to_vec(); v.sort(); v }}", args_str[0]),
                    "reverse" => format!("{{ let mut v = ({}).to_vec(); v.reverse(); v }}", args_str[0]),
                    "any" => {
                        let (names, body) = inline_lambda(&args[1], 1);
                        format!("({}).iter().any(|{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "all" => {
                        let (names, body) = inline_lambda(&args[1], 1);
                        format!("({}).iter().all(|{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "contains" => format!("({}).contains(&{})", args_str[0], args_str[1]),
                    "each" => {
                        let (names, body) = inline_lambda(&args[1], 1);
                        format!("{{ for {} in ({}).iter().cloned() {{ {} ; }} }}", names[0], args_str[0], body)
                    }
                    "map" => {
                        let (names, body) = inline_lambda(&args[1], 1);
                        // If in effect context and body contains ?, use try_collect pattern
                        if self.in_effect && body.contains("?") {
                            format!("({}).clone().into_iter().map(|{}| -> Result<_, String> {{ Ok({{ {} }}) }}).collect::<Result<Vec<_>, _>>()?", args_str[0], names[0], body)
                        } else {
                            format!("({}).clone().into_iter().map(|{}| {{ {} }}).collect::<Vec<_>>()", args_str[0], names[0], body)
                        }
                    }
                    "filter" => {
                        let (names, body) = inline_lambda(&args[1], 1);
                        format!("({}).clone().into_iter().filter(|{}| {{ let {} = {}.clone(); {} }}).collect::<Vec<_>>()", args_str[0], names[0], names[0], names[0], body)
                    }
                    "find" => {
                        let (names, body) = inline_lambda(&args[1], 1);
                        format!("({}).clone().into_iter().find(|{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "fold" => {
                        let (names, body) = inline_lambda(&args[2], 2);
                        let init = &args_str[1];
                        // If body contains ?, use try_fold pattern
                        if self.in_effect && body.contains("?") {
                            format!("({}).clone().into_iter().try_fold({}, |{}, {}| -> Result<_, String> {{ Ok({{ {} }}) }})?", args_str[0], init, names[0], names[1], body)
                        } else {
                            // Add type annotation on accumulator if it's a Result (Rust can't infer)
                            let acc_typed = if init.starts_with("Ok(") || init.starts_with("Err(") {
                                format!("{}: Result<_, String>", names[0])
                            } else {
                                names[0].clone()
                            };
                            format!("({}).clone().into_iter().fold({}, |{}, {}| {{ {} }})", args_str[0], init, acc_typed, names[1], body)
                        }
                    }
                    _ => format!("/* list.{} */ todo!()", func),
                }
            },
            "map" => match func {
                "new" => "HashMap::new()".to_string(),
                "get" => format!("({}).get(&{}).cloned()", args_str[0], args_str[1]),
                "get_or" => format!("({}).get(&{}).cloned().unwrap_or({})", args_str[0], args_str[1], args_str[2]),
                "set" => format!("{{ let mut m = ({}).clone(); m.insert({}, {}); m }}", args_str[0], args_str[1], args_str[2]),
                "contains" => format!("({}).contains_key(&{})", args_str[0], args_str[1]),
                "remove" => format!("{{ let mut m = ({}).clone(); m.remove(&{}); m }}", args_str[0], args_str[1]),
                "keys" => format!("{{ let mut v: Vec<_> = ({}).keys().cloned().collect(); v.sort(); v }}", args_str[0]),
                "values" => format!("({}).values().cloned().collect::<Vec<_>>()", args_str[0]),
                "len" => format!("(({}).len() as i64)", args_str[0]),
                "entries" => format!("({}).iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()", args_str[0]),
                "from_list" => {
                    let inline_lambda = |lambda_arg: &Expr| -> (Vec<String>, String) {
                        if let Expr::Lambda { params, body } = lambda_arg {
                            let names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                            let body_str = self.gen_expr(body);
                            (names, body_str)
                        } else {
                            let f = self.gen_expr(lambda_arg);
                            (vec!["__x".to_string()], format!("({})((__x).clone())", f))
                        }
                    };
                    let (names, body) = inline_lambda(&args[1]);
                    format!("({}).clone().into_iter().map(|{}| {{ {} }}).collect::<HashMap<_, _>>()", args_str[0], names[0], body)
                }
                _ => format!("/* map.{} */ todo!()", func),
            },
            "int" => match func {
                "to_hex" => format!("format!(\"{{:x}}\", {} as u64)", args_str[0]),
                "to_string" => format!("({}).to_string()", args_str[0]),
                _ => format!("/* int.{} */ todo!()", func),
            },
            "float" => match func {
                "to_string" => format!("({}).to_string()", args_str[0]),
                "to_int" => format!("(({}) as i64)", args_str[0]),
                "round" => format!("({}).round()", args_str[0]),
                "floor" => format!("({}).floor()", args_str[0]),
                "ceil" => format!("({}).ceil()", args_str[0]),
                "abs" => format!("({}).abs()", args_str[0]),
                "sqrt" => format!("({}).sqrt()", args_str[0]),
                "parse" => format!("({}).parse::<f64>().map_err(|e| e.to_string())?", args_str[0]),
                _ => format!("/* float.{} */ todo!()", func),
            },
            "env" => match func {
                "unix_timestamp" => {
                    "(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)".to_string()
                }
                "args" => "std::env::args().collect::<Vec<String>>()".to_string(),
                _ => format!("/* env.{} */ todo!()", func),
            },
            "json" => match func {
                "parse" => format!("almide_json_parse(&{})?", args_str[0]),
                "stringify" => format!("almide_json_stringify(&{})", args_str[0]),
                "get" => format!("almide_json_get(&{}, &{})", args_str[0], args_str[1]),
                "get_string" => format!("almide_json_get_string(&{}, &{})", args_str[0], args_str[1]),
                "get_int" => format!("almide_json_get_int(&{}, &{})", args_str[0], args_str[1]),
                "get_bool" => format!("almide_json_get_bool(&{}, &{})", args_str[0], args_str[1]),
                "get_array" => format!("almide_json_get_array(&{}, &{})", args_str[0], args_str[1]),
                "keys" => format!("almide_json_keys(&{})", args_str[0]),
                "to_string" => format!("almide_json_to_string(&{})", args_str[0]),
                "to_int" => format!("almide_json_to_int(&{})", args_str[0]),
                "from_string" => format!("JStr({})", args_str[0]),
                "from_int" => format!("JInt({})", args_str[0]),
                "from_bool" => format!("JBool({})", args_str[0]),
                "null" => "JNull".to_string(),
                "array" => format!("JArray({})", args_str[0]),
                "from_map" => format!("JObject({})", args_str[0]),
                _ => format!("/* json.{} */ todo!()", func),
            },
            "path" => match func {
                "join" => format!("{{ let p = std::path::Path::new(&*{}).join(&*{}); p.to_string_lossy().to_string() }}", args_str[0], args_str[1]),
                "dirname" => format!("std::path::Path::new(&*{}).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()", args_str[0]),
                "basename" => format!("std::path::Path::new(&*{}).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()", args_str[0]),
                "extension" => format!("std::path::Path::new(&*{}).extension().map(|e| e.to_string_lossy().to_string())", args_str[0]),
                "is_absolute?" | "is_absolute_qm_" => format!("std::path::Path::new(&*{}).is_absolute()", args_str[0]),
                _ => format!("/* path.{} */ todo!()", func),
            },
            _ => {
                format!("{}::{}({})", module, func, args_str.join(", "))
            }
        }
    }

    fn has_string_literal_in_option_pattern(arms: &[MatchArm]) -> bool {
        fn check_pattern(pat: &Pattern) -> bool {
            match pat {
                Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => {
                    check_pattern(inner)
                }
                Pattern::Literal { value } => matches!(&**value, Expr::String { .. }),
                _ => false,
            }
        }
        arms.iter().any(|arm| check_pattern(&arm.pattern))
    }

    fn gen_match(&self, subject: &Expr, arms: &[MatchArm]) -> String {
        let subj = self.gen_expr(subject);
        let needs_deref = Self::has_string_literal_in_option_pattern(arms);
        let subj_expr = if needs_deref {
            format!("{}.as_deref()", subj)
        } else {
            subj
        };
        let mut lines = vec![format!("match {} {{", subj_expr)];
        for arm in arms {
            let pat = self.gen_pattern(&arm.pattern);
            let guard = arm.guard.as_ref().map(|g| format!(" if {}", self.gen_expr(g))).unwrap_or_default();
            let body = self.gen_expr(&arm.body);
            lines.push(format!("    {}{} => {{ {} }}", pat, guard, body));
        }
        lines.push("}".to_string());
        lines.join("\n")
    }

    fn gen_pattern(&self, pat: &Pattern) -> String {
        match pat {
            Pattern::Wildcard => "_".to_string(),
            Pattern::Ident { name } => name.clone(),
            Pattern::Literal { value } => self.gen_pattern_literal(value),
            Pattern::None => "None".to_string(),
            Pattern::Some { inner } => format!("Some({})", self.gen_pattern(inner)),
            Pattern::Ok { inner } => format!("Ok({})", self.gen_pattern(inner)),
            Pattern::Err { inner } => format!("Err({})", self.gen_pattern(inner)),
            Pattern::Constructor { name, args } => {
                if args.is_empty() {
                    format!("{}()", name)
                } else {
                    let ps: Vec<String> = args.iter().map(|p| self.gen_pattern(p)).collect();
                    format!("{}({})", name, ps.join(", "))
                }
            }
            Pattern::RecordPattern { name, fields } => {
                let fs: Vec<String> = fields.iter().map(|f| {
                    if let Some(p) = &f.pattern {
                        format!("{}: {}", f.name, self.gen_pattern(p))
                    } else {
                        f.name.clone()
                    }
                }).collect();
                format!("{} {{ {} }}", name, fs.join(", "))
            }
        }
    }

    fn gen_pattern_literal(&self, expr: &Expr) -> String {
        match expr {
            Expr::String { value } => format!("\"{}\"", value),
            Expr::Int { raw, .. } => format!("{}i64", raw),
            Expr::Float { value } => value.to_string(),
            Expr::Bool { value } => value.to_string(),
            _ => self.gen_expr(expr),
        }
    }

    fn gen_block(&self, stmts: &[Stmt], final_expr: Option<&Expr>) -> String {
        let mut lines = vec!["{".to_string()];
        for stmt in stmts {
            lines.push(format!("    {}", self.gen_stmt(stmt)));
        }
        if let Some(expr) = final_expr {
            lines.push(format!("    {}", self.gen_expr(expr)));
        }
        lines.push("}".to_string());
        lines.join("\n")
    }

    fn is_ok_unit(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Ok { expr } if matches!(expr.as_ref(), Expr::Unit))
    }

    fn gen_do_block(&self, stmts: &[Stmt], final_expr: Option<&Expr>) -> String {
        let has_guard = stmts.iter().any(|s| matches!(s, Stmt::Guard { .. }));
        if has_guard {
            // Check if any guard uses ok(()) as else branch (indicates Result context)
            let has_ok_unit_guard = stmts.iter().any(|s| {
                if let Stmt::Guard { else_, .. } = s { self.is_ok_unit(else_) } else { false }
            });
            let mut lines = vec!["{ loop {".to_string()];
            for stmt in stmts {
                match stmt {
                    Stmt::Guard { cond, else_ } => {
                        let c = self.gen_expr(cond);
                        if matches!(else_, Expr::Unit) || self.is_ok_unit(else_) {
                            lines.push(format!("    if !({}) {{ break; }}", c));
                        } else {
                            let e = self.gen_expr(else_);
                            if e.contains("return ") {
                                lines.push(format!("    if !({}) {{ {}; }}", c, e));
                            } else {
                                lines.push(format!("    if !({}) {{ return {}; }}", c, e));
                            }
                        }
                    }
                    _ => lines.push(format!("    {}", self.gen_stmt(stmt))),
                }
            }
            if let Some(expr) = final_expr {
                lines.push(format!("    {};", self.gen_expr(expr)));
            }
            lines.push("}".to_string());
            // After the loop, provide the appropriate trailing value
            if has_ok_unit_guard && self.in_effect {
                lines.push("Ok::<(), String>(()) }".to_string());
            } else {
                lines.push("}".to_string());
            }
            lines.join("\n")
        } else {
            // In a do block inside a Result-returning function, enable auto-unwrap
            let prev = self.in_do_block.get();
            if self.in_effect {
                self.in_do_block.set(true);
            }
            // Wrap the final expression in Ok() if we're in effect context
            let result = if self.in_effect && final_expr.is_some() {
                let expr = final_expr.unwrap();
                let inner = self.gen_expr(expr);
                let mut lines = vec!["{".to_string()];
                for stmt in stmts {
                    lines.push(format!("    {}", self.gen_stmt(stmt)));
                }
                lines.push(format!("    Ok({})", inner));
                lines.push("}".to_string());
                lines.join("\n")
            } else {
                self.gen_block(stmts, final_expr)
            };
            self.in_do_block.set(prev);
            result
        }
    }

    fn gen_stmt(&self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Let { name, value, .. } => {
                // Use gen_arg to clone Ident values, preventing move issues
                let val = match value {
                    Expr::If { cond, then, else_ } => {
                        let c = self.gen_expr(cond);
                        let t = self.gen_arg(then);
                        let e = self.gen_arg(else_);
                        format!("if {} {{ {} }} else {{ {} }}", c, t, e)
                    }
                    _ => self.gen_expr(value),
                };
                format!("let {} = {};", name, val)
            }
            Stmt::LetDestructure { fields, value } => {
                format!("let ({}) = {};", fields.join(", "), self.gen_expr(value))
            }
            Stmt::Var { name, value, .. } => {
                format!("let mut {} = {};", name, self.gen_expr(value))
            }
            Stmt::Assign { name, value } => {
                format!("{} = {};", name, self.gen_expr(value))
            }
            Stmt::Guard { cond, else_ } => {
                let c = self.gen_expr(cond);
                let e = self.gen_expr(else_);
                if e.contains("return ") {
                    format!("if !({}) {{ {}; }}", c, e)
                } else {
                    format!("if !({}) {{ return {}; }}", c, e)
                }
            }
            Stmt::Expr { expr } => {
                format!("{};", self.gen_expr(expr))
            }
        }
    }
}

pub fn emit(program: &Program, modules: &[(String, Program)]) -> String {
    let mut emitter = Emitter::new();
    emitter.emit_program(program, modules);
    emitter.out
}
