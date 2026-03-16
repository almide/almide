const __almd_random = {
  int(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; },
  float() { return Math.random(); },
  choice(xs) { return xs.length > 0 ? xs[Math.floor(Math.random() * xs.length)] : null; },
  shuffle(xs) { const a = [...xs]; for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; },
};
