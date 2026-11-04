// `x % <literal>` on the certified native leg (#4006): the standard codegen
// spells it `almide_rem_const::<D>(x)` (prelude_rem.rs: the magic-number
// remainder, faster on a remainder chain than rustc's own lowering),
// and the certified render must not be slower than the route it replaces.
// The divisor is a literal when its value is a `ConstInt` defined once and
// never rebound by a `SetLocal` (a loop-carried `var` starts as a
// `ConstInt` too) — read from the function's ops, not from a name. The
// divisors that qualify (`>= 3`, not a power of two) can be neither 0 nor
// -1, so the checked shim's two aborts are unreachable and dropping them
// changes no observable byte.

thread_local! {
    static REM_DIVISORS: std::cell::RefCell<BTreeMap<ValueId, i64>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// The values of `func` that hold one literal divisor for its whole life.
pub(crate) fn literal_divisors(func: &MirFunction) -> BTreeMap<ValueId, i64> {
    let mut defs: BTreeMap<ValueId, (i64, usize)> = BTreeMap::new();
    let mut rebound = std::collections::BTreeSet::new();
    for op in &func.ops {
        match op {
            Op::ConstInt { dst, value } => defs.entry(*dst).or_insert((*value, 0)).1 += 1,
            Op::SetLocal { local, .. } => {
                rebound.insert(*local);
            }
            _ => {}
        }
    }
    defs.into_iter()
        .filter(|(v, (d, n))| *n == 1 && !rebound.contains(v) && almide_codegen::rem_const_divisor(*d))
        .map(|(v, (d, _))| (v, d))
        .collect()
}

/// Holds `func`'s literal divisors for the length of its render, and puts
/// back the enclosing function's on drop (a nested render cannot leak its
/// ids into the outer one).
pub(crate) struct RemDivisorScope(Option<BTreeMap<ValueId, i64>>);

pub(crate) fn rem_divisor_scope(func: &MirFunction) -> RemDivisorScope {
    let mine = literal_divisors(func);
    RemDivisorScope(Some(REM_DIVISORS.with(|m| std::mem::replace(&mut *m.borrow_mut(), mine))))
}

impl Drop for RemDivisorScope {
    fn drop(&mut self) {
        if let Some(prev) = self.0.take() {
            REM_DIVISORS.with(|m| *m.borrow_mut() = prev);
        }
    }
}

pub(crate) fn literal_rem_divisor(b: ValueId) -> Option<i64> {
    REM_DIVISORS.with(|m| m.borrow().get(&b).copied())
}

pub(crate) fn rem_const_shim() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(almide_codegen::rem_const_prelude_text).as_str()
}

#[cfg(test)]
mod rem_const_tests {
    fn render(src: &str) -> String {
        crate::pipeline::try_render_rust_source_certified(src).expect("renders on the certified leg").0
    }

    #[test]
    fn a_literal_divisor_takes_the_constant_remainder() {
        let rust = render("fn f(n: Int) -> Int = n % 1000003\nfn main() -> Unit = println(int.to_string(f(12345678901)))\n");
        assert!(rust.contains("almide_rem_const::<1000003>("), "{rust}");
        assert!(rust.contains("fn almide_rem_const<"), "the runtime text rides along");
    }

    #[test]
    fn a_power_of_two_or_a_rebound_divisor_keeps_the_checked_shim() {
        let rust = render(
            "fn f(n: Int) -> Int = n % 8\nfn g(n: Int) -> Int = {\n  var d = 1000003\n  var acc = n\n  var i = 0\n  while i < 3 {\n    acc = acc % d\n    d = d - 1000003\n    i = i + 1\n  }\n  acc\n}\nfn main() -> Unit = {\n  println(int.to_string(f(13)))\n  println(int.to_string(g(13)))\n}\n",
        );
        assert!(!rust.contains("almide_rem_const::<"), "{rust}");
        assert!(rust.contains("rt_chk_mod("), "{rust}");
    }
}
