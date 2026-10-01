// #3176: two modules declare the same case names in a different order. The
// MIR rung resolved a constructor by its bare name, so `a.Halt` and
// `a.Turn(5)` were built with `b.Cmd`'s tags while the function still lowered
// in-profile: a silent wrong pick, certified as is. Every block must carry
// the tag of the variant the value's type names.
#[cfg(test)]
mod ctor_resolution_tests {
    use super::*;

    const A: &str = "type Sig = | Go(Int) | Halt | Turn(Int)\n";
    const B: &str = "type Cmd = | Halt | Turn(Int) | Go(Int)\n";
    const MAIN: &str = r#"
import self.a
import self.b

fn ab() -> Int = {
  let x = a.Halt
  let y = b.Go(7)
  let z = a.Turn(5)
  let w = b.Turn(6)
  let p = match y {
    b.Go(n) => n,
    _ => 0,
  }
  let q = match z {
    a.Turn(n) => n,
    _ => 0,
  }
  p + q
}

effect fn main() -> Unit = println(int.to_string(ab()))
"#;

    /// The tag slot of every two-slot block `ab` builds, in construction order.
    fn constructed_tags() -> Vec<i64> {
        let modules = vec![
            ("a".to_string(), parse_or_wall(A).expect("a parses"), true),
            ("b".to_string(), parse_or_wall(B).expect("b parses"), true),
        ];
        let ir = source_to_ir_with(MAIN, &modules).expect("program lowers to IR");
        let (globals, global_inits) = witness_globals(&ir);
        let (records, variants) = witness_layouts(&ir);
        let func = ir.functions.iter().find(|f| f.name.as_str() == "ab").expect("ab");
        let mirs = crate::lower::lower_function_all_with_globals(func, &globals, &global_inits, &records, &variants)
            .expect("ab lowers in-profile");
        let ops = &mirs[0].ops;
        let consts: std::collections::HashMap<u32, i64> = ops
            .iter()
            .filter_map(|op| match op {
                crate::Op::ConstInt { dst, value } => Some((dst.0, *value)),
                _ => None,
            })
            .collect();
        ops.iter()
            .filter_map(|op| match op {
                crate::Op::ListLit { elems, .. } if elems.len() == 2 => consts.get(&elems[0].0).copied(),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_case_shared_with_another_module_takes_its_own_variants_tag() {
        // a.Halt = Sig tag 1, b.Go = Cmd tag 2, a.Turn = Sig tag 2, b.Turn = Cmd tag 1.
        assert_eq!(constructed_tags(), vec![1, 2, 2, 1]);
    }
}
