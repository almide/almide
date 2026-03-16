const __almd_regex = {
  is_match(pat: string, s: string): boolean { return new RegExp(pat).test(s); },
  full_match(pat: string, s: string): boolean { return new RegExp(`^(?:${pat})$`).test(s); },
  find(pat: string, s: string): string | null { const m = s.match(new RegExp(pat)); return m ? m[0] : null; },
  find_all(pat: string, s: string): string[] { const m = s.match(new RegExp(pat, 'g')); return m ? [...m] : []; },
  replace(pat: string, s: string, rep: string): string { return s.replace(new RegExp(pat, 'g'), rep); },
  replace_first(pat: string, s: string, rep: string): string { return s.replace(new RegExp(pat), rep); },
  split(pat: string, s: string): string[] { return s.split(new RegExp(pat)); },
  captures(pat: string, s: string): string[] | null { const m = s.match(new RegExp(pat)); return m && m.length > 1 ? m.slice(1) : null; },
};
