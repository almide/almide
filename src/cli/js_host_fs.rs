//! The JS host's in-memory filesystem (#4438): `init(source, { preopens })`
//! hands the module folders the page holds, and the module's WASI p1 fs
//! imports read and write them. The runtime (`js_host_fs.js`) and the fs
//! variants of the shared shims ship only when the shipped module imports an
//! fs op (#2276): a module without one keeps today's glue byte for byte.

/// The imports that only a filesystem serves: one of them in the shipped
/// module turns the in-memory filesystem on.
pub(super) const FS_IMPORTS: &[&str] = &[
    "fd_prestat_get",
    "fd_prestat_dir_name",
    "path_open",
    "fd_readdir",
    "fd_filestat_get",
    "path_filestat_get",
    "path_create_directory",
    "path_remove_directory",
    "path_unlink_file",
    "path_rename",
];

/// Does the shipped module import an fs op?
pub(super) fn active<'a>(mut wasi_imports: impl Iterator<Item = &'a str>) -> bool {
    wasi_imports.any(|n| FS_IMPORTS.contains(&n))
}

/// The runtime the fs shims call: the tree, the fd table, the walk.
pub(super) const FS_RUNTIME_JS: &str = include_str!("js_host_fs.js");

/// `fsReset` runs in `init()`, before instantiation: a bad tree is a
/// `TypeError` from `init`, never a failure in the middle of `run()`.
pub(super) const FS_INIT_JS: &str = "  fsReset(h.preopens);\n";

/// The shims with a filesystem behind them. A name here that is also in the
/// base table (`fd_write`, `fd_read`, `fd_close`, `fd_seek`) replaces the base
/// body when the filesystem is on; the stdio behaviour is unchanged.
pub(super) const FS_SHIMS: &[(&str, &str)] = &[
    ("fd_write", r#"  fd_write(fd, iovs, iovsLen, nwritten) {
    const fe = fsFds.get(fd);
    if (fe !== undefined) return fe.node.type === FS_DIR ? 31 : fsWriteAt(fe, iovs, iovsLen, nwritten);
    const v = view();
    let total = 0;
    const parts = [];
    for (let i = 0; i < iovsLen; i++) {
      const ptr = v.getUint32(iovs + 8 * i, true);
      const len = v.getUint32(iovs + 8 * i + 4, true);
      parts.push(bytes().slice(ptr, ptr + len));
      total += len;
    }
    if (fd !== 1 && fd !== 2) return 8; // EBADF
    const chunk = new Uint8Array(total);
    let off = 0;
    for (const p of parts) { chunk.set(p, off); off += p.length; }
    emit(fd, chunk);
    view().setUint32(nwritten, total, true);
    return 0;
  }"#),
    ("fd_read", r#"  fd_read(fd, iovs, iovsLen, nread) {
    const fe = fsFds.get(fd);
    if (fe !== undefined) return fsReadAt(fe, iovs, iovsLen, nread);
    if (fd !== 0) return 8;
    view().setUint32(nread, 0, true);
    return 0;
  }"#),
    ("fd_close", r#"  fd_close(fd) {
    if (fsFds.delete(fd)) return 0;
    return fd <= 2 ? 0 : 8;
  }"#),
    ("fd_seek", r#"  fd_seek(fd, offset, whence, out) {
    const e = fsFds.get(fd);
    if (e === undefined || e.node.type === FS_DIR) return 8;
    const base = whence === 0 ? 0 : whence === 1 ? e.pos : whence === 2 ? e.node.size : -1;
    if (base < 0) return 28;
    const pos = base + Number(offset);
    if (pos < 0) return 28;
    e.pos = pos;
    view().setBigUint64(out, BigInt(pos), true);
    return 0;
  }"#),
    ("fd_prestat_get", r#"  fd_prestat_get(fd, out) {
    const e = fsFds.get(fd);
    if (e === undefined || e.preopen === undefined) return 8;
    const v = view();
    v.setUint32(out, 0, true);
    v.setUint32(out + 4, e.preopen.length, true);
    return 0;
  }"#),
    ("fd_prestat_dir_name", r#"  fd_prestat_dir_name(fd, p, l) {
    const e = fsFds.get(fd);
    if (e === undefined || e.preopen === undefined) return 8;
    if (l < e.preopen.length) return 37;
    bytes().set(e.preopen, p);
    return 0;
  }"#),
    ("path_open", r#"  path_open(fd, dirflags, p, l, oflags, rightsBase, rightsInheriting, fdflags, out) {
    const at = fsDirFd(fd);
    if (typeof at === "number") return at;
    const w = fsWalk(at, fsPath(p, l));
    if (w.errno) return w.errno;
    const write = (BigInt(rightsBase) & 64n) !== 0n || (oflags & 8) !== 0;
    let node = w.node;
    if (node === undefined) {
      if ((oflags & 1) === 0) return 44;
      if ((oflags & 2) !== 0) return 28;
      if (w.trailing) return 31;
      if (!w.root.writable) return 2;
      node = fsFileNode(new Uint8Array(0), false);
      w.parent.entries.set(w.name, node);
    } else {
      if ((oflags & 5) === 5) return 20;
      if ((oflags & 2) !== 0 && node.type !== FS_DIR) return 54;
      if (node.type === FS_DIR && write) return 31;
      if (write && !w.root.writable) return 2;
      if ((oflags & 8) !== 0) { node.buf = new Uint8Array(0); node.size = 0; node.shared = false; }
    }
    const e = { node, root: w.root, pos: 0, append: (fdflags & 1) !== 0, read: (BigInt(rightsBase) & 2n) !== 0n, write };
    if (node.type === FS_DIR) e.stack = w.name === null ? w.stack : [...w.stack, node];
    const nfd = fsNextFd++;
    fsFds.set(nfd, e);
    view().setUint32(out, nfd, true);
    return 0;
  }"#),
    ("fd_readdir", r#"  fd_readdir(fd, buf, len, cookie, used) {
    const e = fsFds.get(fd);
    if (e === undefined) return 8;
    if (e.node.type !== FS_DIR) return 54;
    const up = e.stack.length > 1 ? e.stack[e.stack.length - 2] : e.node;
    const list = [[".", e.node], ["..", up], ...fsSorted(e.node)];
    let off = 0;
    for (let i = Number(cookie); i < list.length && off < len; i++) {
      const [name, node] = list[i];
      const nb = encoder.encode(name);
      const ent = new Uint8Array(24 + nb.length);
      const dv = new DataView(ent.buffer);
      dv.setBigUint64(0, BigInt(i + 1), true);
      dv.setBigUint64(8, BigInt(node.ino), true);
      dv.setUint32(16, nb.length, true);
      dv.setUint8(20, node.type);
      ent.set(nb, 24);
      const n = Math.min(ent.length, len - off);
      bytes().set(ent.subarray(0, n), buf + off);
      off += n;
    }
    view().setUint32(used, off, true);
    return 0;
  }"#),
    ("fd_filestat_get", r#"  fd_filestat_get(fd, out) {
    const e = fsFds.get(fd);
    if (e === undefined) return 8;
    return fsStat(out, e.node);
  }"#),
    ("path_filestat_get", r#"  path_filestat_get(fd, flags, p, l, out) {
    const at = fsDirFd(fd);
    if (typeof at === "number") return at;
    const w = fsWalk(at, fsPath(p, l));
    if (w.errno) return w.errno;
    if (w.node === undefined) return 44;
    return fsStat(out, w.node);
  }"#),
    ("path_create_directory", r#"  path_create_directory(fd, p, l) {
    const at = fsDirFd(fd);
    if (typeof at === "number") return at;
    const w = fsWalk(at, fsPath(p, l));
    if (w.errno) return w.errno;
    if (w.node !== undefined) return 20;
    if (!w.root.writable) return 2;
    w.parent.entries.set(w.name, fsDirNode());
    return 0;
  }"#),
    ("path_remove_directory", r#"  path_remove_directory(fd, p, l) {
    const at = fsDirFd(fd);
    if (typeof at === "number") return at;
    const w = fsWalk(at, fsPath(p, l));
    if (w.errno) return w.errno;
    if (w.node === undefined) return 44;
    if (w.name === null) return 28;
    if (w.node.type !== FS_DIR) return 54;
    if (!w.root.writable) return 2;
    if (w.node.entries.size > 0) return 55;
    w.parent.entries.delete(w.name);
    return 0;
  }"#),
    ("path_unlink_file", r#"  path_unlink_file(fd, p, l) {
    const at = fsDirFd(fd);
    if (typeof at === "number") return at;
    const w = fsWalk(at, fsPath(p, l));
    if (w.errno) return w.errno;
    if (w.node === undefined) return 44;
    if (w.name === null || w.node.type === FS_DIR) return 31;
    if (!w.root.writable) return 2;
    w.parent.entries.delete(w.name);
    return 0;
  }"#),
    ("path_rename", r#"  path_rename(fd1, p1, l1, fd2, p2, l2) {
    const a0 = fsDirFd(fd1);
    if (typeof a0 === "number") return a0;
    const b0 = fsDirFd(fd2);
    if (typeof b0 === "number") return b0;
    const a = fsWalk(a0, fsPath(p1, l1));
    if (a.errno) return a.errno;
    const b = fsWalk(b0, fsPath(p2, l2));
    if (b.errno) return b.errno;
    if (a.node === undefined) return 44;
    if (a.name === null || b.name === null) return 28;
    if (!a.root.writable || !b.root.writable) return 2;
    if (a.node === b.node) return 0;
    if (b.node !== undefined) {
      if (b.node.type === FS_DIR) {
        if (a.node.type !== FS_DIR) return 31;
        if (b.node.entries.size > 0) return 55;
      } else if (a.node.type === FS_DIR) {
        return 54;
      }
    }
    if (a.node.type === FS_DIR && b.stack.includes(a.node)) return 28;
    b.parent.entries.set(b.name, a.node);
    a.parent.entries.delete(a.name);
    return 0;
  }"#),
];

/// The fs body for `name`, when the filesystem is on and serves it.
pub(super) fn shim(name: &str) -> Option<&'static str> {
    FS_SHIMS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

/// The `.d.ts` the filesystem adds: the tree a preopen takes and `files()`.
pub(super) const FS_DTS_TYPES: &str = r#"/** A folder: names (a `/` nests) to bytes, a string (stored as UTF-8) or a sub-folder. */
export type Tree =
  | Map<string, Uint8Array | ArrayBuffer | ArrayBufferView | string | Tree>
  | { [path: string]: Uint8Array | ArrayBuffer | ArrayBufferView | string | Tree };
/** One folder the module sees at the preopen's name. Read-only unless `writable`. */
export interface Preopen {
  files?: Tree;
  writable?: boolean;
}
/** Every file under the preopen `name` after the run, by its path inside it, sorted; the bytes are copies. */
export function files(name: string): Map<string, Uint8Array>;

"#;

/// The `Hooks` member the filesystem adds.
pub(super) const FS_DTS_HOOK: &str = "  /** The folders the module sees, in order (the first is fd 3); none when omitted. */\n  preopens?: Record<string, Preopen> | Map<string, Preopen>;\n";
