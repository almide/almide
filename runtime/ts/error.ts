const __almd_error = {
  chain(outer: string, cause: string): string { return outer + ": " + cause; },
  message(_r: any): string { return ""; },
};
