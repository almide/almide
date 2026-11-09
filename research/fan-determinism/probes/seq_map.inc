
// The sequential reading of fan.map: every element runs in list order, the
// lowest-index Err is the result.
fn seq_map(xs: List[Int], f: (Int) -> Result[Int, String]) -> Result[List[Int], String] = {
  let rs = list.map(xs, f)
  let errs = list.filter_map(rs, (r) => match r {
    ok(_) => none,
    err(e) => some(e),
  })
  match list.first(errs) {
    some(e) => err(e),
    none => ok(list.filter_map(rs, (r) => match r {
      ok(v) => some(v),
      err(_) => none,
    })),
  }
}
