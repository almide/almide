const __almd_error = {
  chain(outer, cause) { return outer + ": " + cause; },
  message(_r) { return ""; },
};
