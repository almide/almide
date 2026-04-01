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
    impure: bool,
    #[serde(default)]
    #[allow(dead_code)]
    ufcs: bool,
    #[serde(default)]
    type_params: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    rust: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    rust_effect: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    rust_min: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    ts: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    ts_min: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    description: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    example: Option<String>,
}

#[derive(Deserialize)]
struct Param {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    #[allow(dead_code)]
    optional: bool,
}

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
    if type_params.iter().any(|tp| tp == s) {
        return format!("Ty::TypeVar(s(\"{}\"))", s);
    }
    match s {
        "Int" => "Ty::Int".to_string(),
        "Float" => "Ty::Float".to_string(),
        "String" => "Ty::String".to_string(),
        "Bool" => "Ty::Bool".to_string(),
        "Unit" => "Ty::Unit".to_string(),
        "Bytes" => "Ty::Bytes".to_string(),
        "Matrix" => "Ty::Matrix".to_string(),
        "Never" => "Ty::Never".to_string(),
        "Unknown" => "Ty::Unknown".to_string(),
        other if other.starts_with("List[") => {
            let inner = &other[5..other.len() - 1];
            format!("Ty::list({})", parse_type(inner, type_params))
        }
        other if other.starts_with("Option[") => {
            let inner = &other[7..other.len() - 1];
            format!("Ty::option({})", parse_type(inner, type_params))
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
                "Ty::result({}, {})",
                parse_type(ok_ty, type_params),
                parse_type(err_ty, type_params)
            )
        }
        other if other.starts_with("Set[") => {
            let inner = &other[4..other.len() - 1];
            format!("Ty::set_of({})", parse_type(inner, type_params))
        }
        other if other.starts_with("Map[") => {
            let inner = &other[4..other.len() - 1];
            let split_pos = split_top_level_comma(inner).expect("Map type needs two type params");
            let key_ty = &inner[..split_pos];
            let val_ty = inner[split_pos + 2..].trim();
            format!("Ty::map_of({}, {})", parse_type(key_ty, type_params), parse_type(val_ty, type_params))
        }
        other if other.starts_with("Fn[") && other.contains("] -> ") => {
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
        other if other.starts_with("fn(") && other.contains(") -> ") => {
            let paren_close = other.find(") -> ").unwrap();
            let params_str = &other[3..paren_close];
            let ret_str = &other[paren_close + 5..];
            let param_types: Vec<String> = if params_str.is_empty() {
                vec![]
            } else {
                params_str
                    .split(", ")
                    .map(|t| parse_type(t.trim(), type_params))
                    .collect()
            };
            format!(
                "Ty::Fn {{ params: vec![{}], ret: Box::new({}) }}",
                param_types.join(", "),
                parse_type(ret_str, type_params)
            )
        }
        other if other.starts_with('(') && other.ends_with(')') => {
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

pub fn generate_stdlib() {
    let defs_dir = Path::new("stdlib/defs");
    if !defs_dir.exists() {
        return;
    }

    let out_dir = Path::new("src/generated");
    fs::create_dir_all(out_dir).unwrap();

    let mut sig_arms = String::new();
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
            let tp = &def.type_params;
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
                "        (\"{module}\", \"{func}\") => FnSig {{ generics: {generics}, params: vec![{params}], ret: {ret}, is_effect: {effect}, structural_bounds: std::collections::HashMap::new(), protocol_bounds: std::collections::HashMap::new() }},\n",
                module = module_name,
                func = fn_name,
                generics = generics_str,
                params = all_params_str.join(", "),
                ret = ret_ty,
                effect = def.effect,
            ));

            module_fn_map.entry(module_name.clone()).or_default().push(fn_name.clone());

            // Alias arms
            for alias in &def.aliases {
                sig_arms.push_str(&format!(
                    "        (\"{module}\", \"{alias}\") => FnSig {{ generics: {generics}, params: vec![{params}], ret: {ret}, is_effect: {effect}, structural_bounds: std::collections::HashMap::new(), protocol_bounds: std::collections::HashMap::new() }},\n",
                    module = module_name,
                    generics = generics_str,
                    params = all_params_str.join(", "),
                    ret = ret_ty,
                    effect = def.effect,
                ));
            }
        }
    }

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
         use crate::types::{{Ty, FnSig}};\n\
         use crate::intern::{{Sym, sym}};\n\n\
         pub fn lookup_generated_sig(module: &str, func: &str) -> Option<FnSig> {{\n\
         \x20   let s = |n: &str| -> Sym {{ sym(n) }};\n\
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

    fs::write(out_dir.join("stdlib_sigs.rs"), sig_file).unwrap();

    println!("cargo:rerun-if-changed=stdlib/defs");
    for entry in fs::read_dir(defs_dir).unwrap().filter_map(|e| e.ok()) {
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }
}
