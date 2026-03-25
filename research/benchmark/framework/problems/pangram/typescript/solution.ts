export function isPangram(sentence: string): boolean {
  const lower = sentence.toLowerCase();
  return "abcdefghijklmnopqrstuvwxyz"
    .split("")
    .every((c) => lower.includes(c));
}
