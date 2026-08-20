// Generator statement arms included into fuzz_differential's Gen impl
// (split for the file-size discipline; `include!` splices ITEM-position
// methods, so this file holds bare fns inside the enclosing impl).

impl Gen {
    fn stmt_map_from_list(&mut self, depth: usize) {
        let name = self.fresh();
        let n = 1 + self.rng.below(3);
        let pairs: Vec<String> = (0..n)
            .map(|_| {
                format!(
                    "({}, {})",
                    self.expr(Ty::Int, depth.saturating_sub(1)),
                    self.expr(Ty::Str, depth.saturating_sub(1))
                )
            })
            .collect();
        self.line(&format!(
            "let {name}: Map[Int, String] = map.from_list([{}])",
            pairs.join(", ")
        ));
        self.vars.push(Var { name, ty: Ty::MapIS, mutable: false });
    }
    fn stmt_push(&mut self, depth: usize) {
        let lists: Vec<String> = self
            .vars
            .iter()
            .filter(|v| v.mutable && v.ty == Ty::ListInt)
            .map(|v| v.name.clone())
            .collect();
        if lists.is_empty() {
            return self.stmt_println(depth);
        }
        let name = lists[self.rng.below(lists.len())].clone();
        let v = self.expr(Ty::Int, depth.saturating_sub(1));
        self.line(&format!("list.push({name}, {v})"));
        // Observe the mutation so pushes aren't dead code.
        self.line(&format!("println(int.to_string(list.len({name})))"));
    }
    /// A BOUND range (C-238): sometimes head-only (the counting-path
    /// deferral), sometimes measured right here — and the var is also
    /// registered as a plain ListInt, so later statements may index or
    /// iterate it, exercising the analysis' disqualifying shapes against
    /// the interp's materializing answer.
    fn stmt_range_bind(&mut self) {
        let name = self.fresh();
        let lo = self.rng.below(5) as i64 - 1;
        let hi = lo + self.rng.below(6) as i64 - 1;
        let op = if self.rng.below(2) == 0 { "..<" } else { "..." };
        self.line(&format!("let {name} = {lo}{op}{hi}"));
        for _ in 0..=self.rng.below(2) {
            let iv = self.fresh();
            self.line(&format!("for {iv} in {name} {{"));
            self.indent += 1;
            self.line(&format!("println(\"${{{iv}}}\")"));
            self.indent -= 1;
            self.line("}");
        }
        if self.rng.below(2) == 0 {
            self.line(&format!("println(int.to_string(list.len({name})))"));
        }
        self.vars.push(Var { name, ty: Ty::ListInt, mutable: false });
    }
    fn stmt_enumerate_loop(&mut self, depth: usize) {
        let (i, x) = (self.fresh(), self.fresh());
        let src = self.expr(Ty::ListInt, depth);
        self.line(&format!("for ({i}, {x}) in list.enumerate({src}) {{"));
        self.indent += 1;
        self.vars.push(Var { name: i.clone(), ty: Ty::Int, mutable: false });
        self.vars.push(Var { name: x.clone(), ty: Ty::Int, mutable: false });
        self.line(&format!("println(\"${{{i}}}:${{{x}}}\")"));
        self.vars.pop();
        self.vars.pop();
        self.indent -= 1;
        self.line("}");
    }
    fn stmt_tuple_destructure(&mut self, depth: usize) {
        let (a, b) = (self.fresh(), self.fresh());
        let x = self.expr(Ty::Int, depth);
        let y = self.expr(Ty::Str, depth.saturating_sub(1));
        self.line(&format!("let ({a}, {b}) = ({x}, {y})"));
        self.vars.push(Var { name: a, ty: Ty::Int, mutable: false });
        self.vars.push(Var { name: b, ty: Ty::Str, mutable: false });
    }
}
