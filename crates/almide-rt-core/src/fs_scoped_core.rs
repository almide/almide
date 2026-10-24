// fs_scoped_core (#4084): every fs call under `[permissions] fs` (C-393) made
// beneath a held descriptor of the granted directory it falls under, so a
// tree rewritten between the scope's decision and the call (a directory
// swapped for a symlink) cannot steer the call out of the grant. ONE source
// for two consumers, through fs_beneath_core.rs, which includes it:
//
// - the native runtime (runtime/rs/src/fs.rs includes fs_beneath_core.rs):
//   a scoped build calls a twin of the runtime's fs functions whose `std::fs`
//   calls are spelled `almide_sfs::` (src/cli/fs_scope_twin.rs), so the
//   runtime source itself is the same with and without a scope;
// - the embedded wasm host (almide-wasm-run host_fs.rs), whose fs ops call
//   `almide_sfs::` directly.
//
// How a call runs: fs_scope.rs DECIDES where the path lands (its longest
// existing prefix canonicalized, the rest applied lexically) and under which
// granted root; the call then walks from that root's descriptor — opened
// once, on first use, by its own canonical path — segment by segment without
// following a symlink (openat2 RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS on
// Linux with the per-segment walk as its ENOSYS / EPERM / E2BIG fallback, the
// O_NOFOLLOW openat walk on macOS: fs_beneath_core's `parent_from`), and
// opens, creates, lists, removes or renames the last segment from its
// parent's descriptor with O_NOFOLLOW. The decided segments hold no symlink
// (the decision resolved them all), so a symlink the walk meets was put
// there after the decision: the call answers the scope's refusal. Errors
// other than that are the plain call's own io errors on the same path.
//
// No-follow calls (`remove*`, `rename`, `is_symlink`) decide the parent and
// keep the last name as written, so they act on a link, never through it.
//
// Residual windows, by construction of what std exposes: the metadata of an
// entry the process may not open (EACCES on the read-only open) is taken by
// the path's lstat after the walk checked the last name is no symlink; an
// operation on a granted root ITSELF (the empty segment list: removing,
// renaming or creating the grant's own directory) is made on the root's
// canonical path. On a platform without the descriptor calls (other than
// 64-bit Linux and macOS) every call decides and then makes the plain call
// on the decided path: the pre-#4084 check, which a concurrent rewrite can
// still race.
//
// Splice discipline (lib.rs): no `crate::` paths, no `use` lines; the inline
// `mod almide_sfs` is the facade both consumers name.

include!("fs_scope.rs");

/// The marker a scoped call's refusal carries in its io error text. Each
/// consumer replaces any error holding it with [`fs_scope_refusal`] of the
/// path the program wrote, so the refusal reads the same whether the scope
/// refused before the call or the call met a symlink swapped in after.
pub const FS_SCOPED_REFUSED: &str = "\u{1}almide: outside [permissions] fs\u{1}";

/// Whether an error text is a scoped call's refusal.
pub fn fs_scoped_refused_in(msg: &str) -> bool {
    msg.contains(FS_SCOPED_REFUSED)
}

fn fs_scoped_refused() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, FS_SCOPED_REFUSED)
}

/// The granted roots, each with the descriptor its calls walk from.
pub struct FsScoped {
    roots: Vec<FsScopedRoot>,
}

struct FsScopedRoot {
    /// The root as the manifest anchored it.
    raw: String,
    /// The root as [`fs_scope_root`] spells it, kept once it exists (its
    /// canonical form); a root that does not exist yet is re-anchored at
    /// every decision, as the scope's own check does.
    path: std::sync::OnceLock<std::path::PathBuf>,
    /// The root's descriptor, opened on its first use and held for the
    /// process; a root that does not exist yet is retried at the next call.
    #[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
    held: std::sync::OnceLock<std::os::fd::OwnedFd>,
}

static FS_SCOPED_CURRENT: std::sync::RwLock<Option<std::sync::Arc<FsScoped>>> = std::sync::RwLock::new(None);

/// Bound every `almide_sfs` call by `roots` (`[permissions] fs`), or lift
/// the bound (`None`). Process-wide, like the key.
pub fn fs_scoped_install(roots: Option<&[String]>) {
    let next = roots.map(|r| std::sync::Arc::new(FsScoped::new(r)));
    *FS_SCOPED_CURRENT.write().unwrap_or_else(|p| p.into_inner()) = next;
}

/// The bound [`fs_scoped_install`] set, if any.
pub fn fs_scoped_current() -> Option<std::sync::Arc<FsScoped>> {
    FS_SCOPED_CURRENT.read().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Whether `path` (its symlinks followed) lands under the installed scope's
/// roots — the scope's check before a call, the `fs_scope_admits` answer
/// over the same roots. The resolution it made is kept for the call that
/// follows on this thread, so the call does not resolve the path again
/// (a decision only: the call still walks beneath the root).
pub fn fs_scoped_admits(path: &str) -> bool {
    let Some(sc) = fs_scoped_current() else { return true };
    match fs_scope_resolve_parts(path) {
        Some((resolved, dotdot)) => {
            let ok = sc.root_of(&resolved).is_some();
            if ok {
                FS_SCOPED_LAST.with(|l| *l.borrow_mut() = Some((path.to_string(), resolved, dotdot)));
            }
            ok
        }
        None => false,
    }
}

thread_local! {
    /// The resolution [`fs_scoped_admits`] made, taken by the next decision
    /// on this thread (whatever its path: it never outlives one call).
    static FS_SCOPED_LAST: std::cell::RefCell<Option<(String, std::path::PathBuf, Option<std::io::Error>)>> = const { std::cell::RefCell::new(None) };
}

/// Where a path lands: its root, the segments under it, and the error the
/// plain call meets first when a `..` steps back over a component that does
/// not resolve (fs_scope_resolve_parts).
struct FsScopedPlace {
    root: usize,
    segs: Vec<String>,
    dotdot: Option<std::io::Error>,
}

impl FsScopedPlace {
    /// The segments, or the plain call's error when the path holds a `..`
    /// the OS would not get past.
    fn segs(self) -> std::io::Result<(usize, Vec<String>)> {
        match self.dotdot {
            Some(e) => Err(e),
            None => Ok((self.root, self.segs)),
        }
    }
}

impl FsScoped {
    /// The scope over `roots` (each anchored by [`fs_scope_root`]).
    pub fn new(roots: &[String]) -> Self {
        FsScoped {
            roots: roots
                .iter()
                .map(|r| FsScopedRoot {
                    raw: r.clone(),
                    path: std::sync::OnceLock::new(),
                    #[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
                    held: std::sync::OnceLock::new(),
                })
                .collect(),
        }
    }

    /// Root `i` as the decisions compare it.
    fn root_path(&self, i: usize) -> std::path::PathBuf {
        let r = &self.roots[i];
        if let Some(p) = r.path.get() {
            return p.clone();
        }
        let p = fs_scope_root(&r.raw);
        if p.exists() {
            let _ = r.path.set(p.clone());
        }
        p
    }

    /// Where `path` lands under the roots; `follow` false keeps its last
    /// name unresolved (the no-follow calls). The refusal when it lands
    /// under none.
    fn place(&self, path: &str, follow: bool) -> std::io::Result<FsScopedPlace> {
        let p = std::path::Path::new(path);
        let last_is_name = matches!(p.components().next_back(), Some(std::path::Component::Normal(_)));
        let (resolved, dotdot) = match p.file_name() {
            Some(name) if !follow && last_is_name => {
                let parent = p.parent().map(|q| q.to_string_lossy().into_owned()).filter(|q| !q.is_empty()).unwrap_or_else(|| ".".to_string());
                let (mut r, e) = fs_scope_resolve_parts(&parent).ok_or_else(fs_scoped_refused)?;
                r.push(name);
                (r, e)
            }
            _ => match FS_SCOPED_LAST.with(|l| l.borrow_mut().take()).filter(|(p, _, _)| p == path) {
                Some((_, r, e)) => (r, e),
                None => fs_scope_resolve_parts(path).ok_or_else(fs_scoped_refused)?,
            },
        };
        let (root, segs) = self.root_of(&resolved).ok_or_else(fs_scoped_refused)?;
        Ok(FsScopedPlace { root, segs, dotdot })
    }

    /// The root a resolved path lies under, and its segments below it.
    fn root_of(&self, resolved: &std::path::Path) -> Option<(usize, Vec<String>)> {
        (0..self.roots.len()).find_map(|i| {
            let rest = resolved.strip_prefix(self.root_path(i)).ok()?;
            Some((i, rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect()))
        })
    }

    /// The decided path `<root>/<segs>` (errors, the residual windows and
    /// the path-based fallback name it).
    fn joined(&self, root: usize, segs: &[String]) -> std::path::PathBuf {
        let mut p = self.root_path(root);
        p.extend(segs);
        p
    }

    /// Whether `path` lands under a root at all (the predicates' answer to a
    /// refusal is false).
    fn admits(&self, path: &str, follow: bool) -> bool {
        self.place(path, follow).is_ok()
    }
}

/// The descriptor half: 64-bit Linux and macOS.
#[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
impl FsScoped {
    /// The held descriptor of root `i`, opened by its canonical path on the
    /// first call that needs it.
    fn held(&self, i: usize) -> std::io::Result<&std::os::fd::OwnedFd> {
        let r = &self.roots[i];
        if let Some(fd) = r.held.get() {
            return Ok(fd);
        }
        let fd: std::os::fd::OwnedFd = std::fs::File::open(self.root_path(i))?.into();
        let _ = r.held.set(fd);
        r.held.get().ok_or_else(|| std::io::Error::other("root descriptor"))
    }

    /// The directory `<root i>/<segs...>`, walked from the held root
    /// without following a symlink; a symlink met is the refusal.
    fn dir(&self, i: usize, segs: &[String]) -> std::io::Result<std::os::fd::OwnedFd> {
        let root = self.held(i)?;
        if segs.is_empty() {
            return root.try_clone();
        }
        let v: Vec<&str> = segs.iter().map(String::as_str).collect();
        let r = fs_beneath_imp::parent_from(root, &v, false, &mut |_| {});
        r.map_err(fs_scoped_from_beneath)
    }

    /// `last` under `dir`, opened as `how` says, never through a symlink.
    fn open_last(dir: &std::os::fd::OwnedFd, last: &str, how: almide_fs_beneath_sys::Open) -> std::io::Result<std::fs::File> {
        almide_fs_beneath_sys::open_at(std::os::fd::AsFd::as_fd(dir), last, how)
            .map(std::fs::File::from)
            .map_err(|e| fs_scoped_from_beneath(fs_beneath_imp::classify(dir, last, e)))
    }

    /// `path` opened as `how` says (symlinks resolved by the decision, none
    /// followed by the call).
    fn open(&self, path: &str, how: almide_fs_beneath_sys::Open) -> std::io::Result<std::fs::File> {
        let (i, segs) = self.place(path, true)?.segs()?;
        match segs.split_last() {
            Some((last, parents)) => Self::open_last(&self.dir(i, parents)?, last, how),
            None => fs_scoped_open_path(&self.root_path(i), how),
        }
    }

    /// The directory `path` as a descriptor (a rooted call's root).
    pub fn open_dir(&self, path: &str) -> std::io::Result<std::os::fd::OwnedFd> {
        let (i, segs) = self.place(path, true)?.segs()?;
        self.dir(i, &segs)
    }

    /// `path`'s metadata, its symlinks followed by the decision.
    pub fn metadata(&self, path: &str) -> std::io::Result<std::fs::Metadata> {
        let (i, segs) = self.place(path, true)?.segs()?;
        let Some((last, parents)) = segs.split_last() else {
            return std::fs::metadata(&self.root_path(i));
        };
        let dir = self.dir(i, parents)?;
        match Self::open_last(&dir, last, almide_fs_beneath_sys::Open::Meta) {
            Ok(f) => f.metadata(),
            // An entry the process may not open: its lstat, once the walk
            // checked the last name is no symlink (the residual window).
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !fs_scoped_refused_in(&e.to_string()) => {
                match almide_fs_beneath_sys::symlink_at(std::os::fd::AsFd::as_fd(&dir), last) {
                    Ok(false) => std::fs::symlink_metadata(self.joined(i, &segs)),
                    Ok(true) => Err(fs_scoped_refused()),
                    Err(_) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// The entry names of the directory `path`, in the system's order.
    pub fn read_dir_names(&self, path: &str) -> std::io::Result<Vec<std::ffi::OsString>> {
        let (i, segs) = self.place(path, true)?.segs()?;
        let fd = match segs.split_last() {
            Some((last, parents)) => {
                let dir = self.dir(i, parents)?;
                almide_fs_beneath_sys::open_at(std::os::fd::AsFd::as_fd(&dir), last, almide_fs_beneath_sys::Open::Dir)
                    .map_err(|e| fs_scoped_from_beneath(fs_beneath_imp::classify(&dir, last, e)))?
            }
            None => self.dir(i, &[])?,
        };
        let names = almide_fs_beneath_sys::list_fd(fd)?;
        Ok(names.into_iter().map(<std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec).collect())
    }

    /// `std::fs::create_dir_all`, one segment at a time from the root: a
    /// missing segment is created, an existing one opened without following.
    pub fn create_dir_all(&self, path: &str) -> std::io::Result<()> {
        // A `..` over a missing directory is fine here: the plain call
        // creates it on its way, as the lexical rest does.
        let place = self.place(path, true)?;
        let (i, segs) = (place.root, place.segs);
        if segs.is_empty() {
            return std::fs::create_dir_all(&self.root_path(i));
        }
        let mut cur = self.dir(i, &[])?;
        for (k, seg) in segs.iter().enumerate() {
            let open = |cur: &std::os::fd::OwnedFd| almide_fs_beneath_sys::open_at(std::os::fd::AsFd::as_fd(cur), seg, almide_fs_beneath_sys::Open::Dir);
            let next = match open(&cur) {
                Ok(fd) => fd,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    match almide_fs_beneath_sys::mkdir_at(std::os::fd::AsFd::as_fd(&cur), seg) {
                        Ok(()) => {}
                        Err(m) if m.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(m) => return Err(m),
                    }
                    open(&cur).map_err(|e| fs_scoped_from_beneath(fs_beneath_imp::classify(&cur, seg, e)))?
                }
                Err(e) => {
                    if let Ok(true) = almide_fs_beneath_sys::symlink_at(std::os::fd::AsFd::as_fd(&cur), seg) {
                        return Err(fs_scoped_refused());
                    }
                    // Not a directory: the plain call's mkdir answers EEXIST
                    // at the last segment and ENOTDIR on the way.
                    return Err(if e.raw_os_error() == Some(20) {
                        std::io::Error::from_raw_os_error(if k + 1 == segs.len() { 17 } else { 20 })
                    } else {
                        e
                    });
                }
            };
            cur = next;
        }
        Ok(())
    }

    /// Remove the file (`dir` false) or empty directory `path` itself.
    pub fn remove(&self, path: &str, dir: bool) -> std::io::Result<()> {
        let (i, segs) = self.place(path, false)?.segs()?;
        let Some((last, parents)) = segs.split_last() else {
            let p = &self.root_path(i);
            return if dir { std::fs::remove_dir(p) } else { std::fs::remove_file(p) };
        };
        almide_fs_beneath_sys::unlink_at(std::os::fd::AsFd::as_fd(&self.dir(i, parents)?), last, dir)
    }

    /// `std::fs::remove_dir_all`: the tree under `path`, walked by
    /// descriptor; a symlink anywhere is removed itself, never followed.
    pub fn remove_dir_all(&self, path: &str) -> std::io::Result<()> {
        let (i, segs) = self.place(path, false)?.segs()?;
        let Some((last, parents)) = segs.split_last() else {
            return std::fs::remove_dir_all(&self.root_path(i));
        };
        let dir = self.dir(i, parents)?;
        fs_scoped_remove_entry(&dir, last)
    }

    /// `std::fs::rename` of the entry `from` (not followed) to `to`.
    pub fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        let (i, a) = self.place(from, false)?.segs()?;
        let (j, b) = self.place(to, false)?.segs()?;
        match (a.split_last(), b.split_last()) {
            (Some((la, pa)), Some((lb, pb))) => {
                let (da, db) = (self.dir(i, pa)?, self.dir(j, pb)?);
                almide_fs_beneath_sys::rename_at(std::os::fd::AsFd::as_fd(&da), la, std::os::fd::AsFd::as_fd(&db), lb)
            }
            _ => std::fs::rename(self.joined(i, &a), self.joined(j, &b)),
        }
    }

    /// Whether `path` itself is a symlink (its last name not followed).
    pub fn is_symlink(&self, path: &str) -> bool {
        let Ok((i, segs)) = self.place(path, false).and_then(FsScopedPlace::segs) else { return false };
        let Some((last, parents)) = segs.split_last() else {
            return self.root_path(i).is_symlink();
        };
        self.dir(i, parents).is_ok_and(|d| matches!(almide_fs_beneath_sys::symlink_at(std::os::fd::AsFd::as_fd(&d), last), Ok(true)))
    }
}

/// A root itself (the empty segment list), opened by its canonical path.
#[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
fn fs_scoped_open_path(p: &std::path::Path, how: almide_fs_beneath_sys::Open) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    match how {
        almide_fs_beneath_sys::Open::Dir | almide_fs_beneath_sys::Open::Read | almide_fs_beneath_sys::Open::Meta => o.read(true),
        almide_fs_beneath_sys::Open::WriteTrunc => o.write(true).create(true).truncate(true),
        almide_fs_beneath_sys::Open::Append => o.append(true).create(true),
    };
    o.open(p)
}

/// Remove `name` under `dir`: a symlink or file unlinked, a directory
/// emptied by descriptor first.
#[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
fn fs_scoped_remove_entry(dir: &std::os::fd::OwnedFd, name: &str) -> std::io::Result<()> {
    let d = std::os::fd::AsFd::as_fd(dir);
    if let Ok(true) = almide_fs_beneath_sys::symlink_at(d, name) {
        return almide_fs_beneath_sys::unlink_at(d, name, false);
    }
    match almide_fs_beneath_sys::open_at(d, name, almide_fs_beneath_sys::Open::Dir) {
        Ok(child) => {
            for n in almide_fs_beneath_sys::list_fd(child.try_clone()?)? {
                match fs_scoped_remove_entry(&child, &String::from_utf8_lossy(&n)) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    r => r?,
                }
            }
            almide_fs_beneath_sys::unlink_at(d, name, true)
        }
        // A file (or an entry swapped for a symlink since the check): the
        // entry itself goes, never what it points at.
        Err(e) if matches!(e.raw_os_error(), Some(20 | 40 | 62)) => almide_fs_beneath_sys::unlink_at(d, name, false),
        Err(e) => Err(e),
    }
}

/// A walk's `Escape` is the scope's refusal; any other error is the call's.
#[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
fn fs_scoped_from_beneath(e: FsBeneathError) -> std::io::Error {
    match e {
        FsBeneathError::Escape => fs_scoped_refused(),
        FsBeneathError::Io(e) => e,
    }
}

/// The path-based half: elsewhere, every call decides and then makes the
/// plain call on the decided path (racy, see the file header).
#[cfg(not(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64")))]
impl FsScoped {
    fn decided(&self, path: &str, follow: bool) -> std::io::Result<std::path::PathBuf> {
        let (i, segs) = self.place(path, follow)?.segs()?;
        Ok(self.joined(i, &segs))
    }
    pub fn open_with(&self, path: &str, o: &std::fs::OpenOptions) -> std::io::Result<std::fs::File> {
        o.open(self.decided(path, true)?)
    }
    pub fn metadata(&self, path: &str) -> std::io::Result<std::fs::Metadata> {
        std::fs::metadata(self.decided(path, true)?)
    }
    pub fn read_dir_names(&self, path: &str) -> std::io::Result<Vec<std::ffi::OsString>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(self.decided(path, true)?)? {
            out.push(e?.file_name());
        }
        Ok(out)
    }
    pub fn create_dir_all(&self, path: &str) -> std::io::Result<()> {
        let place = self.place(path, true)?;
        std::fs::create_dir_all(self.joined(place.root, &place.segs))
    }
    pub fn remove(&self, path: &str, dir: bool) -> std::io::Result<()> {
        let p = self.decided(path, false)?;
        if dir { std::fs::remove_dir(p) } else { std::fs::remove_file(p) }
    }
    pub fn remove_dir_all(&self, path: &str) -> std::io::Result<()> {
        std::fs::remove_dir_all(self.decided(path, false)?)
    }
    pub fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        std::fs::rename(self.decided(from, false)?, self.decided(to, false)?)
    }
    pub fn is_symlink(&self, path: &str) -> bool {
        self.decided(path, false).is_ok_and(|p| p.is_symlink())
    }
}

/// The facade both consumers call: `std::fs`'s names and shapes, bounded by
/// the installed scope ([`fs_scoped_install`]) and the plain `std::fs` call
/// when none is installed. The native scoped twin is the runtime's fs source
/// with `std::fs::` spelled `almide_sfs::` and the path predicates spelled
/// through [`almide_sfs::FsScopedPathExt`].
pub mod almide_sfs {
    type Scoped = std::sync::Arc<super::FsScoped>;

    fn scope() -> Option<Scoped> {
        super::fs_scoped_current()
    }

    fn s(p: &std::path::Path) -> std::borrow::Cow<'_, str> {
        p.to_string_lossy()
    }

    #[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
    fn open(sc: &Scoped, p: &std::path::Path, o: &OpenOptions) -> std::io::Result<std::fs::File> {
        let how = if o.append {
            super::almide_fs_beneath_sys::Open::Append
        } else if o.write || o.create || o.truncate {
            super::almide_fs_beneath_sys::Open::WriteTrunc
        } else {
            super::almide_fs_beneath_sys::Open::Read
        };
        sc.open(&s(p), how)
    }

    #[cfg(not(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64")))]
    fn open(sc: &Scoped, p: &std::path::Path, o: &OpenOptions) -> std::io::Result<std::fs::File> {
        sc.open_with(&s(p), &o.std())
    }

    /// `std::fs::OpenOptions`, the flags the runtime sets.
    #[derive(Clone, Default)]
    pub struct OpenOptions {
        read: bool,
        write: bool,
        append: bool,
        create: bool,
        truncate: bool,
    }

    impl OpenOptions {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn read(&mut self, v: bool) -> &mut Self {
            self.read = v;
            self
        }
        pub fn write(&mut self, v: bool) -> &mut Self {
            self.write = v;
            self
        }
        pub fn append(&mut self, v: bool) -> &mut Self {
            self.append = v;
            self
        }
        pub fn create(&mut self, v: bool) -> &mut Self {
            self.create = v;
            self
        }
        pub fn truncate(&mut self, v: bool) -> &mut Self {
            self.truncate = v;
            self
        }
        fn std(&self) -> std::fs::OpenOptions {
            let mut o = std::fs::OpenOptions::new();
            o.read(self.read).write(self.write).append(self.append).create(self.create).truncate(self.truncate);
            o
        }
        pub fn open<P: AsRef<std::path::Path>>(&self, p: P) -> std::io::Result<std::fs::File> {
            match scope() {
                None => self.std().open(p),
                Some(sc) => open(&sc, p.as_ref(), self),
            }
        }
    }

    /// `std::fs::File::open` / `File::create`; the file is `std::fs::File`.
    pub struct File;

    impl File {
        pub fn open<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<std::fs::File> {
            OpenOptions::new().read(true).open(p)
        }
        pub fn create<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<std::fs::File> {
            OpenOptions::new().write(true).create(true).truncate(true).open(p)
        }
    }

    pub fn read<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<Vec<u8>> {
        if scope().is_none() {
            return std::fs::read(p);
        }
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut File::open(p)?, &mut out)?;
        Ok(out)
    }

    pub fn read_to_string<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<String> {
        if scope().is_none() {
            return std::fs::read_to_string(p);
        }
        let mut out = String::new();
        std::io::Read::read_to_string(&mut File::open(p)?, &mut out)?;
        Ok(out)
    }

    pub fn write<P: AsRef<std::path::Path>, C: AsRef<[u8]>>(p: P, data: C) -> std::io::Result<()> {
        if scope().is_none() {
            return std::fs::write(p, data);
        }
        std::io::Write::write_all(&mut File::create(p)?, data.as_ref())
    }

    pub fn metadata<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<std::fs::Metadata> {
        match scope() {
            None => std::fs::metadata(p),
            Some(sc) => sc.metadata(&s(p.as_ref())),
        }
    }

    pub fn create_dir_all<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<()> {
        match scope() {
            None => std::fs::create_dir_all(p),
            Some(sc) => sc.create_dir_all(&s(p.as_ref())),
        }
    }

    pub fn remove_file<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<()> {
        match scope() {
            None => std::fs::remove_file(p),
            Some(sc) => sc.remove(&s(p.as_ref()), false),
        }
    }

    pub fn remove_dir<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<()> {
        match scope() {
            None => std::fs::remove_dir(p),
            Some(sc) => sc.remove(&s(p.as_ref()), true),
        }
    }

    pub fn remove_dir_all<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<()> {
        match scope() {
            None => std::fs::remove_dir_all(p),
            Some(sc) => sc.remove_dir_all(&s(p.as_ref())),
        }
    }

    pub fn rename<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(from: P, to: Q) -> std::io::Result<()> {
        match scope() {
            None => std::fs::rename(from, to),
            Some(sc) => sc.rename(&s(from.as_ref()), &s(to.as_ref())),
        }
    }

    /// `std::fs::copy`: the source opened for reading (a regular file, as
    /// std requires), the destination created or truncated with its
    /// permissions, the bytes copied.
    pub fn copy<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(from: P, to: Q) -> std::io::Result<u64> {
        if scope().is_none() {
            return std::fs::copy(from, to);
        }
        let mut src = File::open(from)?;
        let meta = src.metadata()?;
        if !meta.is_file() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "the source path is neither a regular file nor a symlink to a regular file"));
        }
        let mut dst = File::create(to)?;
        dst.set_permissions(meta.permissions())?;
        std::io::copy(&mut src, &mut dst)
    }

    /// `std::fs::canonicalize`: the decided place, once it exists.
    pub fn canonicalize<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<std::path::PathBuf> {
        match scope() {
            None => std::fs::canonicalize(p),
            Some(sc) => {
                let path = s(p.as_ref()).into_owned();
                sc.metadata(&path)?;
                let place = sc.place(&path, true)?;
                let (i, segs) = place.segs()?;
                Ok(sc.joined(i, &segs))
            }
        }
    }

    /// `std::fs::read_dir`'s entries, in the system's order.
    pub fn read_dir<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<ReadDir> {
        let dir = p.as_ref().to_path_buf();
        let names: Vec<std::io::Result<std::ffi::OsString>> = match scope() {
            None => std::fs::read_dir(&dir)?.map(|e| e.map(|e| e.file_name())).collect(),
            Some(sc) => sc.read_dir_names(&s(&dir))?.into_iter().map(Ok).collect(),
        };
        Ok(ReadDir { dir, names: names.into_iter() })
    }

    /// `std::fs::ReadDir`.
    pub struct ReadDir {
        dir: std::path::PathBuf,
        names: std::vec::IntoIter<std::io::Result<std::ffi::OsString>>,
    }

    impl Iterator for ReadDir {
        type Item = std::io::Result<DirEntry>;
        fn next(&mut self) -> Option<Self::Item> {
            let dir = &self.dir;
            self.names.next().map(|r| r.map(|name| DirEntry { path: dir.join(&name), name }))
        }
    }

    /// `std::fs::DirEntry`: its name and the directory's path joined to it.
    pub struct DirEntry {
        name: std::ffi::OsString,
        path: std::path::PathBuf,
    }

    impl DirEntry {
        pub fn file_name(&self) -> std::ffi::OsString {
            self.name.clone()
        }
        pub fn path(&self) -> std::path::PathBuf {
            self.path.clone()
        }
    }

    /// The directory `p` as a descriptor (a rooted call's root): beneath its
    /// granted root under a scope, by its path without one.
    #[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
    pub fn open_dir(p: &str) -> std::io::Result<std::os::fd::OwnedFd> {
        match scope() {
            None => std::fs::File::open(p).map(Into::into),
            Some(sc) => sc.open_dir(p),
        }
    }

    pub fn exists<P: AsRef<std::path::Path>>(p: P) -> bool {
        metadata(p).is_ok()
    }

    pub fn is_dir<P: AsRef<std::path::Path>>(p: P) -> bool {
        metadata(p).is_ok_and(|m| m.is_dir())
    }

    pub fn is_file<P: AsRef<std::path::Path>>(p: P) -> bool {
        metadata(p).is_ok_and(|m| m.is_file())
    }

    /// `Path::is_symlink`: the last name is not followed.
    pub fn is_symlink<P: AsRef<std::path::Path>>(p: P) -> bool {
        match scope() {
            None => p.as_ref().is_symlink(),
            Some(sc) => sc.is_symlink(&s(p.as_ref())),
        }
    }

    /// Whether `p` lands under the installed scope (always, without one).
    pub fn admits(p: &str, follow: bool) -> bool {
        scope().is_none_or(|sc| sc.admits(p, follow))
    }

    /// The path predicates and `Metadata`'s, under one spelling the twin
    /// rewrites `.is_dir()` and its kin to: a path asks the scope, a
    /// `Metadata` answers itself.
    pub trait FsScopedPathExt {
        fn __almide_s_exists(&self) -> bool;
        fn __almide_s_is_dir(&self) -> bool;
        fn __almide_s_is_file(&self) -> bool;
        fn __almide_s_is_symlink(&self) -> bool;
    }

    impl FsScopedPathExt for std::path::Path {
        fn __almide_s_exists(&self) -> bool {
            exists(self)
        }
        fn __almide_s_is_dir(&self) -> bool {
            is_dir(self)
        }
        fn __almide_s_is_file(&self) -> bool {
            is_file(self)
        }
        fn __almide_s_is_symlink(&self) -> bool {
            is_symlink(self)
        }
    }

    impl FsScopedPathExt for std::fs::Metadata {
        fn __almide_s_exists(&self) -> bool {
            true
        }
        fn __almide_s_is_dir(&self) -> bool {
            self.is_dir()
        }
        fn __almide_s_is_file(&self) -> bool {
            self.is_file()
        }
        fn __almide_s_is_symlink(&self) -> bool {
            self.file_type().is_symlink()
        }
    }
}
