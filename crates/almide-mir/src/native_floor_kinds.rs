// the declared param and return kinds of native_floor.rs, include!-spliced back at module level (a code move under the
// 800-line ceiling the codopsy gate holds this crate to).


/// `A` when `t` is `Set[A]` over a type param.
fn set_param_name(t: &TypeExpr, type_params: &BTreeSet<String>) -> Option<String> {
    match t {
        TypeExpr::Generic { name, args } if name.as_str() == "Set" && args.len() == 1 => {
            simple(&args[0]).filter(|a| type_params.contains(*a)).map(str::to_string)
        }
        _ => None,
    }
}

fn set_param(t: &TypeExpr, a: &str) -> Option<FloorParam> {
    match t {
        TypeExpr::Simple { name } if name.as_str() == a => Some(FloorParam::SetElem(false)),
        TypeExpr::Generic { name, args } if name.as_str() == "Set" && args.len() == 1 && simple(&args[0]) == Some(a) => {
            Some(FloorParam::Set(false))
        }
        _ => None,
    }
}

fn set_ret(t: &TypeExpr, a: &str) -> Option<FloorRet> {
    match t {
        TypeExpr::Generic { name, args } if args.len() == 1 && simple(&args[0]) == Some(a) => match name.as_str() {
            "Set" => Some(FloorRet::Set),
            "List" => Some(FloorRet::SetElems),
            _ => None,
        },
        _ => None,
    }
}

/// `AlmideSet<A>` over a type param (squashed).
fn is_generic_set(t: &str) -> bool {
    t.strip_prefix("AlmideSet<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param)
}

/// `(K, V)` when `t` is `Map[K, V]` or `List[(K, V)]` over two type params.
fn map_params(t: &TypeExpr, type_params: &BTreeSet<String>) -> Option<(String, String)> {
    let kv = |args: &[TypeExpr]| -> Option<(String, String)> {
        let (k, v) = (simple(args.first()?)?, simple(args.get(1)?)?);
        (args.len() == 2 && type_params.contains(k) && type_params.contains(v)).then(|| (k.to_string(), v.to_string()))
    };
    match t {
        TypeExpr::Generic { name, args } if name.as_str() == "Map" => kv(args),
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => match &args[0] {
            TypeExpr::Tuple { elements } => kv(elements),
            _ => None,
        },
        _ => None,
    }
}

fn map_param(t: &TypeExpr, (k, v): &(String, String)) -> Option<FloorParam> {
    match t {
        TypeExpr::Simple { name } if name.as_str() == k => Some(FloorParam::MapKey(false)),
        TypeExpr::Simple { name } if name.as_str() == v => Some(FloorParam::MapVal(false)),
        TypeExpr::Generic { name, .. } if name.as_str() == "Map" => {
            map_params(t, &[k.clone(), v.clone()].into_iter().collect()).filter(|kv| kv == &(k.clone(), v.clone())).map(|_| FloorParam::Map(false))
        }
        TypeExpr::Generic { name, .. } if name.as_str() == "List" => {
            map_params(t, &[k.clone(), v.clone()].into_iter().collect()).filter(|kv| kv == &(k.clone(), v.clone())).map(|_| FloorParam::MapPairs)
        }
        _ => None,
    }
}

fn map_ret(t: &TypeExpr, (k, v): &(String, String)) -> Option<FloorRet> {
    match t {
        TypeExpr::Generic { name, args } if name.as_str() == "Map" => {
            (args.len() == 2 && simple(&args[0]) == Some(k.as_str()) && simple(&args[1]) == Some(v.as_str())).then_some(FloorRet::Map)
        }
        TypeExpr::Generic { name, args } if is_option_name(name.as_str()) && args.len() == 1 && simple(&args[0]) == Some(v.as_str()) => {
            Some(FloorRet::MapOptVal)
        }
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => match (&args[0], simple(&args[0])) {
            (_, Some(x)) if x == k => Some(FloorRet::MapKeys),
            (_, Some(x)) if x == v => Some(FloorRet::MapVals),
            (TypeExpr::Tuple { elements }, _)
                if elements.len() == 2 && simple(&elements[0]) == Some(k.as_str()) && simple(&elements[1]) == Some(v.as_str()) =>
            {
                Some(FloorRet::MapEntries)
            }
            _ => None,
        },
        _ => None,
    }
}

/// The Almide type a declaration spells, for the declared types the native
/// render carries (scalars, String, lists, tuples, Option, Map); `None` for
/// any other spelling.
fn ty_of(t: &TypeExpr) -> Option<almide_lang::types::Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    Some(match t {
        TypeExpr::Simple { name } => match name.as_str() {
            "Int" => Ty::Int,
            "Bool" => Ty::Bool,
            "Float" => Ty::Float,
            "String" => Ty::String,
            _ => return None,
        },
        TypeExpr::Tuple { elements } => Ty::Tuple(elements.iter().map(ty_of).collect::<Option<Vec<_>>>()?.into_iter().collect()),
        TypeExpr::Generic { name, args } => {
            let c = match name.as_str() {
                "List" => TypeConstructorId::List,
                "Map" => TypeConstructorId::Map,
                n if is_option_name(n) => TypeConstructorId::Option,
                _ => return None,
            };
            Ty::Applied(c, args.iter().map(ty_of).collect::<Option<Vec<_>>>()?.into_iter().collect())
        }
        _ => return None,
    })
}

/// `AlmideMap<A, B>` over two type params (squashed).
fn is_generic_map(t: &str) -> bool {
    t.strip_prefix("AlmideMap<")
        .and_then(|x| x.strip_suffix('>'))
        .and_then(|x| x.split_once(','))
        .is_some_and(|(a, b)| is_type_param(a) && is_type_param(b))
}

fn scalar_of(name: &str) -> Option<Scalar> {
    match name {
        "Int" => Some(Scalar::Int),
        "Bool" => Some(Scalar::Bool),
        "Float" => Some(Scalar::Float),
        _ => None,
    }
}

fn floor_ret(t: &TypeExpr, type_params: &BTreeSet<String>) -> Option<FloorRet> {
    match t {
        TypeExpr::Simple { name } if type_params.contains(name.as_str()) => Some(FloorRet::Elem(ElemRet::Value)),
        TypeExpr::Generic { name, args } if is_option_name(name.as_str()) && args.len() == 1 => {
            floor_ret_option(&args[0], type_params)
        }
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 && is_param_expr(&args[0], type_params) => {
            Some(FloorRet::Elem(ElemRet::List))
        }
        TypeExpr::Simple { name } => floor_ret_simple(name.as_str()),
        TypeExpr::Generic { name, args } if name.as_str() == "Result" && args.len() == 2 => floor_ret_result(&args[0], &args[1]),
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => {
            if let TypeExpr::Generic { name, args: inner } = &args[0] {
                let rows = name.as_str() == "List" && inner.len() == 1 && simple(&inner[0]) == Some("Float");
                return rows.then_some(FloorRet::FloatRows);
            }
            match simple(&args[0])? {
                "Int" => Some(FloorRet::ListInt),
                "String" => Some(FloorRet::ListStr),
                "Matrix" => Some(FloorRet::ListMatrix),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Is `e` one of the fn's type params, spelled as a simple type?
fn is_param_expr(e: &TypeExpr, type_params: &BTreeSet<String>) -> bool {
    simple(e).is_some_and(|a| type_params.contains(a))
}

/// The return kind of an Option of `arg`.
fn floor_ret_option(arg: &TypeExpr, type_params: &BTreeSet<String>) -> Option<FloorRet> {
    if is_param_expr(arg, type_params) {
        return Some(FloorRet::Elem(ElemRet::Opt));
    }
    if simple(arg) == Some("String") {
        return Some(FloorRet::OptStr);
    }
    if simple(arg) == Some("Matrix") {
        return Some(FloorRet::OptMatrix);
    }
    scalar_of(simple(arg)?).map(FloorRet::Opt)
}

/// The return kind of a simple (non-generic, non-type-param) type `name`.
fn floor_ret_simple(name: &str) -> Option<FloorRet> {
    match name {
        "Unit" => Some(FloorRet::Unit),
        "Int" => Some(FloorRet::Int),
        "Bool" => Some(FloorRet::Bool),
        "Float" => Some(FloorRet::Float),
        "String" => Some(FloorRet::Str),
        "Bytes" => Some(FloorRet::Bytes),
        "Matrix" => Some(FloorRet::Matrix),
        n if n == VALUE_TYPE => Some(FloorRet::Value),
        _ => None,
    }
}

/// The generated list kind a floor list param declares: a list of runtime
/// `Value`s, of matrices, or of `(String, Value)` pairs (the slot struct a
/// stored pair literal is typed as). `None` for the scalar and String lists,
/// which are no generated type, and for any other param.
pub(crate) fn generated_list_of(p: FloorParam) -> Option<u16> {
    use crate::render_native::NTy;
    use crate::render_native_rec as rec;
    let elem = match p {
        FloorParam::List(ListElem::Value) => NTy::Rec(rec::value_index()),
        FloorParam::List(ListElem::Matrix) => NTy::Rec(rec::matrix_index()),
        FloorParam::List(ListElem::StrValuePairs) => NTy::Rec(rec::slots_type_of(&[NTy::Str, NTy::Rec(rec::value_index())])),
        _ => return None,
    };
    Some(rec::list_type_of(elem))
}

/// The runtime body `symbol` names, for a declaration whose function-typed
/// param keeps it off this floor (`crate::native_hof`): its Rust param types,
/// its return type, and the items it reaches. `None` when the runtime has no
/// takeable item of that name or it reaches one the header cannot carry.
pub(crate) fn runtime_body(symbol: &str) -> Option<(Vec<String>, String, Vec<&'static str>)> {
    let rt = runtime_index();
    let (params, ret) = rust_signature(rt.items.get(symbol)?.text)?;
    Some((params.iter().map(|p| squash(p)).collect(), squash(&ret), closure(symbol, rt).ok()?))
}

/// The runtime item `symbol` names and every item it reaches, taken whole
/// without reading its signature (a const-generic array twin's bounds hold
/// parentheses the signature reader does not expect).
pub(crate) fn runtime_items(symbol: &str) -> Option<Vec<&'static str>> {
    closure(symbol, runtime_index()).ok()
}

/// The stdlib fn the self-hosted body registered under `call` declares it
/// specializes, if any (`@specializes`).
pub(crate) fn specializes(call: &str) -> Option<String> {
    specialized_target(call)
}

/// The element type the self-hosted body registered under `call` declares
/// for its first list param (`list.unique_float`'s `List[Float]` → `Float`).
pub(crate) fn specialized_elem(call: &str) -> Option<String> {
    specialized_list_elem(call)
}
