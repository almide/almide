function __almd_result_unwrap_or<A>(v: A, _d: A): A { return v; }
function __almd_result_unwrap_or_else<A>(v: A, _f: (e: any) => A): A { return v; }
function __almd_result_is_ok(_v: any): boolean { return true; }
function __almd_result_is_err(_v: any): boolean { return false; }
function __almd_result_to_option<A>(v: A): A | null { return v; }
function __almd_result_to_err_option(_v: any): any { return null; }
function __almd_result_collect<T>(rs: T[]): T[] { return rs; }
function __almd_result_partition<T>(rs: T[]): [T[], never[]] { return [rs, []]; }
function __almd_result_collect_map<T, U>(xs: T[], f: (x: T) => U): U[] { return xs.map(f); }
const __almd_result = {
  collect<T>(rs: T[]): T[] { return rs; },
  partition<T>(rs: T[]): [T[], never[]] { return [rs, []]; },
  collect_map<T, U>(xs: T[], f: (x: T) => U): U[] { return xs.map(f); },
};
