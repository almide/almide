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
    /// Type parameters for generics (e.g. ["A", "B"] or ["K", "V"])
    #[serde(default)]
    type_params: Vec<String>,
    rust: String,
    /// Alternative Rust template when in effect context and body contains `?`
    #[serde(default)]
    rust_effect: Option<String>,
    /// Alternative Rust template when optional params are omitted
    #[serde(default)]
    rust_min: Option<String>,
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    ts_min: Option<String>,
    /// Sanitized aliases for function names with special chars (e.g. "match?" -> "match_hdlm_qm_")
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Deserialize)]
struct Param {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    /// Whether this param can be omitted (triggers rust_min fallback)
    #[serde(default)]
    optional: bool,
}

/// Extract closure arity from type string. Fn[A, B] -> C → Some(2), non-Fn → None
fn closure_arity_from_type(ty: &str) -> Option<usize> {
    if ty.starts_with("Fn[") {
        if let Some(bracket_end) = ty.find("] -> ") {
            let params_str = &ty[3..bracket_end];
            if params_str.is_empty() {
                return Some(0);
            }
            // Count top-level commas (respecting nested brackets)
            let mut count = 1;
            let mut depth = 0;
            for ch in params_str.chars() {
                match ch {
                    '[' | '{' => depth += 1,
                    ']' | '}' => depth -= 1,
                    ',' if depth == 0 => count += 1,
                    _ => {}
                }
            }
            return Some(count);
        }
    }
    None
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

fn parse_type(s: &str, type_params: &[String]) -> String {
    // Check if s is a type variable (declared in type_params)
    if type_params.iter().any(|tp| tp == s) {
        return format!("Ty::TypeVar(s(\"{}\"))", s);
    }
    match s {
        "Int" => "Ty::Int".to_string(),
        "Float" => "Ty::Float".to_string(),
        "String" => "Ty::String".to_string(),
        "Bool" => "Ty::Bool".to_string(),
        "Unit" => "Ty::Unit".to_string(),
        "Unknown" => "Ty::Unknown".to_string(),
        other if other.starts_with("List[") => {
            let inner = &other[5..other.len() - 1];
            format!("Ty::List(Box::new({}))", parse_type(inner, type_params))
        }
        other if other.starts_with("Option[") => {
            let inner = &other[7..other.len() - 1];
            format!("Ty::Option(Box::new({}))", parse_type(inner, type_params))
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
                parse_type(ok_ty, type_params),
                parse_type(err_ty, type_params)
            )
        }
        other if other.starts_with("Map[") => {
            let inner = &other[4..other.len() - 1];
            let split_pos = split_top_level_comma(inner).expect("Map type needs two type params");
            let key_ty = &inner[..split_pos];
            let val_ty = inner[split_pos + 2..].trim();
            format!("Ty::Map(Box::new({}), Box::new({}))", parse_type(key_ty, type_params), parse_type(val_ty, type_params))
        }
        other if other.starts_with("Fn[") && other.contains("] -> ") => {
            // Fn[A, B] -> C
            let arrow_pos = other.rfind("] -> ").unwrap();
            let params_str = &other[3..arrow_pos];
            let ret_str = &other[arrow_pos + 5..];
            let param_types: Vec<String> = params_str
                .split(", ")
                .map(|t| parse_type(t.trim(), type_params))
                .collect();
            format!(
                "Ty::Fn {{ params: vec![{}], ret: Box::new({}) }}",
                param_types.join(", "),
                parse_type(ret_str, type_params)
            )
        }
        other if other.starts_with('(') && other.ends_with(')') => {
            // Tuple type: (A, B, C)
            let inner = &other[1..other.len() - 1];
            let mut elements = Vec::new();
            let mut start = 0;
            let mut depth = 0;
            for (i, ch) in inner.char_indices() {
                match ch {
                    '[' | '(' | '{' => depth += 1,
                    ']' | ')' | '}' => depth -= 1,
                    ',' if depth == 0 => {
                        elements.push(parse_type(inner[start..i].trim(), type_params));
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            elements.push(parse_type(inner[start..].trim(), type_params));
            format!("Ty::Tuple(vec![{}])", elements.join(", "))
        }
        other if other.starts_with('{') && other.ends_with('}') => {
            // Record type: {field1: Type1, field2: Type2, ...}
            let inner = &other[1..other.len() - 1];
            let fields: Vec<String> = inner
                .split(", ")
                .map(|field| {
                    let parts: Vec<&str> = field.splitn(2, ": ").collect();
                    format!("(s(\"{}\"), {})", parts[0].trim(), parse_type(parts[1].trim(), type_params))
                })
                .collect();
            format!("Ty::Record {{ fields: vec![{}] }}", fields.join(", "))
        }
        other => format!("Ty::Named(s(\"{}\"), vec![])", other),
    }
}

/// Check if any param is a closure (Fn type)
fn has_closures(def: &FnDef) -> bool {
    def.params.iter().any(|p| closure_arity_from_type(&p.ty).is_some())
}

/// Check if any param is optional
fn has_optional(def: &FnDef) -> bool {
    def.params.iter().any(|p| p.optional)
}

/// Render a template into a Rust expression string.
/// Handles: {param} -> args_str[i], {f.args} -> closure names, {f.body} -> closure body
/// Returns (let_bindings, expr) where let_bindings are closure setup lines.
fn render_template_full(template: &str, params: &[Param], _use_effect: bool) -> (Vec<String>, String) {
    let mut let_bindings = Vec::new();
    let mut let_binding_done: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Build lookup maps — derive closure info from type encoding (like Obj-C @? for blocks)
    let mut closure_params: std::collections::HashMap<&str, (usize, usize)> = std::collections::HashMap::new(); // name -> (index, arity)
    let mut regular_params: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, p) in params.iter().enumerate() {
        if let Some(arity) = closure_arity_from_type(&p.ty) {
            closure_params.insert(&p.name, (i, arity));
        } else {
            regular_params.insert(&p.name, i);
        }
    }

    // Collect known placeholder names
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in params {
        known.insert(p.name.clone());
        if let Some(arity) = closure_arity_from_type(&p.ty) {
            for suffix in &["args", "body", "clone_bindings"] {
                known.insert(format!("{}.{}", p.name, suffix));
            }
            for n in 0..arity {
                known.insert(format!("{}.args[{}]", p.name, n));
            }
        }
    }

    // Scan template, find all {placeholder} occurrences, replace known ones with markers
    let mut fmt_str = String::new();
    let mut fmt_args: Vec<String> = Vec::new();
    let mut pos = 0;
    let tmpl_bytes = template.as_bytes();

    while pos < tmpl_bytes.len() {
        if tmpl_bytes[pos] == b'{' {
            // Look for the nearest } to extract candidate placeholder
            if let Some(close_offset) = template[pos+1..].find('}') {
                let candidate = &template[pos+1..pos+1+close_offset];
                if known.contains(candidate) {
                    // Known placeholder — replace
                    let marker = format!("\x00{}\x00", fmt_args.len());
                    fmt_str.push_str(&marker);
                    pos = pos + 1 + close_offset + 1;

                    if candidate.contains('.') {
                        let dot = candidate.find('.').unwrap();
                        let param_name = &candidate[..dot];
                        let field = &candidate[dot+1..];
                        let &(idx, arity) = closure_params.get(param_name).unwrap();
                        let cl = format!("__cl_{}", param_name);

                        if let_binding_done.insert(cl.clone()) {
                            let_bindings.push(format!(
                                "                let ({cl}_names, {cl}_body) = inline_lambda({idx}, {arity});"
                            ));
                        }

                        match field {
                            "args" => fmt_args.push(format!("{cl}_names.join(\", \")")),
                            "body" => fmt_args.push(format!("{cl}_body")),
                            "clone_bindings" => {
                                if arity == 1 {
                                    fmt_args.push(format!(
                                        "format!(\"let {{}} = {{}}.clone(); \", {cl}_names[0], {cl}_names[0])"
                                    ));
                                } else {
                                    fmt_args.push(format!(
                                        "{cl}_names.iter().map(|n| format!(\"let {{}} = {{}}.clone(); \", n, n)).collect::<Vec<_>>().join(\"\")"
                                    ));
                                }
                            }
                            f if f.starts_with("args[") => {
                                let n: usize = f[5..f.len()-1].parse().unwrap();
                                fmt_args.push(format!("{cl}_names[{n}]"));
                            }
                            _ => panic!("Unknown closure field: {candidate}"),
                        }
                    } else {
                        // Regular param
                        let idx = regular_params[candidate];
                        fmt_args.push(format!("args_str[{idx}]"));
                    }
                    continue;
                }
            }
            // Not a known placeholder — emit the { literally
            fmt_str.push('{');
            pos += 1;
        } else {
            fmt_str.push(tmpl_bytes[pos] as char);
            pos += 1;
        }
    }

    // Escape remaining literal braces
    fmt_str = fmt_str.replace('{', "{{").replace('}', "}}");

    // Restore markers as {}
    for i in 0..fmt_args.len() {
        fmt_str = fmt_str.replace(&format!("\x00{i}\x00"), "{}");
    }

    let expr = if fmt_args.is_empty() {
        format!("\"{fmt_str}\".to_string()")
    } else {
        format!("format!(\"{fmt_str}\", {})", fmt_args.join(", "))
    };

    (let_bindings, expr)
}

/// Format a match arm, wrapping in a block if there are let bindings
fn format_arm(module: &str, func: &str, let_bindings: &[String], expr: &str) -> String {
    if let_bindings.is_empty() {
        format!(
            "            (\"{module}\", \"{func}\") => {expr},\n",
        )
    } else {
        format!(
            "            (\"{module}\", \"{func}\") => {{\n{bindings}\n                {expr}\n            }},\n",
            bindings = let_bindings.join("\n"),
        )
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
    let mut needs_closures = false;
    let mut module_fn_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

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
            if has_closures(def) {
                needs_closures = true;
            }

            // Generate type signature
            let tp = &def.type_params;
            let params_str: Vec<String> = def
                .params
                .iter()
                .filter(|p| !p.optional) // Required params only for sig
                .map(|p| format!("(s(\"{}\"), {})", p.name, parse_type(&p.ty, tp)))
                .collect();
            // Full params including optional (for sig we include all)
            let all_params_str: Vec<String> = def
                .params
                .iter()
                .map(|p| format!("(s(\"{}\"), {})", p.name, parse_type(&p.ty, tp)))
                .collect();
            let ret_ty = parse_type(&def.ret, tp);

            let generics_str = if def.type_params.is_empty() {
                "vec![]".to_string()
            } else {
                let gs: Vec<String> = def.type_params.iter().map(|t| format!("s(\"{}\")", t)).collect();
                format!("vec![{}]", gs.join(", "))
            };
            sig_arms.push_str(&format!(
                "        (\"{module}\", \"{func}\") => FnSig {{ generics: {generics}, params: vec![{params}], ret: {ret}, is_effect: {effect} }},\n",
                module = module_name,
                func = fn_name,
                generics = generics_str,
                params = all_params_str.join(", "),
                ret = ret_ty,
                effect = def.effect,
            ));

            module_fn_map.entry(module_name.clone()).or_default().push(fn_name.clone());

            // Generate Rust codegen
            let has_opt = has_optional(def);
            let has_effect_variant = def.rust_effect.is_some();

            if has_opt && has_effect_variant {
                // Both optional and effect variants
                let (binds, expr) = render_template_full(&def.rust, &def.params, false);
                let (binds_e, expr_e) = render_template_full(def.rust_effect.as_ref().unwrap(), &def.params, true);
                let (binds_min, expr_min) = render_template_full(def.rust_min.as_ref().unwrap(), &def.params, false);
                let required_count = def.params.iter().filter(|p| !p.optional).count();
                let mut all_binds = binds.clone();
                all_binds.extend(binds_e.iter().cloned());
                all_binds.extend(binds_min.iter().cloned());
                all_binds.dedup();
                rust_arms.push_str(&format!(
                    "            (\"{module}\", \"{func}\") => {{\n{bindings}\n                if args_str.len() > {req} {{\n                    if in_effect && {expr_e}.contains(\"?\") {{ {expr_e} }} else {{ {expr} }}\n                }} else {{\n                    {expr_min}\n                }}\n            }},\n",
                    module = module_name,
                    func = fn_name,
                    bindings = all_binds.join("\n"),
                    req = required_count,
                    expr = expr,
                    expr_e = expr_e,
                    expr_min = expr_min,
                ));
            } else if has_opt {
                // Optional param variants only
                let (binds, expr) = render_template_full(&def.rust, &def.params, false);
                let (binds_min, expr_min) = render_template_full(def.rust_min.as_ref().unwrap(), &def.params, false);
                let required_count = def.params.iter().filter(|p| !p.optional).count();
                let mut all_binds = binds.clone();
                all_binds.extend(binds_min.iter().cloned());
                all_binds.dedup();
                if all_binds.is_empty() {
                    rust_arms.push_str(&format!(
                        "            (\"{module}\", \"{func}\") => if args_str.len() > {req} {{ {expr} }} else {{ {expr_min} }},\n",
                        module = module_name,
                        func = fn_name,
                        req = required_count,
                        expr = expr,
                        expr_min = expr_min,
                    ));
                } else {
                    rust_arms.push_str(&format!(
                        "            (\"{module}\", \"{func}\") => {{\n{bindings}\n                if args_str.len() > {req} {{ {expr} }} else {{ {expr_min} }}\n            }},\n",
                        module = module_name,
                        func = fn_name,
                        bindings = all_binds.join("\n"),
                        req = required_count,
                        expr = expr,
                        expr_min = expr_min,
                    ));
                }
            } else if has_effect_variant {
                // Effect variant: check in_effect && body contains "?"
                let (binds, expr) = render_template_full(&def.rust, &def.params, false);
                let (binds_e, expr_e) = render_template_full(def.rust_effect.as_ref().unwrap(), &def.params, true);
                let mut all_binds = binds.clone();
                all_binds.extend(binds_e.iter().cloned());
                all_binds.dedup();
                // We need to check the body for "?" — use the closure body variable
                let closure_param = def.params.iter().find(|p| closure_arity_from_type(&p.ty).is_some());
                let body_check = if let Some(cp) = closure_param {
                    format!("__cl_{}_body.contains(\"?\")", cp.name)
                } else {
                    "false".to_string()
                };
                rust_arms.push_str(&format!(
                    "            (\"{module}\", \"{func}\") => {{\n{bindings}\n                if in_effect && {check} {{ {expr_e} }} else {{ {expr} }}\n            }},\n",
                    module = module_name,
                    func = fn_name,
                    bindings = all_binds.join("\n"),
                    check = body_check,
                    expr = expr,
                    expr_e = expr_e,
                ));
            } else if !def.effect && def.rust.ends_with('?') {
                // Pure-but-fallible: template has ? but function is not effect.
                // Generate conditional: in_effect uses ?, otherwise omit ? (returns raw Result).
                let (binds, expr_with_q) = render_template_full(&def.rust, &def.params, false);
                let rust_no_q = def.rust.trim_end_matches('?');
                let (_, expr_no_q) = render_template_full(rust_no_q, &def.params, false);
                let mut all_binds = binds.clone();
                all_binds.dedup();
                if all_binds.is_empty() {
                    rust_arms.push_str(&format!(
                        "            (\"{module}\", \"{func}\") => if in_effect {{ {with_q} }} else {{ {no_q} }},\n",
                        module = module_name,
                        func = fn_name,
                        with_q = expr_with_q,
                        no_q = expr_no_q,
                    ));
                } else {
                    rust_arms.push_str(&format!(
                        "            (\"{module}\", \"{func}\") => {{\n{bindings}\n                if in_effect {{ {with_q} }} else {{ {no_q} }}\n            }},\n",
                        module = module_name,
                        func = fn_name,
                        bindings = all_binds.join("\n"),
                        with_q = expr_with_q,
                        no_q = expr_no_q,
                    ));
                }
            } else {
                // Simple case
                let (binds, expr) = render_template_full(&def.rust, &def.params, false);
                rust_arms.push_str(&format_arm(&module_name, fn_name, &binds, &expr));
            }

            // Generate alias arms
            for alias in &def.aliases {
                sig_arms.push_str(&format!(
                    "        (\"{module}\", \"{alias}\") => FnSig {{ generics: {generics}, params: vec![{params}], ret: {ret}, is_effect: {effect} }},\n",
                    module = module_name,
                    generics = generics_str,
                    params = all_params_str.join(", "),
                    ret = ret_ty,
                    effect = def.effect,
                ));
                // For aliases, just duplicate the Rust arm (copy the last generated arm with alias)
                let last_arm = rust_arms.lines().rev()
                    .take_while(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                // Simple: re-render
                if has_effect_variant || has_opt {
                    // For complex arms, just replace the function name
                    let arm_text = rust_arms.rfind(&format!("(\"{}\", \"{}\")", module_name, fn_name));
                    if let Some(_) = arm_text {
                        // Re-generate with alias name (simplest: extract last arm block)
                        // For now, regenerate from template
                        if has_opt {
                            let (binds, expr) = render_template_full(&def.rust, &def.params, false);
                            let (_, expr_min) = render_template_full(def.rust_min.as_ref().unwrap(), &def.params, false);
                            let required_count = def.params.iter().filter(|p| !p.optional).count();
                            if binds.is_empty() {
                                rust_arms.push_str(&format!(
                                    "            (\"{module}\", \"{alias}\") => if args_str.len() > {req} {{ {expr} }} else {{ {expr_min} }},\n",
                                    module = module_name,
                                    req = required_count,
                                ));
                            }
                        } else {
                            let (binds, expr) = render_template_full(&def.rust, &def.params, false);
                            rust_arms.push_str(&format_arm(&module_name, alias, &binds, &expr));
                        }
                    }
                } else {
                    let (binds, expr) = render_template_full(&def.rust, &def.params, false);
                    rust_arms.push_str(&format_arm(&module_name, alias, &binds, &expr));
                }
            }

            // Generate TS codegen (skip if no ts template, or if function uses closures — TS emitter handles closures separately)
            if let Some(ts_template) = &def.ts {
                if has_closures(def) {
                    // TS closure-taking functions are handled by emit_ts/expressions.rs
                } else {
                let (_, ts_expr) = render_template_full(ts_template, &def.params, false);
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
                // Handle ts_min for optional
                if has_opt {
                    if let Some(ts_min) = &def.ts_min {
                        // We'd need a separate dispatch for TS too, but for now TS doesn't do optional args differently
                        let _ = ts_min;
                    }
                }
                } // else (non-closure)
            }
        }
    }

    // Write generated files
    // Generate module_functions match arms
    let mut mod_fn_arms = String::new();
    for (module, fns) in &module_fn_map {
        let names: Vec<String> = fns.iter().map(|f| format!("\"{}\"", f)).collect();
        mod_fn_arms.push_str(&format!(
            "        \"{}\" => vec![{}],\n",
            module,
            names.join(", ")
        ));
    }

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
         }}\n\n\
         pub fn generated_module_functions(module: &str) -> Vec<&'static str> {{\n\
         \x20   match module {{\n\
         {}\
         \x20       _ => vec![],\n\
         \x20   }}\n\
         }}\n",
        sig_arms, mod_fn_arms
    );

    // Rust codegen function — with inline_lambda callback for closure support
    let rust_fn_sig = if needs_closures {
        "pub fn gen_generated_call(\n    \
         \x20   module: &str,\n    \
         \x20   func: &str,\n    \
         \x20   args_str: &[String],\n    \
         \x20   in_effect: bool,\n    \
         \x20   inline_lambda: &dyn Fn(usize, usize) -> (Vec<String>, String),\n\
         ) -> Option<String>"
    } else {
        "pub fn gen_generated_call(\n    \
         \x20   module: &str,\n    \
         \x20   func: &str,\n    \
         \x20   args_str: &[String],\n    \
         \x20   _in_effect: bool,\n    \
         \x20   _inline_lambda: &dyn Fn(usize, usize) -> (Vec<String>, String),\n\
         ) -> Option<String>"
    };

    let rust_file = format!(
        "// AUTO-GENERATED by build.rs from stdlib/defs/*.toml — DO NOT EDIT\n\n\
         {sig} {{\n\
         \x20   let expr = match (module, func) {{\n\
         {arms}\
         \x20       _ => return None,\n\
         \x20   }};\n\
         \x20   Some(expr)\n\
         }}\n",
        sig = rust_fn_sig,
        arms = rust_arms
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
