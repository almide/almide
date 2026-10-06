// The order `register_decls` registers a module's `type` declarations in
// (#3407). `include!`d by registration.rs; it shares that module's scope.
//
// A transparent alias is resolved ONCE, when it registers: `type A = B`
// resolves `B` against the table and stores the result. Registered in source
// order, `type A = B` above `type B = (Int, Int)` found no `B`, kept the name
// `B`, and `A` became an opaque name a tuple index then refused (E045).
// Declaration order is not meaningful in Almide, so every declaration that
// spells an alias of the same module is registered AFTER that alias: the
// aliases form a dependency graph, and registration walks it depth-first from
// each declaration in source order (Tarjan's strongly connected components,
// which come out dependencies-first). A declaration nothing depends on keeps
// its source position, protocols included.
//
// Records and variants are nominal: a reference to one stays its name and is
// expanded on demand, so nothing has to register before them and a record
// may refer to itself. Only an ALIAS can be in a cycle — `type A = B` /
// `type B = A`, or `type A = List[A]` through a generic argument — and such
// an alias has no body to stand for. Its members are registered as
// `Unknown`, and the checker reports each cycle once (E094, through
// `alias_cycle_diags`, with the file it is checking), so nothing downstream expands a
// name that leads back to itself (an `Unknown` unifies with everything, so a
// use of the alias reports at most what an unannotated value would).

/// One step of a module's type and protocol registration.
enum DeclStep<'d> {
    /// A protocol, at its source position.
    Protocol(&'d ast::Decl),
    /// A `type` declaration; `cyclic` when it is an alias in a cycle.
    Type { decl: &'d ast::Decl, cyclic: bool },
    /// A cycle of aliases, reported after its members registered: the
    /// members in cycle order, starting at the first one declared.
    Cycle(Vec<&'d ast::Decl>),
}

/// Whether a `type` declaration's body is an alias (resolved when it
/// registers) rather than a nominal record or variant.
fn is_alias_body(ty: &ast::TypeExpr) -> bool {
    !matches!(ty, ast::TypeExpr::Record { .. } | ast::TypeExpr::Variant { .. })
}

/// Every bare type name `te` spells, outside the letters in `bound`.
fn spelled_type_names(te: &ast::TypeExpr, bound: &[Sym], out: &mut Vec<Sym>) {
    let mut push = |n: Sym| {
        if !n.as_str().contains('.') && !bound.contains(&n) {
            out.push(n);
        }
    };
    match te {
        ast::TypeExpr::Simple { name } => push(*name),
        ast::TypeExpr::Generic { name, args } => {
            push(*name);
            for a in args { spelled_type_names(a, bound, out); }
        }
        ast::TypeExpr::Record { fields } | ast::TypeExpr::OpenRecord { fields } => {
            for f in fields { spelled_type_names(&f.ty, bound, out); }
        }
        ast::TypeExpr::Fn { params, ret, .. } => {
            for p in params { spelled_type_names(p, bound, out); }
            spelled_type_names(ret, bound, out);
        }
        ast::TypeExpr::Tuple { elements: ts } | ast::TypeExpr::Union { members: ts } => {
            for t in ts { spelled_type_names(t, bound, out); }
        }
        ast::TypeExpr::Variant { cases, .. } => {
            for c in cases {
                match c {
                    ast::VariantCase::Unit { .. } => {}
                    ast::VariantCase::Tuple { fields, .. } => {
                        for t in fields { spelled_type_names(t, bound, out); }
                    }
                    ast::VariantCase::Record { fields, .. } => {
                        for f in fields { spelled_type_names(&f.ty, bound, out); }
                    }
                }
            }
        }
        ast::TypeExpr::ConstLit { .. } => {}
    }
}

/// The aliases of `decls` a `type` declaration's body (and its conformance
/// arguments) spells — the declarations that must register before it.
fn alias_deps(decl: &ast::Decl, aliases: &HashMap<Sym, Vec<usize>>) -> Vec<usize> {
    let ast::Decl::Type { ty, generics, deriving_refs, .. } = decl else { return Vec::new() };
    let bound: Vec<Sym> = generics.iter().flatten().map(|g| g.name).collect();
    let mut names = Vec::new();
    spelled_type_names(ty, &bound, &mut names);
    for r in deriving_refs.iter().flatten() {
        for a in &r.args { spelled_type_names(a, &bound, &mut names); }
    }
    let mut deps: Vec<usize> = names.iter().filter_map(|n| aliases.get(n)).flatten().copied().collect();
    deps.sort_unstable();
    deps.dedup();
    deps
}

struct Tarjan<'d> {
    decls: &'d [ast::Decl],
    deps: Vec<Vec<usize>>,
    index: Vec<Option<usize>>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next: usize,
    steps: Vec<DeclStep<'d>>,
}

impl<'d> Tarjan<'d> {
    fn visit(&mut self, v: usize) {
        self.index[v] = Some(self.next);
        self.low[v] = self.next;
        self.next += 1;
        self.stack.push(v);
        self.on_stack[v] = true;
        for w in self.deps[v].clone() {
            match self.index[w] {
                None => {
                    self.visit(w);
                    self.low[v] = self.low[v].min(self.low[w]);
                }
                Some(iw) if self.on_stack[w] => self.low[v] = self.low[v].min(iw),
                Some(_) => {}
            }
        }
        if Some(self.low[v]) != self.index[v] {
            return;
        }
        let mut scc = Vec::new();
        while let Some(w) = self.stack.pop() {
            self.on_stack[w] = false;
            scc.push(w);
            if w == v { break; }
        }
        scc.sort_unstable();
        let cyclic = scc.len() > 1 || self.deps[v].contains(&v);
        for &m in &scc {
            self.steps.push(DeclStep::Type { decl: &self.decls[m], cyclic });
        }
        if cyclic {
            let path = self.cycle_path(&scc);
            self.steps.push(DeclStep::Cycle(path.into_iter().map(|i| &self.decls[i]).collect()));
        }
    }

    /// A path through the component from its first-declared member back to
    /// it: `[A, B]` for `A → B → A`.
    fn cycle_path(&self, scc: &[usize]) -> Vec<usize> {
        let start = scc[0];
        let mut prev: HashMap<usize, usize> = HashMap::new();
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(u) = queue.pop_front() {
            for &w in &self.deps[u] {
                if w == start {
                    let mut path = vec![u];
                    let mut at = u;
                    while at != start {
                        at = prev[&at];
                        path.push(at);
                    }
                    path.reverse();
                    return path;
                }
                if scc.contains(&w) && !prev.contains_key(&w) {
                    prev.insert(w, u);
                    queue.push_back(w);
                }
            }
        }
        scc.to_vec()
    }
}

/// The type and protocol registration steps of `decls`, in the order they
/// must run (see the file header).
fn type_registration_steps(decls: &[ast::Decl]) -> Vec<DeclStep<'_>> {
    let mut aliases: HashMap<Sym, Vec<usize>> = HashMap::new();
    for (i, d) in decls.iter().enumerate() {
        if let ast::Decl::Type { name, ty, .. } = d
            && is_alias_body(ty)
        {
            aliases.entry(*name).or_default().push(i);
        }
    }
    let n = decls.len();
    let mut t = Tarjan {
        decls,
        deps: decls.iter().map(|d| alias_deps(d, &aliases)).collect(),
        index: vec![None; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        next: 0,
        steps: Vec::new(),
    };
    for (i, d) in decls.iter().enumerate() {
        match d {
            ast::Decl::Type { .. } if t.index[i].is_none() => t.visit(i),
            ast::Decl::Protocol { .. } => t.steps.push(DeclStep::Protocol(d)),
            _ => {}
        }
    }
    t.steps
}

/// Register an alias in a cycle as `Unknown` under every key its
/// registration wrote, so no later expansion follows it back to itself.
fn register_cyclic_alias_unknown(env: &mut TypeEnv, name: &str, prefix: Option<&str>) {
    let owner = type_decl_prefix(env, prefix, name);
    let key = sym(&prefixed_key(owner.as_deref(), name));
    let Some(written) = env.types.get(&key).cloned() else { return };
    env.types.insert(key, Ty::Unknown);
    let bare = sym(name);
    if bare != key && env.types.get(&bare) == Some(&written) {
        env.types.insert(bare, Ty::Unknown);
    }
}

/// E094 for every alias cycle among `decls` — one file's declarations — at
/// the first alias declared in it, with that alias's name. Pure over the
/// AST, so the checker reports the cycles of the file it is checking, with
/// that file's name; the registration that turned the members into
/// `Unknown` cannot name a file.
pub fn alias_cycle_diags(decls: &[ast::Decl]) -> Vec<(Sym, Diagnostic)> {
    type_registration_steps(decls).into_iter().filter_map(|step| match step {
        DeclStep::Cycle(cycle) => match cycle.first() {
            Some(ast::Decl::Type { name, .. }) => Some((*name, alias_cycle_diag(&cycle))),
            _ => None,
        },
        _ => None,
    }).collect()
}

/// E094: the aliases of `cycle` (in cycle order) refer to themselves.
fn alias_cycle_diag(cycle: &[&ast::Decl]) -> Diagnostic {
    let names: Vec<Sym> = cycle.iter().filter_map(|d| match d {
        ast::Decl::Type { name, .. } => Some(*name),
        _ => None,
    }).collect();
    let first = names[0];
    let path = names.iter().chain(std::iter::once(&first)).map(|n| n.as_str()).collect::<Vec<_>>().join(" → ");
    let msg = if names.len() == 1 {
        format!("type alias '{}' refers to itself ({})", first, path)
    } else {
        format!("type aliases form a cycle: {}", path)
    };
    let mut diag = err(
        msg,
        format!(
            "An alias is another name for its body, so an alias that leads back to itself names nothing. \
             Give the cycle a nominal type — a record or variant may refer to itself: `type {} = {{ inner: ... }}`",
            first
        ),
        format!("type {}", first),
    ).with_code("E094");
    if let Some(ast::Decl::Type { span: Some(sp), .. }) = cycle.first() {
        diag.line = Some(sp.line);
        diag.col = Some(sp.col);
    }
    diag
}
