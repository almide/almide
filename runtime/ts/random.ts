const __almd_random = {
  int(min: number, max: number): number { return Math.floor(Math.random() * (max - min + 1)) + min; },
  float(): number { return Math.random(); },
  choice<T>(xs: T[]): T | null { return xs.length > 0 ? xs[Math.floor(Math.random() * xs.length)] : null; },
  shuffle<T>(xs: T[]): T[] { const a = [...xs]; for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; },
};
