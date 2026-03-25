export type ConfigPair = [string, string];

export function parseConfig(content: string): ConfigPair[] {
  // TODO: implement
  // Throw new Error("line N: missing '='") etc. on error
  throw new Error("not implemented");
}

export function mergeConfigs(
  base: ConfigPair[],
  overlay: ConfigPair[],
): ConfigPair[] {
  // TODO: implement
  return [];
}

export function serializeConfig(pairs: ConfigPair[]): string {
  // TODO: implement
  return "";
}

export function lookup(
  pairs: ConfigPair[],
  key: string,
): string | undefined {
  // TODO: implement
  return undefined;
}

export function filterByPrefix(
  pairs: ConfigPair[],
  prefix: string,
): ConfigPair[] {
  // TODO: implement
  return [];
}
