use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct FnDef {
    params: Vec<Param>,
    #[serde(rename = "return")]
    _ret: String,
    #[serde(default)]
    effect: bool,
    #[serde(default)]
    impure: bool,
    #[serde(default)]
    #[allow(dead_code)]
    ufcs: bool,
    #[serde(default)]
    #[allow(dead_code)]
    type_params: Vec<String>,
    rust: String,
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    ty: String,
    #[serde(default)]
    optional: bool,
}

#[allow(dead_code)]
fn closure_arity_from_type(ty: &str) -> Option<usize> {
    if ty.starts_with("Fn[") {
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

pub fn generate(workspace_root: &Path, out_dir: &Path) {
    let defs_dir = workspace_root.join("stdlib/defs");
    if !defs_dir.exists() {
        return;
    }

    // Scan runtime .rs files for actual function signatures
    let mut runtime_param_types: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let runtime_dir = workspace_root.join("runtime/rs/src");
    if runtime_dir.exists() {
        for entry in fs::read_dir(&runtime_dir).unwrap().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "rs") {
                continue;
            }
            let content = fs::read_to_string(&path).unwrap_or_default();
            for line in content.lines() {
                if let Some(start) = line.find("pub fn almide_rt_") {
                    let rest = &line[start..];
                    if let (Some(paren_open), Some(paren_close)) =
                        (rest.find('('), rest.find(')'))
                    {
                        let fn_name = &rest[7..paren_open];
                        let params_str = &rest[paren_open + 1..paren_close];
                        let param_types: Vec<String> = params_str
                            .split(',')
                            .map(|p| {
                                let p = p.trim();
                                if let Some(colon_pos) = p.find(':') {
                                    p[colon_pos + 1..].trim().to_string()
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

    let mut entries: Vec<_> = fs::read_dir(&defs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "toml"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut arg_transform_arms = String::new();

    for entry in entries {
        let path = entry.path();
        let module_name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let content = fs::read_to_string(&path).unwrap();
        let defs: BTreeMap<String, FnDef> = toml::from_str(&content).unwrap();

        for (fn_name, def) in &defs {
            let rust_tmpl = &def.rust;
            let mut transforms = Vec::new();
            for (i, param) in def.params.iter().enumerate() {
                let pname = &param.name;
                let rt_fn_name = format!("almide_rt_{}_{}", module_name, fn_name);
                let runtime_ty = runtime_param_types
                    .get(&rt_fn_name)
                    .and_then(|types| types.get(i))
                    .map(|s| s.as_str())
                    .unwrap_or("");

                let transform = if rust_tmpl.contains(&format!("{{{}.args}}", pname))
                    || rust_tmpl.contains(&format!("{{{}.body}}", pname))
                {
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
                    if runtime_ty == "String" {
                        "ArgTransform::Direct"
                    } else {
                        "ArgTransform::BorrowStr"
                    }
                } else if rust_tmpl.contains(&format!("&mut {{{}}}", pname)) {
                    "ArgTransform::BorrowMut"
                } else if rust_tmpl.contains(&format!("&{{{}}}", pname)) {
                    "ArgTransform::BorrowRef"
                } else {
                    "ArgTransform::Direct"
                };
                transforms.push(transform.to_string());
            }

            let rt_name = {
                let tmpl = &def.rust;
                if let Some(paren) = tmpl.find('(') {
                    let prefix = &tmpl[..paren];
                    let name = prefix
                        .rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
                        .next()
                        .unwrap_or("");
                    if !name.is_empty() {
                        name.to_string()
                    } else {
                        format!("almide_rt_{}_{}", module_name, fn_name)
                    }
                } else {
                    format!("almide_rt_{}_{}", module_name, fn_name)
                }
            };

            let effect_suffix = if def.effect { "true" } else { "false" };
            let has_mut = transforms.iter().any(|t| t.contains("BorrowMut"));
            let is_pure = !def.effect && !def.impure && !has_mut;
            let pure_suffix = if is_pure { "true" } else { "false" };
            let required_count = def.params.iter().filter(|p| !p.optional).count();
            arg_transform_arms.push_str(&format!(
                "            (\"{module}\", \"{func}\") => Some(StdlibCallInfo {{ args: &[{transforms}], effect: {effect}, pure_: {pure_}, name: \"{rt_name}\", required: {required} }}),\n",
                module = module_name,
                func = fn_name,
                transforms = transforms.join(", "),
                effect = effect_suffix,
                pure_ = pure_suffix,
                required = required_count,
            ));
        }
    }

    let arg_transforms_file = format!(
        "// AUTO-GENERATED by build.rs from stdlib/defs/*.toml — DO NOT EDIT\n\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub enum ArgTransform {{\n\
         \x20   Direct,     // pass as-is\n\
         \x20   BorrowStr,  // &*expr (borrow as &str)\n\
         \x20   BorrowRef,  // &expr (borrow as reference)\n\
         \x20   BorrowMut,  // &mut expr (mutable borrow, strips clone)\n\
         \x20   ToVec,      // (expr).to_vec() (owned copy)\n\
         \x20   LambdaClone, // lambda with clone bindings\n\
         \x20   WrapSome,   // Some(expr) (wrap in Option)\n\
         \x20   LambdaResultWrap, // lambda with Ok(body) wrapping\n\
         }}\n\n\
         pub struct StdlibCallInfo {{\n\
         \x20   pub args: &'static [ArgTransform],\n\
         \x20   pub effect: bool,\n\
         \x20   pub pure_: bool,\n\
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

    fs::write(out_dir.join("arg_transforms.rs"), arg_transforms_file).unwrap();

    // Rerun if defs change
    println!("cargo:rerun-if-changed={}", defs_dir.display());
    for entry in fs::read_dir(&defs_dir).unwrap().filter_map(|e| e.ok()) {
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }
}
