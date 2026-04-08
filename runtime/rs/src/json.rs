// json extern — JSON parse, stringify, and Value query/manipulation
// Value type is defined in value.rs (included before this file)

// ── Parse + Stringify ──

pub fn almide_rt_json_stringify(v: Value) -> String { almide_rt_value_stringify(&v) }

pub fn almide_rt_json_parse(text: String) -> Result<Value, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    fn skip_ws(chars: &[char], pos: &mut usize) { while *pos < chars.len() && chars[*pos].is_whitespace() { *pos += 1; } }
    fn parse_value(chars: &[char], pos: &mut usize) -> Result<Value, String> {
        skip_ws(chars, pos);
        if *pos >= chars.len() { return Err("unexpected end of input".into()); }
        match chars[*pos] {
            '"' => parse_string(chars, pos).map(Value::Str),
            '{' => parse_object(chars, pos),
            '[' => parse_array(chars, pos),
            't' | 'f' => parse_bool(chars, pos),
            'n' => parse_null(chars, pos),
            c if c == '-' || c.is_ascii_digit() => parse_number(chars, pos),
            c => Err(format!("unexpected char '{}' at pos {}", c, pos)),
        }
    }
    fn parse_string(chars: &[char], pos: &mut usize) -> Result<String, String> {
        *pos += 1; let mut s = String::new();
        while *pos < chars.len() && chars[*pos] != '"' {
            if chars[*pos] == '\\' { *pos += 1; match chars.get(*pos) { Some('n')=>s.push('\n'), Some('t')=>s.push('\t'), Some('"')=>s.push('"'), Some('\\')=>s.push('\\'), Some('/')=>s.push('/'), Some('u')=>{s.push('?');*pos+=4;} _=>{} } } else { s.push(chars[*pos]); }
            *pos += 1;
        }
        if *pos < chars.len() { *pos += 1; } Ok(s)
    }
    fn parse_number(chars: &[char], pos: &mut usize) -> Result<Value, String> {
        let start = *pos; if chars[*pos]=='-'{*pos+=1;} while *pos<chars.len()&&chars[*pos].is_ascii_digit(){*pos+=1;} let mut is_float=false;
        if *pos<chars.len()&&chars[*pos]=='.'{is_float=true;*pos+=1;while *pos<chars.len()&&chars[*pos].is_ascii_digit(){*pos+=1;}}
        if *pos<chars.len()&&(chars[*pos]=='e'||chars[*pos]=='E'){is_float=true;*pos+=1;if *pos<chars.len()&&(chars[*pos]=='+'||chars[*pos]=='-'){*pos+=1;}while *pos<chars.len()&&chars[*pos].is_ascii_digit(){*pos+=1;}}
        let s:String=chars[start..*pos].iter().collect();
        if is_float{s.parse::<f64>().map(Value::Float).map_err(|e|e.to_string())}else{s.parse::<i64>().map(Value::Int).map_err(|e|e.to_string())}
    }
    fn parse_bool(chars:&[char],pos:&mut usize)->Result<Value,String>{if chars[*pos..].starts_with(&['t','r','u','e']){*pos+=4;Ok(Value::Bool(true))}else if chars[*pos..].starts_with(&['f','a','l','s','e']){*pos+=5;Ok(Value::Bool(false))}else{Err("expected bool".into())}}
    fn parse_null(chars:&[char],pos:&mut usize)->Result<Value,String>{if chars[*pos..].starts_with(&['n','u','l','l']){*pos+=4;Ok(Value::Null)}else{Err("expected null".into())}}
    fn parse_array(chars:&[char],pos:&mut usize)->Result<Value,String>{*pos+=1;skip_ws(chars,pos);let mut items=Vec::new();if *pos<chars.len()&&chars[*pos]==']'{*pos+=1;return Ok(Value::Array(items));}loop{items.push(parse_value(chars,pos)?);skip_ws(chars,pos);if *pos<chars.len()&&chars[*pos]==','{*pos+=1;skip_ws(chars,pos);}else{break;}}skip_ws(chars,pos);if *pos<chars.len()&&chars[*pos]==']'{*pos+=1;}Ok(Value::Array(items))}
    fn parse_object(chars:&[char],pos:&mut usize)->Result<Value,String>{*pos+=1;skip_ws(chars,pos);let mut pairs=Vec::new();if *pos<chars.len()&&chars[*pos]=='}'{*pos+=1;return Ok(Value::Object(pairs));}loop{skip_ws(chars,pos);let key=parse_string(chars,pos)?;skip_ws(chars,pos);if *pos<chars.len()&&chars[*pos]==':'{*pos+=1;}let val=parse_value(chars,pos)?;pairs.push((key,val));skip_ws(chars,pos);if *pos<chars.len()&&chars[*pos]==','{*pos+=1;}else{break;}}skip_ws(chars,pos);if *pos<chars.len()&&chars[*pos]=='}'{*pos+=1;}Ok(Value::Object(pairs))}
    parse_value(&chars, &mut pos)
}

// ── Key-based getters ──

pub fn almide_json_get(j: &Value, key: &str) -> Option<Value> {
    match j {
        Value::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
        _ => None,
    }
}

pub fn almide_json_get_string(j: &Value, key: &str) -> Option<String> {
    match almide_json_get(j, key)? { Value::Str(s) => Some(s), _ => None }
}

pub fn almide_json_get_int(j: &Value, key: &str) -> Option<i64> {
    match almide_json_get(j, key)? { Value::Int(n) => Some(n), Value::Float(f) => Some(f as i64), _ => None }
}

pub fn almide_json_get_float(j: &Value, key: &str) -> Option<f64> {
    match almide_json_get(j, key)? { Value::Float(f) => Some(f), Value::Int(n) => Some(n as f64), _ => None }
}

pub fn almide_json_get_bool(j: &Value, key: &str) -> Option<bool> {
    match almide_json_get(j, key)? { Value::Bool(b) => Some(b), _ => None }
}

pub fn almide_json_get_array(j: &Value, key: &str) -> Option<Vec<Value>> {
    match almide_json_get(j, key)? { Value::Array(a) => Some(a), _ => None }
}

// ── Keyless extractors ──

pub fn almide_json_to_string(j: &Value) -> Option<String> {
    match j { Value::Str(s) => Some(s.clone()), _ => None }
}

pub fn almide_json_to_int(j: &Value) -> Option<i64> {
    match j { Value::Int(n) => Some(*n), Value::Float(f) => Some(*f as i64), _ => None }
}

pub fn almide_json_as_float(j: &Value) -> Option<f64> {
    match j { Value::Float(f) => Some(*f), Value::Int(n) => Some(*n as f64), _ => None }
}

pub fn almide_json_as_bool(j: &Value) -> Option<bool> {
    match j { Value::Bool(b) => Some(*b), _ => None }
}

pub fn almide_json_as_array(j: &Value) -> Option<Vec<Value>> {
    match j { Value::Array(a) => Some(a.clone()), _ => None }
}

// ── Object operations ──

pub fn almide_json_keys(j: &Value) -> Vec<String> {
    match j { Value::Object(entries) => entries.iter().map(|(k, _)| k.clone()).collect(), _ => vec![] }
}

pub fn almide_json_to_map(j: &Value) -> Option<HashMap<String, String>> {
    match j {
        Value::Object(entries) => {
            let map: HashMap<String, String> = entries.iter().map(|(k, v)| {
                let s = match v {
                    Value::Str(s) => s.clone(),
                    _ => almide_rt_value_stringify(v),
                };
                (k.clone(), s)
            }).collect();
            Some(map)
        }
        _ => None,
    }
}

pub fn almide_json_object(entries: Vec<(String, Value)>) -> Value {
    Value::Object(entries)
}

pub fn almide_json_from_float(n: f64) -> Value { Value::Float(n) }

// ── Stringify pretty ──

pub fn almide_json_stringify_pretty(j: &Value) -> String {
    stringify_value(j, 0)
}

fn stringify_value(v: &Value, depth: usize) -> String {
    let ind = "  ".repeat(depth);
    let ind1 = "  ".repeat(depth + 1);
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => if *b { "true" } else { "false" }.into(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{}", f),
        Value::Str(s) => format!("{:?}", s),
        Value::Array(items) => {
            if items.is_empty() { return "[]".into(); }
            let parts: Vec<String> = items.iter().map(|v| format!("{}{}", ind1, stringify_value(v, depth + 1))).collect();
            format!("[\n{}\n{}]", parts.join(",\n"), ind)
        }
        Value::Object(entries) => {
            if entries.is_empty() { return "{}".into(); }
            let parts: Vec<String> = entries.iter().map(|(k, v)| format!("{}{:?}: {}", ind1, k, stringify_value(v, depth + 1))).collect();
            format!("{{\n{}\n{}}}", parts.join(",\n"), ind)
        }
    }
}

// ── JsonPath type and operations ──

type JsonPath = AlmideJsonPath;

#[derive(Debug, Clone, PartialEq)]
pub enum AlmideJsonPath {
    JpRoot,
    JpField(Box<AlmideJsonPath>, String),
    JpIndex(Box<AlmideJsonPath>, i64),
}

impl std::fmt::Display for AlmideJsonPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlmideJsonPath::JpRoot => write!(f, "$"),
            AlmideJsonPath::JpField(parent, name) => write!(f, "{}.{}", parent, name),
            AlmideJsonPath::JpIndex(parent, i) => write!(f, "{}[{}]", parent, i),
        }
    }
}

// Wrapper functions for stdlib codegen (json.root(), json.field(), json.index())
pub fn almide_rt_json_root() -> AlmideJsonPath { AlmideJsonPath::JpRoot }
pub fn almide_rt_json_field(path: AlmideJsonPath, name: String) -> AlmideJsonPath { AlmideJsonPath::JpField(Box::new(path), name) }
pub fn almide_rt_json_index(path: AlmideJsonPath, i: i64) -> AlmideJsonPath { AlmideJsonPath::JpIndex(Box::new(path), i) }

/// Resolve a JsonPath to a list of traversal steps, root-first.
fn resolve_path(path: &AlmideJsonPath) -> Vec<PathStep> {
    let mut steps = Vec::new();
    let mut current = path;
    loop {
        match current {
            AlmideJsonPath::JpRoot => break,
            AlmideJsonPath::JpField(parent, name) => {
                steps.push(PathStep::Field(name.clone()));
                current = parent;
            }
            AlmideJsonPath::JpIndex(parent, i) => {
                steps.push(PathStep::Index(*i));
                current = parent;
            }
        }
    }
    steps.reverse();
    steps
}

enum PathStep {
    Field(String),
    Index(i64),
}

fn get_by_step(v: &Value, step: &PathStep) -> Option<Value> {
    match step {
        PathStep::Field(key) => almide_json_get(v, key),
        PathStep::Index(i) => match v {
            Value::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                items.get(idx).cloned()
            }
            _ => None,
        },
    }
}

pub fn almide_json_get_path(j: &Value, path: &AlmideJsonPath) -> Option<Value> {
    let steps = resolve_path(path);
    let mut current = j.clone();
    for step in &steps {
        current = get_by_step(&current, step)?;
    }
    Some(current)
}

pub fn almide_json_set_path(j: &Value, path: &AlmideJsonPath, value: Value) -> Result<Value, String> {
    let steps = resolve_path(path);
    Ok(set_at_steps(j, &steps, &value))
}

pub fn almide_json_remove_path(j: &Value, path: &AlmideJsonPath) -> Value {
    let steps = resolve_path(path);
    remove_at_steps(j, &steps)
}

fn set_at_steps(j: &Value, steps: &[PathStep], value: &Value) -> Value {
    if steps.is_empty() { return value.clone(); }
    match &steps[0] {
        PathStep::Field(key) => match j {
            Value::Object(entries) => {
                let rest = &steps[1..];
                let mut new_entries: Vec<(String, Value)> = entries.iter()
                    .map(|(k, v)| if k == key { (k.clone(), set_at_steps(v, rest, value)) } else { (k.clone(), v.clone()) })
                    .collect();
                if !entries.iter().any(|(k, _)| k == key) {
                    new_entries.push((key.clone(), set_at_steps(&Value::Object(vec![]), rest, value)));
                }
                Value::Object(new_entries)
            }
            _ => Value::Object(vec![(key.clone(), set_at_steps(&Value::Object(vec![]), &steps[1..], value))]),
        },
        PathStep::Index(i) => match j {
            Value::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                let mut new_items = items.clone();
                if idx < new_items.len() {
                    new_items[idx] = set_at_steps(&new_items[idx], &steps[1..], value);
                }
                Value::Array(new_items)
            }
            _ => j.clone(),
        },
    }
}

fn remove_at_steps(j: &Value, steps: &[PathStep]) -> Value {
    if steps.is_empty() { return Value::Null; }
    match &steps[0] {
        PathStep::Field(key) => match j {
            Value::Object(entries) => {
                if steps.len() == 1 {
                    Value::Object(entries.iter().filter(|(k, _)| k != key).cloned().collect())
                } else {
                    Value::Object(entries.iter().map(|(k, v)| {
                        if k == key { (k.clone(), remove_at_steps(v, &steps[1..])) } else { (k.clone(), v.clone()) }
                    }).collect())
                }
            }
            other => other.clone(),
        },
        PathStep::Index(i) => match j {
            Value::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                if steps.len() == 1 {
                    Value::Array(items.iter().enumerate().filter(|(ii, _)| *ii != idx).map(|(_, v)| v.clone()).collect())
                } else {
                    let mut new_items = items.clone();
                    if idx < new_items.len() {
                        new_items[idx] = remove_at_steps(&new_items[idx], &steps[1..]);
                    }
                    Value::Array(new_items)
                }
            }
            other => other.clone(),
        },
    }
}
