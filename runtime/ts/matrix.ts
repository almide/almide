const __almd_matrix = {
  zeros(rows: number, cols: number): number[][] { return Array.from({length: rows}, () => new Array(cols).fill(0)); },
  ones(rows: number, cols: number): number[][] { return Array.from({length: rows}, () => new Array(cols).fill(1)); },
  shape(m: number[][]): [number, number] { return [m.length, m.length > 0 ? m[0].length : 0]; },
  rows(m: number[][]): number { return m.length; },
  cols(m: number[][]): number { return m.length > 0 ? m[0].length : 0; },
  get(m: number[][], row: number, col: number): number { return m[row][col]; },
  transpose(m: number[][]): number[][] {
    if (m.length === 0) return [];
    return m[0].map((_, c) => m.map(row => row[c]));
  },
  from_lists(rows: number[][]): number[][] { return rows.map(r => [...r]); },
  to_lists(m: number[][]): number[][] { return m.map(r => [...r]); },
  add(a: number[][], b: number[][]): number[][] { return a.map((row, i) => row.map((v, j) => v + b[i][j])); },
  mul(a: number[][], b: number[][]): number[][] {
    const rows = a.length, cols = b[0]?.length ?? 0, inner = a[0]?.length ?? 0;
    return Array.from({length: rows}, (_, i) =>
      Array.from({length: cols}, (_, j) => {
        let sum = 0; for (let k = 0; k < inner; k++) sum += a[i][k] * b[k][j]; return sum;
      }));
  },
  scale(m: number[][], s: number): number[][] { return m.map(row => row.map(v => v * s)); },
  map(m: number[][], f: (x: number) => number): number[][] { return m.map(row => row.map(f)); },
};
