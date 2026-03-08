use crate::ast::*;
use crate::stdlib;
use super::Emitter;

impl Emitter {
    pub(crate) fn resolve_ufcs_module(method: &str) -> Option<&'static str> {
        stdlib::resolve_ufcs_module(method)
    }

    /// Extract lambda parameter names and body code from a lambda expression or function reference.
    pub(crate) fn inline_lambda(&self, lambda_arg: &Expr, arity: usize) -> (Vec<String>, String) {
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
    }

    pub(crate) fn gen_call(&self, callee: &Expr, args: &[Expr]) -> String {
        // Handle module calls
        if let Expr::Member { object, field } = callee {
            if let Expr::Ident { name: module } = object.as_ref() {
                let is_stdlib = stdlib::is_stdlib_module(module);
                let resolved_mod = self.module_aliases.get(module.as_str())
                    .cloned()
                    .unwrap_or_else(|| module.to_string());
                let is_user_module = self.user_modules.contains(&resolved_mod);
                if is_stdlib || is_user_module {
                    return self.gen_module_call(module, field, args);
                }
            }
            // UFCS: receiver.method(args) => module.method(receiver, args)
            if let Some(resolved) = Self::resolve_ufcs_module(field) {
                let mut new_args = vec![object.as_ref().clone()];
                new_args.extend(args.iter().cloned());
                return self.gen_module_call(resolved, field, &new_args);
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
        // Auto-propagate ? for effect fn calls within effect context (not in tests, not suppressed)
        if self.in_effect && !self.in_test && !self.skip_auto_q.get() {
            if let Expr::Ident { name } = callee {
                if self.effect_fns.contains(name) {
                    return format!("{}?", call);
                }
            }
        }
        // In do blocks, auto-unwrap calls to Result-returning functions
        if self.in_do_block.get() {
            if let Expr::Ident { name } = callee {
                if self.result_fns.contains(name) || self.effect_fns.contains(name) {
                    return format!("{}?", call);
                }
            }
        }
        call
    }

    pub(crate) fn gen_module_call(&self, module: &str, func: &str, args: &[Expr]) -> String {
        let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
        match module {
            "fs" => match func {
                "read_text" => format!("std::fs::read_to_string(&*{}).map_err(AlmideIoError::from)?", args_str[0]),
                "write" => format!("std::fs::write(&*{}, &*{}).map_err(AlmideIoError::from)?", args_str[0], args_str[1]),
                "write_bytes" => format!("std::fs::write(&*{}, &{}).map_err(AlmideIoError::from)?", args_str[0], args_str[1]),
                "read_bytes" => format!("std::fs::read(&*{}).map_err(AlmideIoError::from)?", args_str[0]),
                "exists?" | "exists_qm_" => format!("std::path::Path::new(&*{}).exists()", args_str[0]),
                "mkdir_p" => format!("std::fs::create_dir_all(&*{}).map_err(AlmideIoError::from)?", args_str[0]),
                "append" => format!("{{ let prev = std::fs::read_to_string(&*{}).unwrap_or_default(); std::fs::write(&*{}, format!(\"{{}}{{}}\", prev, {})).map_err(AlmideIoError::from)?; }}", args_str[0], args_str[0], args_str[1]),
                "read_lines" => format!("{{ let s = std::fs::read_to_string(&*{}).map_err(AlmideIoError::from)?; s.split('\\n').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect::<Vec<String>>() }}", args_str[0]),
                "remove" => format!("std::fs::remove_file(&*{}).map_err(AlmideIoError::from)?", args_str[0]),
                "list_dir" => format!("{{ let mut v: Vec<String> = std::fs::read_dir(&*{}).map_err(AlmideIoError::from)?.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect(); v.sort(); v }}", args_str[0]),
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
                "chars" => format!("({}).chars().map(|c| c.to_string()).collect::<Vec<String>>()", args_str[0]),
                "index_of" => format!("({}).find(&*{}).map(|i| i as i64)", args_str[0], args_str[1]),
                "repeat" => format!("({}).repeat({} as usize)", args_str[0], args_str[1]),
                "from_bytes" => format!("{{ let __bytes: Vec<i64> = {}; String::from_utf8(__bytes.iter().map(|b| *b as u8).collect::<Vec<u8>>()).unwrap_or_default() }}", args_str[0]),
                "is_digit?" | "is_digit_qm_" => format!("({}).chars().all(|c| c.is_ascii_digit()) && !({}).is_empty()", args_str[0], args_str[0]),
                "is_alpha?" | "is_alpha_qm_" => format!("({}).chars().all(|c| c.is_ascii_alphabetic()) && !({}).is_empty()", args_str[0], args_str[0]),
                "is_alphanumeric?" | "is_alphanumeric_qm_" => format!("({}).chars().all(|c| c.is_ascii_alphanumeric()) && !({}).is_empty()", args_str[0], args_str[0]),
                "is_whitespace?" | "is_whitespace_qm_" => format!("({}).chars().all(|c| c.is_whitespace()) && !({}).is_empty()", args_str[0], args_str[0]),
                _ => format!("/* string.{} */ todo!()", func),
            },
            "list" => {
                match func {
                    "len" => format!("(({}).len() as i64)", args_str[0]),
                    "get" => format!("({}).get({} as usize).cloned()", args_str[0], args_str[1]),
                    "get_or" => format!("({}).get({} as usize).cloned().unwrap_or({})", args_str[0], args_str[1], args_str[2]),
                    "sort" => format!("{{ let mut v = ({}).to_vec(); v.sort(); v }}", args_str[0]),
                    "reverse" => format!("{{ let mut v = ({}).to_vec(); v.reverse(); v }}", args_str[0]),
                    "any" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("({}).iter().any(|{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "all" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("({}).iter().all(|{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "contains" => format!("({}).contains(&{})", args_str[0], args_str[1]),
                    "each" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("{{ for {} in ({}).iter().cloned() {{ {} ; }} }}", names[0], args_str[0], body)
                    }
                    "map" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        // If in effect context and body contains ?, use try_collect pattern
                        if self.in_effect && body.contains("?") {
                            format!("({}).clone().into_iter().map(|{}| -> Result<_, String> {{ Ok({{ {} }}) }}).collect::<Result<Vec<_>, _>>()?", args_str[0], names[0], body)
                        } else {
                            format!("({}).clone().into_iter().map(|{}| {{ {} }}).collect::<Vec<_>>()", args_str[0], names[0], body)
                        }
                    }
                    "filter" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("({}).clone().into_iter().filter(|{}| {{ let {} = {}.clone(); {} }}).collect::<Vec<_>>()", args_str[0], names[0], names[0], names[0], body)
                    }
                    "find" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("({}).clone().into_iter().find(|{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "fold" => {
                        let (names, body) = self.inline_lambda(&args[2], 2);
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
                    "enumerate" => format!("({}).clone().into_iter().enumerate().map(|(i, x)| (i as i64, x)).collect::<Vec<_>>()", args_str[0]),
                    "zip" => format!("({}).clone().into_iter().zip(({}).clone().into_iter()).collect::<Vec<_>>()", args_str[0], args_str[1]),
                    "flatten" => format!("({}).clone().into_iter().flatten().collect::<Vec<_>>()", args_str[0]),
                    "take" => format!("({}).clone().into_iter().take({} as usize).collect::<Vec<_>>()", args_str[0], args_str[1]),
                    "drop" => format!("({}).clone().into_iter().skip({} as usize).collect::<Vec<_>>()", args_str[0], args_str[1]),
                    "sort_by" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("{{ let mut v = ({}).to_vec(); v.sort_by(|__a, __b| {{ let {n} = __a.clone(); let __ka = {{ {body} }}; let {n} = __b.clone(); let __kb = {{ {body} }}; __ka.partial_cmp(&__kb).unwrap_or(std::cmp::Ordering::Equal) }}); v }}", args_str[0], n = names[0], body = body)
                    }
                    "unique" => format!("{{ let mut seen = Vec::new(); let mut out = Vec::new(); for x in ({}).iter() {{ if !seen.contains(x) {{ seen.push(x.clone()); out.push(x.clone()); }} }} out }}", args_str[0]),
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
                    let (names, body) = self.inline_lambda(&args[1], 1);
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
                "from_int" => format!("({} as f64)", args_str[0]),
                _ => format!("/* float.{} */ todo!()", func),
            },
            "env" => match func {
                "unix_timestamp" => {
                    "(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)".to_string()
                }
                "args" => "std::env::args().collect::<Vec<String>>()".to_string(),
                "get" => format!("std::env::var(&*{}).ok()", args_str[0]),
                "set" => format!("std::env::set_var(&*{}, &*{})", args_str[0], args_str[1]),
                "cwd" => "std::env::current_dir().map(|p| p.to_string_lossy().to_string()).map_err(|e| e.to_string())?".to_string(),
                _ => format!("/* env.{} */ todo!()", func),
            },
            "process" => match func {
                "exec" => format!("match std::process::Command::new(&*{}).args({{ let __a: Vec<String> = {}; __a }}.iter().map(|s| s.as_str())).output() {{ Ok(__out) => if __out.status.success() {{ Ok(String::from_utf8_lossy(&__out.stdout).to_string()) }} else {{ Err(String::from_utf8_lossy(&__out.stderr).to_string()) }}, Err(e) => Err(e.to_string()) }}", args_str[0], args_str[1]),
                "exit" => format!("std::process::exit({} as i32)", args_str[0]),
                "stdin_lines" => "{{ use std::io::BufRead; std::io::stdin().lock().lines().collect::<Result<Vec<String>, _>>().map_err(|e| e.to_string())? }}".to_string(),
                _ => format!("/* process.{} */ todo!()", func),
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
            "http" => match func {
                "serve" => {
                    let (names, body) = self.inline_lambda(&args[1], 1);
                    // serve returns Result<(), String> which matches effect fn main's return type
                    format!("{{ almide_http_serve({}, |{}| -> Result<AlmideHttpResponse, String> {{ Ok({{ {} }}) }})?; Ok(()) }}", args_str[0], names[0], body)
                }
                "response" => format!("AlmideHttpResponse::new({}, {}.to_string())", args_str[0], args_str[1]),
                "json" => format!("AlmideHttpResponse::json({}, {}.to_string())", args_str[0], args_str[1]),
                "with_headers" => format!("AlmideHttpResponse::with_headers({}, {}.to_string(), {})", args_str[0], args_str[1], args_str[2]),
                "get" => format!("almide_http_get(&{})?", args_str[0]),
                "post" => format!("almide_http_post(&{}, &{})?", args_str[0], args_str[1]),
                _ => format!("/* http.{} */ todo!()", func),
            },
            "math" => match func {
                "min" => format!("std::cmp::min({}, {})", args_str[0], args_str[1]),
                "max" => format!("std::cmp::max({}, {})", args_str[0], args_str[1]),
                "abs" => format!("({}).abs()", args_str[0]),
                "pow" => format!("({} as i64).wrapping_pow({} as u32)", args_str[0], args_str[1]),
                "pi" => "std::f64::consts::PI".to_string(),
                "e" => "std::f64::consts::E".to_string(),
                "sin" => format!("({} as f64).sin()", args_str[0]),
                "cos" => format!("({} as f64).cos()", args_str[0]),
                "tan" => format!("({} as f64).tan()", args_str[0]),
                "log" => format!("({} as f64).ln()", args_str[0]),
                "exp" => format!("({} as f64).exp()", args_str[0]),
                "sqrt" => format!("({} as f64).sqrt()", args_str[0]),
                _ => format!("/* math.{} */ todo!()", func),
            },
            "random" => match func {
                "int" => format!("{{ let __range = ({1} - {0} + 1) as u64; let mut __buf = [0u8; 8]; std::fs::File::open(\"/dev/urandom\").and_then(|mut f| {{ use std::io::Read; f.read_exact(&mut __buf) }}).unwrap(); let __r = u64::from_le_bytes(__buf) % __range; ({0} + __r as i64) }}", args_str[0], args_str[1]),
                "float" => "{ let mut __buf = [0u8; 8]; std::fs::File::open(\"/dev/urandom\").and_then(|mut f| { use std::io::Read; f.read_exact(&mut __buf) }).unwrap(); (u64::from_le_bytes(__buf) as f64) / (u64::MAX as f64) }".to_string(),
                "choice" => format!("{{ let __xs = &{}; if __xs.is_empty() {{ None }} else {{ let mut __buf = [0u8; 8]; std::fs::File::open(\"/dev/urandom\").and_then(|mut f| {{ use std::io::Read; f.read_exact(&mut __buf) }}).unwrap(); Some(__xs[(u64::from_le_bytes(__buf) as usize) % __xs.len()].clone()) }} }}", args_str[0]),
                "shuffle" => format!("{{ let mut __v = ({}).clone(); let __n = __v.len(); for __i in (1..__n).rev() {{ let mut __buf = [0u8; 8]; std::fs::File::open(\"/dev/urandom\").and_then(|mut f| {{ use std::io::Read; f.read_exact(&mut __buf) }}).unwrap(); let __j = (u64::from_le_bytes(__buf) as usize) % (__i + 1); __v.swap(__i, __j); }} __v }}", args_str[0]),
                _ => format!("/* random.{} */ todo!()", func),
            },
            "regex" => match func {
                "match?" | "match_qm_" => format!("almide_regex_is_match(&{}, &{})", args_str[0], args_str[1]),
                "full_match?" | "full_match_qm_" => format!("almide_regex_full_match(&{}, &{})", args_str[0], args_str[1]),
                "find" => format!("almide_regex_find(&{}, &{})", args_str[0], args_str[1]),
                "find_all" => format!("almide_regex_find_all(&{}, &{})", args_str[0], args_str[1]),
                "replace" => format!("almide_regex_replace(&{}, &{}, &{})", args_str[0], args_str[1], args_str[2]),
                "replace_first" => format!("almide_regex_replace_first(&{}, &{}, &{})", args_str[0], args_str[1], args_str[2]),
                "split" => format!("almide_regex_split(&{}, &{})", args_str[0], args_str[1]),
                "captures" => format!("almide_regex_captures(&{}, &{})", args_str[0], args_str[1]),
                _ => format!("/* regex.{} */ todo!()", func),
            },
            "time" => match func {
                "now" => "(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)".to_string(),
                "millis" => "(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64)".to_string(),
                "sleep" => format!("std::thread::sleep(std::time::Duration::from_millis({} as u64))", args_str[0]),
                "year" => format!("almide_time_parts({}).0", args_str[0]),
                "month" => format!("almide_time_parts({}).1", args_str[0]),
                "day" => format!("almide_time_parts({}).2", args_str[0]),
                "hour" => format!("almide_time_parts({}).3", args_str[0]),
                "minute" => format!("almide_time_parts({}).4", args_str[0]),
                "second" => format!("almide_time_parts({}).5", args_str[0]),
                "weekday" => format!("almide_time_weekday({})", args_str[0]),
                "to_iso" => format!("almide_time_to_iso({})", args_str[0]),
                "from_parts" => format!("almide_time_from_parts({}, {}, {}, {}, {}, {})", args_str[0], args_str[1], args_str[2], args_str[3], args_str[4], args_str[5]),
                _ => format!("/* time.{} */ todo!()", func),
            },
            _ => {
                let resolved = self.module_aliases.get(module)
                    .cloned()
                    .unwrap_or_else(|| module.to_string());
                let call = format!("{}::{}({})", resolved, func, args_str.join(", "));
                // Auto-propagate ? for user module effect/Result functions
                if self.in_effect && (self.effect_fns.contains(&func.to_string()) || self.result_fns.contains(&func.to_string())) {
                    format!("{}?", call)
                } else {
                    call
                }
            }
        }
    }
}
