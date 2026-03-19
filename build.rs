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
    #[allow(dead_code)]
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
    /// Human-readable description (English)
    #[serde(default)]
    #[allow(dead_code)]
    description: Option<String>,
    /// Usage example in Almide syntax
    #[serde(default)]
    #[allow(dead_code)]
    example: Option<String>,
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
/// Handles nested brackets: Fn[List[Int], Map[String, Int]] -> Bool → Some(2)
fn closure_arity_from_type(ty: &str) -> Option<usize> {
    if ty.starts_with("Fn[") {
        // Find the matching ] for Fn[ respecting nested brackets
        let mut depth = 0;
        let mut bracket_end = None;
        for (i, ch) in ty[3..].char_indices() {
            match ch {
                '[' | '{' => depth += 1,
                ']' if depth > 0 => depth -= 1,
                ']' if depth == 0 => {
                    bracket_end = Some(i + 3);
                    break;
                }
                _ => {}
            }
        }
        if let Some(end) = bracket_end {
            if ty[end..].starts_with("] -> ") {
                let params_str = &ty[3..end];
                if params_str.is_empty() {
                    return Some(0);
                }
                // Count top-level commas (respecting nested brackets)
                let mut count = 1;
                let mut d = 0;
                for ch in params_str.chars() {
                    match ch {
                        '[' | '{' => d += 1,
                        ']' | '}' => d -= 1,
                        ',' if d == 0 => count += 1,
                        _ => {}
                    }
                }
                return Some(count);
            }
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
        other if other.starts_with("Set[") => {
            let inner = &other[4..other.len() - 1];
            format!("Ty::Named(s(\"Set\"), vec![{}])", parse_type(inner, type_params))
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
            // Not a known placeholder — check if it looks like an intended placeholder
            if let Some(close_offset) = template[pos+1..].find('}') {
                let candidate = &template[pos+1..pos+1+close_offset];
                // If candidate looks like an identifier (no spaces, no operators), warn
                if !candidate.is_empty()
                    && !candidate.contains(' ')
                    && !candidate.contains('+')
                    && !candidate.contains('-')
                    && candidate.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '[' || c == ']')
                {
                    panic!(
                        "build.rs: unknown placeholder {{{}}} in template. Known: {:?}",
                        candidate,
                        known.iter().collect::<Vec<_>>()
                    );
                }
            }
            // Literal brace
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
    let mut arg_transform_arms = String::new();

    // Scan runtime .rs files for actual function signatures
    // This determines whether a String param in TOML is passed as String (owned) or &str (borrowed)
    let mut runtime_param_types: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let runtime_dir = Path::new("runtime/rs/src");
    if runtime_dir.exists() {
        for entry in fs::read_dir(runtime_dir).unwrap().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "rs") { continue; }
            let content = fs::read_to_string(&path).unwrap_or_default();
            for line in content.lines() {
                // Match: pub fn almide_rt_xxx(param1: Type1, param2: Type2) -> RetType {
                if let Some(start) = line.find("pub fn almide_rt_") {
                    let rest = &line[start..];
                    if let (Some(paren_open), Some(paren_close)) = (rest.find('('), rest.find(')')) {
                        let fn_name_end = paren_open;
                        let fn_name = &rest[7..fn_name_end]; // skip "pub fn "
                        let params_str = &rest[paren_open+1..paren_close];
                        let param_types: Vec<String> = params_str.split(',')
                            .map(|p| {
                                let p = p.trim();
                                if let Some(colon_pos) = p.find(':') {
                                    p[colon_pos+1..].trim().to_string()
                                } else {
                                    "unknown".to_string()
                                }
                            })
                            .collect();
                        runtime_param_types.insert(fn_name.to_string(), param_types);
                    }
                }
            }
        }
    }

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
            let _params_str: Vec<String> = def
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
                "        (\"{module}\", \"{func}\") => FnSig {{ generics: {generics}, params: vec![{params}], ret: {ret}, is_effect: {effect}, structural_bounds: std::collections::HashMap::new() }},\n",
                module = module_name,
                func = fn_name,
                generics = generics_str,
                params = all_params_str.join(", "),
                ret = ret_ty,
                effect = def.effect,
            ));

            module_fn_map.entry(module_name.clone()).or_default().push(fn_name.clone());

            // Generate arg transform table entry
            {
                let rust_tmpl = &def.rust;
                let mut transforms = Vec::new();
                for (i, param) in def.params.iter().enumerate() {
                    let pname = &param.name;
                    let _ptype = &param.ty;
                    // Check actual runtime function signature for this param
                    let rt_fn_name = format!("almide_rt_{}_{}", module_name, fn_name);
                    let runtime_ty = runtime_param_types.get(&rt_fn_name)
                        .and_then(|types| types.get(i))
                        .map(|s| s.as_str())
                        .unwrap_or("");

                    let transform = if rust_tmpl.contains(&format!("{{{}.args}}", pname)) || rust_tmpl.contains(&format!("{{{}.body}}", pname)) {
                        // Check if lambda body is wrapped in Ok({ ... })
                        let body_placeholder = format!("{{{}.body}}", pname);
                        if rust_tmpl.contains("Ok(") && rust_tmpl.contains(&body_placeholder) {
                            "ArgTransform::LambdaResultWrap"
                        } else {
                            "ArgTransform::LambdaClone"
                        }
                    } else if rust_tmpl.contains(&format!("({{{}}}).to_vec()", pname)) {
                        "ArgTransform::ToVec"
                    } else if rust_tmpl.contains(&format!("Some({{{}}}", pname)) {
                        "ArgTransform::WrapSome"
                    } else if rust_tmpl.contains(&format!("&*{{{}}}", pname)) {
                        // BorrowStr: check runtime signature — if it takes String (owned), use Direct
                        if runtime_ty == "String" {
                            "ArgTransform::Direct"
                        } else {
                            "ArgTransform::BorrowStr"
                        }
                    } else if rust_tmpl.contains(&format!("&{{{}}}", pname)) {
                        "ArgTransform::BorrowRef"
                    } else {
                        "ArgTransform::Direct"
                    };
                    transforms.push(format!("{}", transform));
                }
                // Extract actual runtime function name from template
                let rt_name = {
                    let tmpl = &def.rust;
                    // Find function/method call: identifier (possibly Type::method) before (
                    if let Some(paren) = tmpl.find('(') {
                        let prefix = &tmpl[..paren];
                        // Handle "Type::method(" or "almide_rt_xxx(" — include :: in identifier chars
                        let name = prefix.rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
                            .next().unwrap_or("");
                        if !name.is_empty() { name.to_string() } else { format!("almide_rt_{}_{}", module_name, fn_name) }
                    } else {
                        format!("almide_rt_{}_{}", module_name, fn_name)
                    }
                };

                let effect_suffix = if def.effect { "true" } else { "false" };
                let required_count = def.params.iter().filter(|p| !p.optional).count();
                arg_transform_arms.push_str(&format!(
                    "            (\"{module}\", \"{func}\") => Some(StdlibCallInfo {{ args: &[{transforms}], effect: {effect}, name: \"{rt_name}\", required: {required} }}),\n",
                    module = module_name,
                    func = fn_name,
                    transforms = transforms.join(", "),
                    effect = effect_suffix,
                    required = required_count,
                ));
            }

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
                    "            (\"{module}\", \"{func}\") => {{\n{bindings}\n                if args_str.len() > {req} {{\n                    if auto_unwrap && {expr_e}.contains(\"?\") {{ {expr_e} }} else {{ {expr} }}\n                }} else {{\n                    {expr_min}\n                }}\n            }},\n",
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
                // Effect variant: check auto_unwrap && body contains "?"
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
                    "            (\"{module}\", \"{func}\") => {{\n{bindings}\n                if auto_unwrap && {check} {{ {expr_e} }} else {{ {expr} }}\n            }},\n",
                    module = module_name,
                    func = fn_name,
                    bindings = all_binds.join("\n"),
                    check = body_check,
                    expr = expr,
                    expr_e = expr_e,
                ));
            } else if def.rust.ends_with('?') {
                // Fallible function: template has ?. In effect context add ?, otherwise return raw Result.
                let (binds, expr_with_q) = render_template_full(&def.rust, &def.params, false);
                let rust_no_q = def.rust.trim_end_matches('?');
                let (_, expr_no_q) = render_template_full(rust_no_q, &def.params, false);
                let mut all_binds = binds.clone();
                all_binds.dedup();
                if all_binds.is_empty() {
                    rust_arms.push_str(&format!(
                        "            (\"{module}\", \"{func}\") => if auto_unwrap {{ {with_q} }} else {{ {no_q} }},\n",
                        module = module_name,
                        func = fn_name,
                        with_q = expr_with_q,
                        no_q = expr_no_q,
                    ));
                } else {
                    rust_arms.push_str(&format!(
                        "            (\"{module}\", \"{func}\") => {{\n{bindings}\n                if auto_unwrap {{ {with_q} }} else {{ {no_q} }}\n            }},\n",
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
                    "        (\"{module}\", \"{alias}\") => FnSig {{ generics: {generics}, params: vec![{params}], ret: {ret}, is_effect: {effect}, structural_bounds: std::collections::HashMap::new() }},\n",
                    module = module_name,
                    generics = generics_str,
                    params = all_params_str.join(", "),
                    ret = ret_ty,
                    effect = def.effect,
                ));
                // For aliases, just duplicate the Rust arm (copy the last generated arm with alias)
                let _last_arm = rust_arms.lines().rev()
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
         \x20   auto_unwrap: bool,\n    \
         \x20   inline_lambda: &dyn Fn(usize, usize) -> (Vec<String>, String),\n\
         ) -> Option<String>"
    } else {
        "pub fn gen_generated_call(\n    \
         \x20   module: &str,\n    \
         \x20   func: &str,\n    \
         \x20   args_str: &[String],\n    \
         \x20   _auto_unwrap: bool,\n    \
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

    // Arg transform table for codegen v3
    let arg_transforms_file = format!(
        "// AUTO-GENERATED by build.rs from stdlib/defs/*.toml — DO NOT EDIT\n\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub enum ArgTransform {{\n\
         \x20   Direct,     // pass as-is\n\
         \x20   BorrowStr,  // &*expr (borrow as &str)\n\
         \x20   BorrowRef,  // &expr (borrow as reference)\n\
         \x20   ToVec,      // (expr).to_vec() (owned copy)\n\
         \x20   LambdaClone, // lambda with clone bindings\n\
         \x20   WrapSome,   // Some(expr) (wrap in Option)\n\
         \x20   LambdaResultWrap, // lambda with Ok(body) wrapping\n\
         }}\n\n\
         pub struct StdlibCallInfo {{\n\
         \x20   pub args: &'static [ArgTransform],\n\
         \x20   pub effect: bool,\n\
         \x20   pub name: &'static str,\n\
         \x20   pub required: usize,\n\
         }}\n\n\
         pub fn lookup(module: &str, func: &str) -> Option<StdlibCallInfo> {{\n\
         \x20   match (module, func) {{\n\
         {arms}\
         \x20       _ => None,\n\
         \x20   }}\n\
         }}\n",
        arms = arg_transform_arms
    );

    fs::write(out_dir.join("stdlib_sigs.rs"), sig_file).unwrap();
    fs::write(out_dir.join("emit_rust_calls.rs"), rust_file).unwrap();
    fs::write(out_dir.join("emit_ts_calls.rs"), ts_file).unwrap();
    fs::write(out_dir.join("arg_transforms.rs"), arg_transforms_file).unwrap();

    // Tell cargo to rerun if defs change
    println!("cargo:rerun-if-changed=stdlib/defs");
    for entry in fs::read_dir(defs_dir).unwrap().filter_map(|e| e.ok()) {
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }

    // ── Runtime codegen: scan runtime/{ts,js,rust} → generated files ──
    generate_runtime_registry(&out_dir);

    // ── Grammar codegen: tokens.toml → token_table.rs ──────────────────
    generate_token_table(&out_dir);
}

/// Scan runtime/{ts,js,rust} directories and generate registry files.
fn generate_runtime_registry(out_dir: &Path) {
    // ── TS/JS runtime ──
    let ts_dir = Path::new("runtime/ts");
    let js_dir = Path::new("runtime/js");
    if !ts_dir.exists() || !js_dir.exists() {
        return;
    }

    // Collect .ts files (excluding helpers which is special)
    let mut modules: Vec<String> = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(ts_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "ts"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let stem = entry.path().file_stem().unwrap().to_str().unwrap().to_string();
        if stem != "helpers" {
            modules.push(stem);
        }
    }

    // Generate ts_runtime.rs
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by build.rs from runtime/{ts,js}/ — DO NOT EDIT\n\n");

    // include_str! for each module
    for m in &modules {
        let upper = m.to_uppercase();
        out.push_str(&format!(
            "const MOD_{upper}_TS: &str = include_str!(\"../../runtime/ts/{m}.ts\");\n"
        ));
        out.push_str(&format!(
            "const MOD_{upper}_JS: &str = include_str!(\"../../runtime/js/{m}.js\");\n"
        ));
    }
    out.push_str("const HELPERS_TS: &str = include_str!(\"../../runtime/ts/helpers.ts\");\n");
    out.push_str("const HELPERS_JS: &str = include_str!(\"../../runtime/js/helpers.js\");\n");
    out.push('\n');

    // Preambles
    out.push_str("const PREAMBLE_TS: &str = \"// ---- Almide Runtime ----\\n\";\n");
    out.push_str("const PREAMBLE_JS: &str = \"\\\n// ---- Almide Runtime (JS) ----\\n\\\nconst __node_process = globalThis.process || {};\\n\";\n");
    out.push_str("const EPILOGUE: &str = \"// ---- End Runtime ----\\n\";\n\n");

    // RuntimeModule struct + ALL_MODULES
    out.push_str("pub struct RuntimeModule {\n    pub name: &'static str,\n    pub ts_source: &'static str,\n    pub js_source: &'static str,\n}\n\n");
    out.push_str("pub static ALL_MODULES: &[RuntimeModule] = &[\n");
    for m in &modules {
        let upper = m.to_uppercase();
        out.push_str(&format!(
            "    RuntimeModule {{ name: \"{m}\", ts_source: MOD_{upper}_TS, js_source: MOD_{upper}_JS }},\n"
        ));
    }
    out.push_str("];\n\n");

    // Functions
    out.push_str(r#"pub fn full_runtime(js_mode: bool) -> String {
    let mut out = String::with_capacity(if js_mode { 16384 } else { 20480 });
    out.push_str(if js_mode { PREAMBLE_JS } else { PREAMBLE_TS });
    for m in ALL_MODULES {
        out.push_str(if js_mode { m.js_source } else { m.ts_source });
    }
    out.push_str(if js_mode { HELPERS_JS } else { HELPERS_TS });
    out.push_str(EPILOGUE);
    out
}

pub fn get_module_source(name: &str, js_mode: bool) -> Option<&'static str> {
    ALL_MODULES.iter().find(|m| m.name == name).map(|m| {
        if js_mode { m.js_source } else { m.ts_source }
    })
}

pub fn get_helpers_source(js_mode: bool) -> &'static str {
    if js_mode { HELPERS_JS } else { HELPERS_TS }
}

pub fn get_preamble(js_mode: bool) -> &'static str {
    if js_mode { PREAMBLE_JS } else { PREAMBLE_TS }
}
"#);

    fs::write(out_dir.join("ts_runtime.rs"), &out).unwrap();

    // ── Rust runtime ──
    let rust_dir = Path::new("runtime/rs/src");
    if rust_dir.exists() {
        let mut rust_entries: Vec<_> = fs::read_dir(rust_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
            .filter(|e| e.file_name() != "lib.rs")
            .collect();
        rust_entries.sort_by_key(|e| e.file_name());

        let mut rust_out = String::new();
        rust_out.push_str("// AUTO-GENERATED by build.rs from runtime/rs/src/ — DO NOT EDIT\n\n");
        rust_out.push_str("pub const RUST_RUNTIME_MODULES: &[(&str, &str)] = &[\n");
        for entry in &rust_entries {
            let stem = entry.path().file_stem().unwrap().to_str().unwrap().to_string();
            let rel = format!("../../runtime/rs/src/{}.rs", stem);
            rust_out.push_str(&format!(
                "    (\"{stem}\", include_str!(\"{rel}\")),\n"
            ));
        }
        rust_out.push_str("];\n");

        fs::write(out_dir.join("rust_runtime.rs"), &rust_out).unwrap();
    }

    // Rerun if runtime files change
    println!("cargo:rerun-if-changed=runtime/ts");
    println!("cargo:rerun-if-changed=runtime/js");
    println!("cargo:rerun-if-changed=runtime/rs/src");
}

/// Token category in tokens.toml
#[derive(Deserialize)]
struct TokensDef {
    keywords: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    keyword_aliases: BTreeMap<String, String>,
    operators: BTreeMap<String, Vec<String>>,
    #[allow(dead_code)]
    delimiters: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    #[allow(dead_code)]
    special: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct PrecedenceLevel {
    name: String,
    precedence: u32,
    operators: Vec<String>,
    associativity: String,
}

#[derive(Deserialize)]
struct PrecedenceDef {
    level: Vec<PrecedenceLevel>,
}

fn generate_token_table(out_dir: &Path) {
    let tokens_path = Path::new("grammar/tokens.toml");
    let prec_path = Path::new("grammar/precedence.toml");
    if !tokens_path.exists() {
        return;
    }

    let tokens_content = fs::read_to_string(tokens_path).unwrap();
    let tokens: TokensDef = toml::from_str(&tokens_content).unwrap();

    // Collect all keywords with their categories
    let mut keyword_entries: Vec<(String, String)> = Vec::new(); // (keyword, category)
    for (category, words) in &tokens.keywords {
        for word in words {
            keyword_entries.push((word.clone(), category.clone()));
        }
    }
    keyword_entries.sort();

    // Build keyword → TokenType name mapping
    fn keyword_to_token_type(kw: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = true;
        for ch in kw.chars() {
            if ch == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }
        result
    }

    // Generate keyword map entries
    let mut keyword_map_lines = String::new();
    for (kw, _cat) in &keyword_entries {
        let tt = keyword_to_token_type(kw);
        keyword_map_lines.push_str(&format!(
            "        m.insert(\"{kw}\", TokenType::{tt});\n"
        ));
    }
    // Add aliases
    for (alias, target) in &tokens.keyword_aliases {
        let tt = keyword_to_token_type(target);
        keyword_map_lines.push_str(&format!(
            "        m.insert(\"{alias}\", TokenType::{tt});\n"
        ));
    }

    // Generate keyword list for tree-sitter (grouped by category)
    let mut ts_keyword_lines = String::new();
    for (category, words) in &tokens.keywords {
        let words_str: Vec<String> = words.iter().map(|w| format!("\"{}\"", w)).collect();
        ts_keyword_lines.push_str(&format!(
            "    // {category}\n    {words},\n",
            words = words_str.join(", "),
        ));
    }
    // Add aliases
    let alias_strs: Vec<String> = tokens.keyword_aliases.keys().map(|a| format!("\"{}\"", a)).collect();
    if !alias_strs.is_empty() {
        ts_keyword_lines.push_str(&format!(
            "    // aliases\n    {},\n", alias_strs.join(", ")
        ));
    }

    // Generate TextMate keyword scopes
    let mut tm_keywords = String::new();
    for (category, words) in &tokens.keywords {
        let scope = match category.as_str() {
            "control" => "keyword.control.almide",
            "declaration" => "keyword.declaration.almide",
            "modifier" => "storage.modifier.almide",
            "value" => "constant.language.almide",
            "flow" => "keyword.control.flow.almide",
            _ => "keyword.other.almide",
        };
        let words_str = words.join("|");
        tm_keywords.push_str(&format!(
            "    // scope: {scope}\n    // pattern: \\\\b({words_str})\\\\b\n"
        ));
    }

    // All keywords as a flat list
    let all_keywords: Vec<&str> = keyword_entries.iter().map(|(k, _)| k.as_str()).collect();
    let all_kw_str: Vec<String> = all_keywords.iter().map(|k| format!("\"{}\"", k)).collect();

    // Collect all operators
    let mut all_operators: Vec<(String, String)> = Vec::new();
    for (category, ops) in &tokens.operators {
        for op in ops {
            all_operators.push((op.clone(), category.clone()));
        }
    }

    // Generate precedence table
    let mut prec_lines = String::new();
    if prec_path.exists() {
        let prec_content = fs::read_to_string(&prec_path).unwrap();
        let prec: PrecedenceDef = toml::from_str(&prec_content).unwrap();
        for level in &prec.level {
            let ops_str: Vec<String> = level.operators.iter().map(|o| format!("\"{}\"", o)).collect();
            prec_lines.push_str(&format!(
                "    // precedence {}: {} ({}) — {}\n",
                level.precedence, level.name, level.associativity,
                ops_str.join(", "),
            ));
        }
        println!("cargo:rerun-if-changed={}", prec_path.display());
    }

    // Write the generated token table
    let token_table = format!(
        r#"// AUTO-GENERATED by build.rs from almide-grammar — DO NOT EDIT
//
// This file provides:
//   - build_keyword_map_generated() for the lexer
//   - ALL_KEYWORDS list
//   - Keyword categories and precedence table as comments for reference

use std::collections::HashMap;
use crate::lexer::TokenType;

/// Build the keyword → TokenType map from grammar/tokens.toml
pub fn build_keyword_map_generated() -> HashMap<&'static str, TokenType> {{
    let mut m = HashMap::new();
{keyword_map_lines}    m
}}

/// All keywords as a flat list (for validation, tree-sitter, TextMate generation)
pub const ALL_KEYWORDS: &[&str] = &[{all_kw}];

/*
── Tree-sitter keyword list ──────────────────────────────────────────
{ts_keyword_lines}
── TextMate grammar scopes ───────────────────────────────────────────
{tm_keywords}
── Operator precedence table ─────────────────────────────────────────
{prec_lines}*/
"#,
        keyword_map_lines = keyword_map_lines,
        all_kw = all_kw_str.join(", "),
        ts_keyword_lines = ts_keyword_lines,
        tm_keywords = tm_keywords,
        prec_lines = prec_lines,
    );

    fs::write(out_dir.join("token_table.rs"), token_table).unwrap();

    // ── Generate tree-sitter keywords file ─────────────────────────────
    let mut ts_rules = String::new();
    ts_rules.push_str("// AUTO-GENERATED by build.rs from grammar/tokens.toml — DO NOT EDIT\n");
    ts_rules.push_str("// Copy these keyword rules into tree-sitter-almide/grammar.js\n\n");

    for (category, words) in &tokens.keywords {
        ts_rules.push_str(&format!("    // {category} keywords\n"));
        for word in words {
            ts_rules.push_str(&format!(
                "    {word}_keyword: $ => '{word}',\n"
            ));
        }
        ts_rules.push('\n');
    }
    // Alias keywords
    for (alias, target) in &tokens.keyword_aliases {
        ts_rules.push_str(&format!(
            "    // alias: {alias} → {target}\n"
        ));
    }

    ts_rules.push_str("\n    // keyword() list for tree-sitter word rule:\n");
    ts_rules.push_str("    // keyword: $ => choice(\n");
    for (kw, _) in &keyword_entries {
        ts_rules.push_str(&format!("    //   $.{kw}_keyword,\n"));
    }
    ts_rules.push_str("    // ),\n");

    fs::write(out_dir.join("tree_sitter_keywords.txt"), ts_rules).unwrap();

    // ── Generate TextMate grammar patterns ─────────────────────────────
    let mut tm_grammar = String::new();
    tm_grammar.push_str("// AUTO-GENERATED by build.rs from grammar/tokens.toml — DO NOT EDIT\n");
    tm_grammar.push_str("// Use these patterns in vscode-almide/syntaxes/almide.tmLanguage.json\n\n");

    for (category, words) in &tokens.keywords {
        let scope = match category.as_str() {
            "control" => "keyword.control.almide",
            "declaration" => "keyword.declaration.almide",
            "modifier" => "storage.modifier.almide",
            "value" => "constant.language.almide",
            "flow" => "keyword.control.flow.almide",
            _ => "keyword.other.almide",
        };
        let pattern = words.join("|");
        tm_grammar.push_str(&format!(
            r#"{{
  "name": "{scope}",
  "match": "\\b({pattern})\\b"
}},
"#
        ));
    }
    // Aliases (Ok, Err, Some, None → constant.language)
    if !tokens.keyword_aliases.is_empty() {
        let alias_pattern: Vec<&String> = tokens.keyword_aliases.keys().collect();
        let pattern = alias_pattern.iter().map(|a| a.as_str()).collect::<Vec<_>>().join("|");
        tm_grammar.push_str(&format!(
            r#"{{
  "name": "constant.language.almide",
  "match": "\\b({pattern})\\b"
}},
"#
        ));
    }

    // Operators
    let mut op_patterns: Vec<String> = Vec::new();
    for (_cat, ops) in &tokens.operators {
        for op in ops {
            // Escape regex special chars
            let escaped: String = op.chars().map(|c| {
                if "+-*/%^|.=!<>()[]{}?\\".contains(c) {
                    format!("\\{}", c)
                } else {
                    c.to_string()
                }
            }).collect();
            op_patterns.push(escaped);
        }
    }
    // Sort by length descending so longer operators match first
    op_patterns.sort_by(|a, b| b.len().cmp(&a.len()));
    let op_pattern = op_patterns.join("|");
    tm_grammar.push_str(&format!(
        r#"{{
  "name": "keyword.operator.almide",
  "match": "{op_pattern}"
}},
"#
    ));

    fs::write(out_dir.join("textmate_patterns.txt"), tm_grammar).unwrap();

    // ── Generate precedence reference ──────────────────────────────────
    if prec_path.exists() {
        let prec_content = fs::read_to_string(&prec_path).unwrap();
        let prec: PrecedenceDef = toml::from_str(&prec_content).unwrap();

        let mut prec_ref = String::new();
        prec_ref.push_str("// AUTO-GENERATED by build.rs from grammar/precedence.toml — DO NOT EDIT\n");
        prec_ref.push_str("// Tree-sitter precedence rules:\n\n");

        for level in &prec.level {
            let prec_fn = match level.associativity.as_str() {
                "left" => "prec.left",
                "right" => "prec.right",
                "none" => "prec",
                _ => "prec",
            };
            prec_ref.push_str(&format!(
                "// {name}: {fn}({prec}, ...)\n// operators: {ops}\n\n",
                name = level.name,
                fn = prec_fn,
                prec = level.precedence,
                ops = level.operators.join(", "),
            ));
        }

        fs::write(out_dir.join("tree_sitter_precedence.txt"), prec_ref).unwrap();
    }

    println!("cargo:rerun-if-changed={}", tokens_path.display());
}
