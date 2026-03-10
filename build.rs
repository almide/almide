use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct FnDef {
    params: Vec<Param>,
    #[serde(rename = "return")]
    ret: String,
    #[serde(default)]
    effect: bool,
    #[serde(default)]
    ufcs: bool,
    rust: String,
    #[serde(default)]
    ts: Option<String>,
    /// Sanitized aliases for function names with special chars (e.g. "match?" -> "match_hdlm_qm_")
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Deserialize)]
struct Param {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

/// Find the position of the top-level ", " separator, respecting nested brackets/braces.
fn split_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0;
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            b',' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b' ' => return Some(i),
            _ => {}
        }
    }
    None
}

fn parse_type(s: &str) -> String {
    match s {
        "Int" => "Ty::Int".to_string(),
        "Float" => "Ty::Float".to_string(),
        "String" => "Ty::String".to_string(),
        "Bool" => "Ty::Bool".to_string(),
        "Unit" => "Ty::Unit".to_string(),
        "Unknown" => "Ty::Unknown".to_string(),
        other if other.starts_with("List[") => {
            let inner = &other[5..other.len() - 1];
            format!("Ty::List(Box::new({}))", parse_type(inner))
        }
        other if other.starts_with("Option[") => {
            let inner = &other[7..other.len() - 1];
            format!("Ty::Option(Box::new({}))", parse_type(inner))
        }
        other if other.starts_with("Result[") => {
            let inner = &other[7..other.len() - 1];
            let split_pos = split_top_level_comma(inner);
            let (ok_ty, err_ty) = if let Some(pos) = split_pos {
                (&inner[..pos], inner[pos + 2..].trim())
            } else {
                (inner, "String")
            };
            format!(
                "Ty::Result(Box::new({}), Box::new({}))",
                parse_type(ok_ty),
                parse_type(err_ty)
            )
        }
        other if other.starts_with("Map[") => {
            let inner = &other[4..other.len() - 1];
            let split_pos = split_top_level_comma(inner).expect("Map type needs two type params");
            let key_ty = &inner[..split_pos];
            let val_ty = inner[split_pos + 2..].trim();
            format!("Ty::Map(Box::new({}), Box::new({}))", parse_type(key_ty), parse_type(val_ty))
        }
        other if other.starts_with('{') && other.ends_with('}') => {
            // Record type: {field1: Type1, field2: Type2, ...}
            let inner = &other[1..other.len() - 1];
            let fields: Vec<String> = inner
                .split(", ")
                .map(|field| {
                    let parts: Vec<&str> = field.splitn(2, ": ").collect();
                    format!("(s(\"{}\"), {})", parts[0].trim(), parse_type(parts[1].trim()))
                })
                .collect();
            format!("Ty::Record {{ fields: vec![{}] }}", fields.join(", "))
        }
        other => format!("Ty::Named(s(\"{}\"))", other),
    }
}

fn render_template(template: &str, args: &[String]) -> String {
    // For each {param_name} in template, replace with args_str[i]
    // We need to know param names to map them
    template.to_string()
}

fn render_rust_template(template: &str, params: &[Param]) -> String {
    // 1. Replace {param_name} with a temporary marker \x00N\x00
    let mut fmt_str = template.to_string();
    let mut fmt_args = Vec::new();
    for (i, p) in params.iter().enumerate() {
        let placeholder = format!("{{{}}}", p.name);
        if fmt_str.contains(&placeholder) {
            let marker = format!("\x00{}\x00", fmt_args.len());
            fmt_str = fmt_str.replace(&placeholder, &marker);
            fmt_args.push(format!("args_str[{}]", i));
        }
    }
    // 2. Escape remaining literal braces for format!()
    fmt_str = fmt_str.replace('{', "{{").replace('}', "}}");
    // 3. Restore markers as {}
    for i in 0..fmt_args.len() {
        let marker = format!("\x00{}\x00", i);
        fmt_str = fmt_str.replace(&marker, "{}");
    }
    if fmt_args.is_empty() {
        format!("\"{}\".to_string()", fmt_str)
    } else {
        format!("format!(\"{}\", {})", fmt_str, fmt_args.join(", "))
    }
}

fn main() {
    let defs_dir = Path::new("stdlib/defs");
    if !defs_dir.exists() {
        return;
    }

    let out_dir = Path::new("src/generated");
    fs::create_dir_all(out_dir).unwrap();

    let mut sig_arms = String::new();
    let mut rust_arms = String::new();
    let mut ts_arms = String::new();

    let mut entries: Vec<_> = fs::read_dir(defs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "toml"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let module_name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let content = fs::read_to_string(&path).unwrap();
        let defs: BTreeMap<String, FnDef> = toml::from_str(&content).unwrap();

        for (fn_name, def) in &defs {
            // Generate type signature
            let params_str: Vec<String> = def
                .params
                .iter()
                .map(|p| format!("(s(\"{}\"), {})", p.name, parse_type(&p.ty)))
                .collect();
            let ret_ty = parse_type(&def.ret);

            sig_arms.push_str(&format!(
                "        (\"{module}\", \"{func}\") => FnSig {{ generics: vec![], params: vec![{params}], ret: {ret}, is_effect: {effect} }},\n",
                module = module_name,
                func = fn_name,
                params = params_str.join(", "),
                ret = ret_ty,
                effect = def.effect,
            ));

            // Generate Rust codegen
            let rust_expr = render_rust_template(&def.rust, &def.params);
            rust_arms.push_str(&format!(
                "            (\"{module}\", \"{func}\") => {expr},\n",
                module = module_name,
                func = fn_name,
                expr = rust_expr,
            ));

            // Generate alias arms for Rust codegen (e.g. "match_hdlm_qm_" -> same as "match?")
            for alias in &def.aliases {
                sig_arms.push_str(&format!(
                    "        (\"{module}\", \"{alias}\") => FnSig {{ generics: vec![], params: vec![{params}], ret: {ret}, is_effect: {effect} }},\n",
                    module = module_name,
                    alias = alias,
                    params = params_str.join(", "),
                    ret = ret_ty,
                    effect = def.effect,
                ));
                rust_arms.push_str(&format!(
                    "            (\"{module}\", \"{alias}\") => {expr},\n",
                    module = module_name,
                    alias = alias,
                    expr = rust_expr,
                ));
            }

            // Generate TS codegen (skip if no ts template)
            if let Some(ts_template) = &def.ts {
                let ts_expr = render_rust_template(ts_template, &def.params);
                ts_arms.push_str(&format!(
                    "            (\"{module}\", \"{func}\") => {expr},\n",
                    module = module_name,
                    func = fn_name,
                    expr = ts_expr,
                ));
                for alias in &def.aliases {
                    ts_arms.push_str(&format!(
                        "            (\"{module}\", \"{alias}\") => {expr},\n",
                        module = module_name,
                        alias = alias,
                        expr = ts_expr,
                    ));
                }
            }
        }
    }

    // Write generated files
    let sig_file = format!(
        "// AUTO-GENERATED by build.rs from stdlib/defs/*.toml — DO NOT EDIT\n\
         use crate::types::{{Ty, FnSig}};\n\n\
         pub fn lookup_generated_sig(module: &str, func: &str) -> Option<FnSig> {{\n\
         \x20   let s = |n: &str| -> String {{ n.to_string() }};\n\
         \x20   let sig = match (module, func) {{\n\
         {}\
         \x20       _ => return None,\n\
         \x20   }};\n\
         \x20   Some(sig)\n\
         }}\n",
        sig_arms
    );

    let rust_file = format!(
        "// AUTO-GENERATED by build.rs from stdlib/defs/*.toml — DO NOT EDIT\n\n\
         pub fn gen_generated_call(module: &str, func: &str, args_str: &[String]) -> Option<String> {{\n\
         \x20   let expr = match (module, func) {{\n\
         {}\
         \x20       _ => return None,\n\
         \x20   }};\n\
         \x20   Some(expr)\n\
         }}\n",
        rust_arms
    );

    let ts_file = format!(
        "// AUTO-GENERATED by build.rs from stdlib/defs/*.toml — DO NOT EDIT\n\n\
         pub fn gen_generated_call(module: &str, func: &str, args_str: &[String]) -> Option<String> {{\n\
         \x20   let expr = match (module, func) {{\n\
         {}\
         \x20       _ => return None,\n\
         \x20   }};\n\
         \x20   Some(expr)\n\
         }}\n",
        ts_arms
    );

    fs::write(out_dir.join("stdlib_sigs.rs"), sig_file).unwrap();
    fs::write(out_dir.join("emit_rust_calls.rs"), rust_file).unwrap();
    fs::write(out_dir.join("emit_ts_calls.rs"), ts_file).unwrap();

    // Tell cargo to rerun if defs change
    println!("cargo:rerun-if-changed=stdlib/defs");
    for entry in fs::read_dir(defs_dir).unwrap().filter_map(|e| e.ok()) {
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }
}
