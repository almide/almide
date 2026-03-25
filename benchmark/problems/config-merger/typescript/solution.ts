export type ConfigPair = [string, string];

export function parseConfig(content: string): ConfigPair[] {
  if (!content) return [];

  const pairs: ConfigPair[] = [];
  const seenKeys = new Set<string>();

  for (const [i, line] of content.split("\n").entries()) {
    const lineNum = i + 1;
    const stripped = line.trim();
    if (!stripped || stripped.startsWith("#")) continue;
    if (!stripped.includes("=")) {
      throw new Error(`line ${lineNum}: missing '='`);
    }
    const eqIdx = stripped.indexOf("=");
    const key = stripped.slice(0, eqIdx);
    const value = stripped.slice(eqIdx + 1);
    if (!key) throw new Error(`line ${lineNum}: empty key`);
    if (seenKeys.has(key)) {
      throw new Error(`line ${lineNum}: duplicate key: ${key}`);
    }
    seenKeys.add(key);
    pairs.push([key, value]);
  }

  return pairs;
}

export function mergeConfigs(
  base: ConfigPair[],
  overlay: ConfigPair[],
): ConfigPair[] {
  const overlayMap = new Map(overlay);
  const baseKeys = new Set<string>();
  const result: ConfigPair[] = [];

  for (const [key, value] of base) {
    baseKeys.add(key);
    result.push([key, overlayMap.get(key) ?? value]);
  }

  for (const [key, value] of overlay) {
    if (!baseKeys.has(key)) result.push([key, value]);
  }

  return result;
}

export function serializeConfig(pairs: ConfigPair[]): string {
  return pairs.map(([k, v]) => `${k}=${v}`).join("\n");
}

export function lookup(
  pairs: ConfigPair[],
  key: string,
): string | undefined {
  for (const [k, v] of pairs) {
    if (k === key) return v;
  }
  return undefined;
}

export function filterByPrefix(
  pairs: ConfigPair[],
  prefix: string,
): ConfigPair[] {
  return pairs.filter(([k]) => k.startsWith(prefix));
}
