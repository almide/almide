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

    /// A map/filter → fold pipeline whose callbacks are sometimes PURE
    /// (the fusion path) and sometimes PRINTING (the refusal path, where
    /// the oracle's stage order — all maps, then all filters, then the
    /// fold — must survive verbatim). Pins stage 58's soundness boundary
    /// from both sides.
    fn stmt_fuse_pipeline(&mut self, depth: usize) {
        let name = self.fresh();
        let src = self.expr(Ty::ListInt, depth.saturating_sub(1));
        let x = self.fresh();
        let map_body = if self.rng.chance(30) {
            // impure: the map callback prints each element — MUST refuse
            // fusion and keep all-maps-first ordering.
            format!("{{\n    println(\"m${{{x}}}\")\n    {x} * 3 + 1\n  }}")
        } else {
            format!("{x} * 3 + 1")
        };
        let f = self.fresh();
        let filter_body = if self.rng.chance(20) {
            format!("{{\n    println(\"f${{{f}}}\")\n    {f} % 2 == 0\n  }}")
        } else {
            format!("{f} % 2 == 0")
        };
        let (a, v) = (self.fresh(), self.fresh());
        self.line(&format!(
            "let {name} = {src} |> list.map(({x}) => {map_body}) |> list.filter(({f}) => {filter_body}) |> list.fold(0, ({a}, {v}) => ({a} + {v}) % 999983)"
        ));
        self.vars.push(Var { name, ty: Ty::Int, mutable: false });
    }

    /// Constant-divisor division/remainder over the EDGE domain (the
    /// strength-reduction net, written BEFORE the optimization): known
    /// literal divisors both signs, dividends spanning 0/±1/±MAX/MIN,
    /// every result printed so the mul-shift path must agree with
    /// i64.div_s/rem_s bit-for-bit, truncation toward zero included.
    fn stmt_div_edges(&mut self) {
        const DIVS: &[i64] = &[2, 3, 4, 5, 7, 8, 10, 16, 100, 999983, -2, -3, -7, -8, -100];
        let xs = [
            "0",
            "1",
            "-1",
            "9223372036854775807",
            "-9223372036854775807",
            "(-9223372036854775807 - 1)",
            "123456789",
            "-987654321",
        ];
        let d = DIVS[self.rng.below(DIVS.len())];
        let x = xs[self.rng.below(xs.len())];
        self.line(&format!("println(int.to_string({x} / {d}) + \"|\" + int.to_string({x} % {d}))"));
    }

    /// A deterministic-budget region whose loop length and declared
    /// nanoseconds are RANDOM, so the fixed seed range lands on both
    /// sides of the exhaustion boundary — the two legs must agree on
    /// every verdict, EIP-150 nesting included (ALS-DT2).
    fn stmt_fuel_region(&mut self) {
        self.needs_effect = true;
        let name = self.fresh();
        let k = self.rng.below(9);
        let ns = self.rng.below(3 * (k + 6));
        // BOTH cut placements (C-320: placement is unobservable) — the
        // callee-loop shape and the direct-in-arm shape, which exercises
        // the cut's exit-bookkeeping repair on every leg.
        if self.rng.chance(50) {
            self.needs_fuel_helper = true;
            self.line(&format!(
                "let {name} = fan.bounded(compute.ns({ns})) {{ fz_loop({k}) }} ?? -1"
            ));
        } else {
            let (s, i) = (self.fresh(), self.fresh());
            self.line(&format!(
                "let {name} = fan.bounded(compute.ns({ns})) {{\n  var {s} = 0\n  for {i} in 0..<{k} {{\n    {s} = {s} + {i}\n  }}\n  {s}\n}} ?? -1"
            ));
        }
        self.line(&format!("println(\"${{{name}}}\")"));
        self.vars.push(Var { name, ty: Ty::Int, mutable: false });
    }


    /// An fs ROUND TRIP in a temp dir — HOST-ORACLE MODE ONLY (the
    /// reference interpreter abstains on host fs; the released native
    /// binary referees instead). Every printed observable is path-free
    /// and the content is seed-derived.
    fn stmt_fs_roundtrip(&mut self) {
        self.needs_effect = true;
        self.needs_fs = true;
        let d = self.fresh();
        let (a, b) = (self.fresh(), self.fresh());
        let payload = format!("l{}\\nl{}\\nx{}", self.rng.below(100), self.rng.below(100), self.rng.below(1000));
        self.line(&format!("let {d} = fs.create_temp_dir(\"gfz\")!"));
        self.line(&format!("let _w{a} = fs.write({d} + \"/a.txt\", \"{payload}\")!"));
        self.line(&format!("let {a} = fs.read_text({d} + \"/a.txt\")!"));
        self.line(&format!("println(\"len=\" + int.to_string(string.len({a})))"));
        self.line(&format!("let {b} = fs.read_lines({d} + \"/a.txt\")!"));
        self.line(&format!("println(\"lines=\" + int.to_string(list.len({b})))"));
        self.line(&format!(
            "println(\"probe=\" + (if fs.exists({d} + \"/a.txt\") then \"y\" else \"n\") + (if fs.exists({d} + \"/nope\") then \"y\" else \"n\"))"
        ));
        self.line(&format!("let _r{b} = fs.remove_all({d})!"));
        self.line(&format!(
            "println(\"gone=\" + (if fs.exists({d}) then \"n\" else \"y\"))"
        ));
    }

    fn expr_list(&mut self, depth: usize) -> String {
            match self.rng.below(8) {
                6 => {
                    // take/drop with counts spanning 0 / in-range /
                    // past-len / NEGATIVE (take: whole list, drop: empty —
                    // the v0 asymmetry; the stage-63 take inversion hid
                    // because no generated program observed take's value).
                    let src = self.expr(Ty::ListInt, depth - 1);
                    let f = ["take", "drop"][self.rng.below(2)];
                    let n = [0i64, 1, 2, 9, -1][self.rng.below(5)];
                    format!("list.{f}({src}, {n})")
                }
                7 => {
                    let src = self.expr(Ty::ListInt, depth - 1);
                    let i = [0i64, 1, 5, -1][self.rng.below(4)];
                    format!("list.insert({src}, {i}, {})", self.expr(Ty::Int, depth - 1))
                }
                5 => {
                    // Slice — start is OFTEN 0 (the everyday form): mutant
                    // 010's survival exposed that no exercised program
                    // sliced from zero.
                    let src = self.expr(Ty::ListInt, depth - 1);
                    let a = self.rng.below(2);
                    let b = a + self.rng.below(4);
                    format!("list.slice({src}, {a}, {b})")
                }
                0 => {
                    let n = self.rng.below(4);
                    let items: Vec<String> = (0..n).map(|_| self.expr(Ty::Int, depth - 1)).collect();
                    format!("[{}]", items.join(", "))
                }
                1 => format!("({} + {})", self.expr(Ty::ListInt, depth - 1), self.expr(Ty::ListInt, depth - 1)),
                // HOF callbacks — inlined lambdas over the new machinery.
                2 => {
                    let p = self.fresh();
                    let src = self.expr(Ty::ListInt, depth - 1);
                    self.vars.push(Var { name: p.clone(), ty: Ty::Int, mutable: false });
                    let body = self.expr(Ty::Int, depth - 1);
                    self.vars.pop();
                    format!("list.map({src}, ({p}) => {body})")
                }
                3 => {
                    let p = self.fresh();
                    let src = self.expr(Ty::ListInt, depth - 1);
                    self.vars.push(Var { name: p.clone(), ty: Ty::Int, mutable: false });
                    let cond = self.expr(Ty::Bool, depth - 1);
                    self.vars.pop();
                    format!("list.filter({src}, ({p}) => {cond})")
                }
                _ => self.leaf(Ty::ListInt),
            }
    }
}
