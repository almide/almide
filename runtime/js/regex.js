const __almd_regex = {
  is_match(pat, s) { return new RegExp(pat).test(s); },
  full_match(pat, s) { return new RegExp(`^(?:${pat})$`).test(s); },
  find(pat, s) { const m = s.match(new RegExp(pat)); return m ? m[0] : null; },
  find_all(pat, s) { const m = s.match(new RegExp(pat, 'g')); return m ? [...m] : []; },
  replace(pat, s, rep) { return s.replace(new RegExp(pat, 'g'), rep); },
  replace_first(pat, s, rep) { return s.replace(new RegExp(pat), rep); },
  split(pat, s) { return s.split(new RegExp(pat)); },
  captures(pat, s) { const m = s.match(new RegExp(pat)); return m && m.length > 1 ? m.slice(1) : null; },
};
