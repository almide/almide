// The in-memory filesystem (#4438): the preopens `init(source, { preopens })`
// names, served to the module's WASI p1 fs imports. Shipped only when the
// module imports one of them (#2276). Nothing here reaches the page's own
// storage or network: a preopen is the tree the page passed, and a write
// lands in that tree, readable back through `files(name)`.
//
// errno follows stock wasmtime over a host directory (the `--dir` preopen):
// ENOENT 44, ENOTDIR 54, EISDIR 31, EEXIST 20, ENOTEMPTY 55, EINVAL 28,
// EBADF 8, and EPERM 63 for a `..` that would leave the preopen. A write
// to a preopen not marked `writable` is EACCES 2, the answer for a folder
// the process may read but not write ("Permission denied").
const FS_DIR = 3, FS_FILE = 4;
let fsFds = new Map();
let fsRoots = new Map();
let fsNextFd = 3;
let fsIno = 0;
function fsDirNode() { return { type: FS_DIR, ino: ++fsIno, entries: new Map() }; }
function fsFileNode(buf, shared) { return { type: FS_FILE, ino: ++fsIno, buf, size: buf.length, shared }; }
function fsBytesOf(v, what) {
  if (typeof v === "string") return [encoder.encode(v), false];
  if (v instanceof Uint8Array) return [v, true];
  if (ArrayBuffer.isView(v)) return [new Uint8Array(v.buffer, v.byteOffset, v.byteLength), true];
  if (v instanceof ArrayBuffer) return [new Uint8Array(v), true];
  return null;
}
function fsFill(dir, tree, what) {
  const rows = tree instanceof Map ? tree.entries() : Object.entries(tree);
  for (const [key, v] of rows) {
    const segs = String(key).split("/").filter((s) => s !== "");
    if (segs.length === 0 || segs.some((s) => s === "." || s === ".." || s.includes("\0"))) {
      throw new TypeError(`almide: ${what} names an invalid path ${JSON.stringify(key)}`);
    }
    let at = dir;
    for (const s of segs.slice(0, -1)) {
      let next = at.entries.get(s);
      if (next === undefined) { next = fsDirNode(); at.entries.set(s, next); }
      if (next.type !== FS_DIR) throw new TypeError(`almide: ${what} names ${JSON.stringify(key)} under a file`);
      at = next;
    }
    const name = segs[segs.length - 1];
    const b = fsBytesOf(v, what);
    const old = at.entries.get(name);
    if (b !== null) {
      if (old !== undefined) throw new TypeError(`almide: ${what} names ${JSON.stringify(key)} twice`);
      at.entries.set(name, fsFileNode(b[0], b[1]));
    } else if (v !== null && typeof v === "object") {
      if (old !== undefined && old.type !== FS_DIR) throw new TypeError(`almide: ${what} names ${JSON.stringify(key)} as a file and a folder`);
      const sub = old ?? fsDirNode();
      at.entries.set(name, sub);
      fsFill(sub, v, what);
    } else {
      throw new TypeError(`almide: ${what}[${JSON.stringify(key)}] is neither bytes, a string nor a folder`);
    }
  }
}
// `init()`'s preopens, in the order given: the first is fd 3.
function fsReset(preopens) {
  fsFds = new Map();
  fsRoots = new Map();
  fsNextFd = 3;
  fsIno = 0;
  if (preopens == null) return;
  const rows = preopens instanceof Map ? [...preopens.entries()] : Object.entries(preopens);
  for (const [name, p] of rows) {
    const what = `init(): preopens[${JSON.stringify(name)}]`;
    if (typeof name !== "string" || name === "" || name.includes("\0")) throw new TypeError(`almide: init(): a preopen name must be a non-empty string, got ${JSON.stringify(name)}`);
    if (p === null || typeof p !== "object") throw new TypeError(`almide: ${what} takes { files?, writable? }, got ${p}`);
    const node = fsDirNode();
    if (p.files != null) fsFill(node, p.files, what + ".files");
    const root = { node, writable: p.writable === true };
    fsRoots.set(name, root);
    fsFds.set(fsNextFd++, { node, stack: [node], root, pos: 0, append: false, preopen: encoder.encode(name) });
  }
}
/** Every file under the preopen `name`, by its path inside it ("a/b.txt"), sorted; the bytes are copies. */
export function files(name) {
  ready();
  const root = fsRoots.get(name);
  if (root === undefined) throw new Error(`almide: files(): no preopen named ${JSON.stringify(name)}`);
  const out = new Map();
  const walk = (dir, prefix) => {
    for (const [n, node] of fsSorted(dir)) {
      if (node.type === FS_DIR) walk(node, prefix + n + "/");
      else out.set(prefix + n, node.buf.slice(0, node.size));
    }
  };
  walk(root.node, "");
  return out;
}
function fsCmp(a, b) {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) if (a[i] !== b[i]) return a[i] - b[i];
  return a.length - b.length;
}
// A folder's entries sorted by their UTF-8 bytes: the listing order a page
// can predict whatever order it passed the tree in.
function fsSorted(dir) {
  return [...dir.entries].map(([n, node]) => [n, node, encoder.encode(n)]).sort((x, y) => fsCmp(x[2], y[2])).map(([n, node]) => [n, node]);
}
function fsPath(p, l) { return decoder.decode(bytes().subarray(p, p + l)); }
function fsDirFd(fd) {
  const e = fsFds.get(fd);
  if (e === undefined) return 8;
  if (e.node.type !== FS_DIR) return 54;
  return e;
}
// `path` beneath the directory fd `at`: the folder stack it walked, the
// parent and name of its last component (null for the folder itself), and
// the node there (undefined when absent), or `{ errno }`.
function fsWalk(at, path) {
  if (path === "") return { errno: 44 };
  if (path.startsWith("/")) return { errno: 63 };
  const parts = path.split("/");
  const trailing = parts.length > 1 && parts[parts.length - 1] === "";
  const comps = parts.filter((c) => c !== "" && c !== ".");
  const stack = at.stack.slice();
  for (let i = 0; i < comps.length - 1; i++) {
    const c = comps[i];
    if (c === "..") {
      if (stack.length === 1) return { errno: 63 };
      stack.pop();
      continue;
    }
    const n = stack[stack.length - 1].entries.get(c);
    if (n === undefined) return { errno: 44 };
    if (n.type !== FS_DIR) return { errno: 54 };
    stack.push(n);
  }
  const last = comps.length === 0 ? "." : comps[comps.length - 1];
  if (last === "." || last === "..") {
    if (last === "..") {
      if (stack.length === 1) return { errno: 63 };
      stack.pop();
    }
    return { stack, parent: null, name: null, node: stack[stack.length - 1], root: at.root };
  }
  const parent = stack[stack.length - 1];
  const node = parent.entries.get(last);
  if (trailing && node !== undefined && node.type !== FS_DIR) return { errno: 54 };
  return { stack, parent, name: last, node, root: at.root, trailing };
}
function fsDirSize(dir) {
  let n = 48 + 3;
  for (const k of dir.entries.keys()) n += 24 + encoder.encode(k).length;
  return n;
}
function fsStat(ptr, node) {
  const v = view();
  bytes().fill(0, ptr, ptr + 64);
  v.setBigUint64(ptr, 1n, true);
  v.setBigUint64(ptr + 8, BigInt(node.ino), true);
  v.setUint8(ptr + 16, node.type);
  v.setBigUint64(ptr + 24, 1n, true);
  v.setBigUint64(ptr + 32, BigInt(node.type === FS_DIR ? fsDirSize(node) : node.size), true);
  return 0;
}
function fsOwn(node) {
  if (node.shared) { node.buf = node.buf.slice(0, node.size); node.shared = false; }
}
function fsWriteAt(e, iovs, iovsLen, nwritten) {
  if (!e.write) return 8;
  const node = e.node;
  fsOwn(node);
  if (e.append) e.pos = node.size;
  let total = 0;
  for (let i = 0; i < iovsLen; i++) {
    const v = view();
    const ptr = v.getUint32(iovs + 8 * i, true);
    const len = v.getUint32(iovs + 8 * i + 4, true);
    const end = e.pos + len;
    if (end > node.buf.length) {
      const b = new Uint8Array(Math.max(end, node.buf.length * 2, 64));
      b.set(node.buf.subarray(0, node.size));
      node.buf = b;
    }
    node.buf.set(bytes().subarray(ptr, ptr + len), e.pos);
    e.pos = end;
    if (end > node.size) node.size = end;
    total += len;
  }
  view().setUint32(nwritten, total, true);
  return 0;
}
function fsReadAt(e, iovs, iovsLen, nread) {
  const node = e.node;
  if (node.type === FS_DIR) return 31;
  if (!e.read) return 8;
  let total = 0;
  for (let i = 0; i < iovsLen; i++) {
    const v = view();
    const ptr = v.getUint32(iovs + 8 * i, true);
    const len = v.getUint32(iovs + 8 * i + 4, true);
    const n = Math.max(0, Math.min(len, node.size - e.pos));
    bytes().set(node.buf.subarray(e.pos, e.pos + n), ptr);
    e.pos += n;
    total += n;
    if (n < len) break;
  }
  view().setUint32(nread, total, true);
  return 0;
}
