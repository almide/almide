const __almd_option = {
  map(o, f) { return o !== null ? f(o) : null; },
  flat_map(o, f) { return o !== null ? f(o) : null; },
  flatten(o) { return o !== null ? o : null; },
  unwrap_or(o, d) { return o !== null ? o : d; },
  unwrap_or_else(o, f) { return o !== null ? o : f(); },
  is_some(o) { return o !== null; },
  is_none(o) { return o === null; },
  to_result(o, err) { if (o !== null) return o; throw new Error(err); },
  filter(o, f) { return o !== null && f(o) ? o : null; },
  zip(a, b) { return a !== null && b !== null ? [a, b] : null; },
  or_else(o, f) { return o !== null ? o : f(); },
  to_list(o) { return o !== null ? [o] : []; },
};
