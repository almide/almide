// E114 — a generic fn that reaches itself at a growing type (#4287).
// `include!`d by check/mod.rs; it shares that module's scope and imports.
//
// Every generic fn is compiled once per type it is called at. A fn that calls
// itself — directly, or through other generic fns — with a type built from
// its own type parameter (`depth((x, x), n - 1)` inside `depth[T]`, which
// asks for `depth[(T, T)]`) needs a new instance for every level, without
// end. Monomorphization made one instance and kept the ill-typed self-call
// in it, so the first error was the wasm wall (E082) or rustc (E0308).
//
// The rule is the expanding-cycle check: build a graph whose nodes are
// (generic fn, type parameter) and whose edges are the bindings each call
// inside a generic fn's body makes — `caller.T -> callee.U` when `U`'s
// binding mentions `T`, GROWING when the binding is more than `T` itself.
// A growing edge on a cycle is an unbounded chain of instances. Cycles of
// plain edges (recursion at the same type, a swap of two parameters) make
// finitely many instances and pass.
//
// Judged post-solve over the resolved generic calls (`generic_calls`, #2496).

impl Checker {
    /// Report every call that sits on a growing cycle of generic
    /// instantiations. `start` is the first entry of `generic_calls` this
    /// program added: earlier programs' calls take part in the graph, but
    /// were already judged in their own pass.
    fn validate_polymorphic_recursion(&mut self, start: usize) {
        type Node = (Sym, Sym);
        // (from, to, growing, call index)
        let mut edges: Vec<(Node, Node, bool, usize)> = Vec::new();
        for (i, call) in self.generic_calls.iter().enumerate() {
            let Some(caller) = call.caller else { continue };
            for (param, ty) in &call.bindings {
                let mut seen: Vec<Sym> = Vec::new();
                collect_rigid_vars(ty, &mut seen);
                for tv in seen {
                    let growing = !matches!(ty, Ty::TypeVar(n) if *n == tv);
                    edges.push(((caller, tv), (call.callee, *param), growing, i));
                }
            }
        }
        if !edges.iter().any(|e| e.2 && e.3 >= start) {
            return;
        }
        let mut adj: std::collections::HashMap<Node, Vec<(Node, usize)>> = std::collections::HashMap::new();
        for (from, to, _, i) in &edges {
            adj.entry(*from).or_default().push((*to, *i));
        }
        let mut reported: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
        for (from, to, growing, i) in &edges {
            if !*growing || *i < start {
                continue;
            }
            // Does the callee's parameter lead back to the caller's? The
            // path's calls name the fns the cycle runs through.
            let Some(path) = find_path(&adj, *to, *from) else { continue };
            let call = &self.generic_calls[*i];
            let key = call.call_span.map(|s| (s.line, s.col)).unwrap_or((0, 0));
            if !reported.insert(key) {
                continue;
            }
            let caller = from.0;
            let bare = |s: Sym| s.as_str().rsplit('.').next().unwrap_or(s.as_str()).to_string();
            let grown = call.bindings.get(&to.1).map(|t| t.display()).unwrap_or_default();
            let mut via: Vec<String> = Vec::new();
            if call.callee != caller {
                via.push(bare(call.callee));
            }
            for j in path {
                let c = self.generic_calls[j].callee;
                if c != caller && !via.contains(&bare(c)) {
                    via.push(bare(c));
                }
            }
            let message = if via.is_empty() {
                format!(
                    "`{}` calls itself at a growing type: its `{}` is `{}` at this call",
                    bare(caller), to.1.as_str(), grown,
                )
            } else {
                format!(
                    "`{}` reaches itself through `{}` at a growing type: `{}`'s `{}` is `{}` at this call",
                    bare(caller), via.join("`, `"), bare(call.callee), to.1.as_str(), grown,
                )
            };
            let mut diag = Diagnostic::error(
                message,
                format!(
                    "A generic fn is compiled once for each type it is called at, so a recursion whose \
                     type grows with every call needs an instance for every level and never ends. Recurse at \
                     the type `{}` was called with: collect the values in a `List[{}]`, or count the \
                     levels in an `Int`.",
                    bare(caller), from.1.as_str(),
                ),
                "polymorphic recursion",
            ).with_code("E114");
            diag.file = call.site.file.clone();
            if let Some(s) = call.call_span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }
}

/// The rigid (declared) type variables `ty` mentions, each once.
fn collect_rigid_vars(ty: &Ty, out: &mut Vec<Sym>) {
    let found: std::cell::RefCell<Vec<Sym>> = std::cell::RefCell::new(Vec::new());
    ty.any_child_recursive(&|t| {
        if let Ty::TypeVar(n) = t && !n.as_str().starts_with('?') {
            found.borrow_mut().push(*n);
        }
        false
    });
    for n in found.into_inner() {
        if !out.contains(&n) {
            out.push(n);
        }
    }
}

/// A path of edges from `from` to `to` (breadth-first), as the call indexes
/// it runs through; `Some(vec![])` when `from == to`.
fn find_path(
    adj: &std::collections::HashMap<(Sym, Sym), Vec<((Sym, Sym), usize)>>,
    from: (Sym, Sym),
    to: (Sym, Sym),
) -> Option<Vec<usize>> {
    let mut prev: std::collections::HashMap<(Sym, Sym), ((Sym, Sym), usize)> = std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::from([from]);
    let mut seen: std::collections::HashSet<(Sym, Sym)> = std::collections::HashSet::from([from]);
    while let Some(n) = queue.pop_front() {
        if n == to {
            let mut path = Vec::new();
            let mut cur = n;
            while let Some((p, i)) = prev.get(&cur) {
                path.push(*i);
                cur = *p;
            }
            path.reverse();
            return Some(path);
        }
        for (m, i) in adj.get(&n).map(|v| v.as_slice()).unwrap_or(&[]) {
            if seen.insert(*m) {
                prev.insert(*m, (n, *i));
                queue.push_back(*m);
            }
        }
    }
    None
}
