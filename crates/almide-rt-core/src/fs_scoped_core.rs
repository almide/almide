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
// No path is used once a call is decided. The metadata of an entry the
// process may not open is taken by descriptor too (`meta_at`: O_PATH on
// Linux, fstatat on macOS, an attributes-only NtCreateFile on Windows). An
// operation on a granted root ITSELF (the empty
// segment list) runs on the root's held descriptor — its metadata, listing,
// opens — or, for the ops that act on the root's NAME (remove, rename), from
// the root's parent directory's descriptor after the entry there is checked
// to be the held root (same device and inode; on Windows volume serial and
// file index), so a root swapped after the
// scope opened it is refused rather than acted on. The root's own path is
// the trust anchor, as a preopened directory's is: it is opened by its
// canonical path when first needed, and again after the root itself was
// removed or replaced by a rename. On Windows the walk is NtCreateFile
// relative to the held directory handle, a reparse point (symlink, junction)
// refused as a symlink is, and a decided segment that is no single NT name
// (a `:stream`) is the scope's refusal (#4293). On a platform without the
// descriptor calls (other than Linux, macOS and Windows) every scoped call
// is refused
// (`fs_beneath_unsupported`), and the toolchain refuses `[permissions] fs`
// there before it builds (`fs_scope_platform_check`): no check-then-call
// path remains for a rewrite to race.
//
// Splice discipline (lib.rs): no `crate::` paths, no `use` lines; the inline
// `mod almide_sfs` is the facade both consumers name.

include!("fs_scope.rs");
// The bounded, regular-file-only whole-file read (#4201) `almide_sfs::read_bounded`
// makes, shared with almide-base (whose fs_errno spells its texts for the wasm
// renderers); the committed snapshot in src/generated/ (build.rs, #3361).
include!("generated/fs_read_bounded.rs");

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
    /// The root's descriptor, opened on its first use and held until the
    /// root itself is removed or renamed; a root that does not exist yet is
    /// retried at the next call.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    held: std::sync::Mutex<Option<almide_fs_beneath_sys::Handle>>,
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
                    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
                    held: std::sync::Mutex::new(None),
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
        // A segment that is no single name on this OS (on Windows an
        // alternate data stream, `a.txt:s`) is refused as an escape is.
        if !segs.iter().all(|seg| fs_beneath_segment_is_one_name(seg)) {
            return Err(fs_scoped_refused());
        }
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

/// The descriptor half: Linux, macOS and Windows.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
impl FsScoped {
    /// The held descriptor of root `i` (a duplicate), opened by its
    /// canonical path on the first call that needs it.
    fn held(&self, i: usize) -> std::io::Result<almide_fs_beneath_sys::Handle> {
        let mut slot = self.roots[i].held.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(fd) = slot.as_ref() {
            return fd.try_clone();
        }
        let fd = almide_fs_beneath_sys::open_dir_path(&self.root_path(i))?;
        let out = fd.try_clone();
        *slot = Some(fd);
        out
    }

    /// Forget root `i`'s descriptor: the root itself was removed or renamed.
    fn release(&self, i: usize) {
        *self.roots[i].held.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    /// The root `i` as a NAME in its parent: the parent's descriptor and the
    /// root's last name, for the ops that act on the name (remove, rename).
    /// When the scope holds the root, the entry under that name must be the
    /// held directory (same device and inode): a root replaced since is the
    /// refusal, never acted on.
    fn root_entry(&self, i: usize) -> std::io::Result<(almide_fs_beneath_sys::Handle, String)> {
        let p = self.root_path(i);
        let (Some(parent), Some(name)) = (p.parent(), p.file_name()) else {
            return Err(fs_scoped_refused());
        };
        let name = name.to_string_lossy().into_owned();
        let pfd = almide_fs_beneath_sys::open_dir_path(parent)?;
        let held = self.roots[i].held.lock().unwrap_or_else(|p| p.into_inner()).as_ref().map(almide_fs_beneath_sys::Handle::try_clone);
        if let Some(held) = held {
            let held = almide_fs_beneath_sys::identity(&std::fs::File::from(held?))?;
            let here = almide_fs_beneath_sys::meta_at(&pfd, &name)?;
            if here.is_symlink() || here.identity() != held {
                return Err(fs_scoped_refused());
            }
        }
        Ok((pfd, name))
    }

    /// The directory `<root i>/<segs...>`, walked from the held root
    /// without following a symlink; a symlink met is the refusal.
    fn dir(&self, i: usize, segs: &[String]) -> std::io::Result<almide_fs_beneath_sys::Handle> {
        let root = self.held(i)?;
        if segs.is_empty() {
            return Ok(root);
        }
        let v: Vec<&str> = segs.iter().map(String::as_str).collect();
        let r = fs_beneath_imp::parent_from(&root, &v, false, &mut |_| {});
        r.map_err(fs_scoped_from_beneath)
    }

    /// `last` under `dir`, opened as `how` says, never through a symlink.
    fn open_last(dir: &almide_fs_beneath_sys::Handle, last: &str, how: almide_fs_beneath_sys::Open) -> std::io::Result<std::fs::File> {
        almide_fs_beneath_sys::open_at(dir, last, how)
            .map(std::fs::File::from)
            .map_err(|e| fs_scoped_from_beneath(fs_beneath_imp::classify(dir, last, e)))
    }

    /// `path` opened as `how` says (symlinks resolved by the decision, none
    /// followed by the call).
    fn open(&self, path: &str, how: almide_fs_beneath_sys::Open) -> std::io::Result<std::fs::File> {
        let (i, segs) = self.place(path, true)?.segs()?;
        match segs.split_last() {
            Some((last, parents)) => Self::open_last(&self.dir(i, parents)?, last, how),
            None => self.open_root(i, how),
        }
    }

    /// Root `i` itself opened as `how` says: its held descriptor when the
    /// open only reads, the same entry re-opened (without following a
    /// symlink, and checked to be the held one) when it writes, and its path
    /// only while it does not exist yet — creating it is how it is anchored.
    fn open_root(&self, i: usize, how: almide_fs_beneath_sys::Open) -> std::io::Result<std::fs::File> {
        let held = match self.held(i) {
            Ok(fd) => fd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return fs_scoped_open_path(&self.root_path(i), how),
            Err(e) => return Err(e),
        };
        let held = std::fs::File::from(held);
        match how {
            almide_fs_beneath_sys::Open::Dir | almide_fs_beneath_sys::Open::Read | almide_fs_beneath_sys::Open::Meta => Ok(held),
            almide_fs_beneath_sys::Open::CreateNew => Err(almide_fs_beneath_sys::err_exists()),
            _ => {
                let (pfd, name) = self.root_entry(i)?;
                let f = Self::open_last(&pfd, &name, how)?;
                if almide_fs_beneath_sys::identity(&f)? != almide_fs_beneath_sys::identity(&held)? {
                    return Err(fs_scoped_refused());
                }
                Ok(f)
            }
        }
    }

    /// The directory `path` as a descriptor (a rooted call's root).
    pub fn open_dir(&self, path: &str) -> std::io::Result<almide_fs_beneath_sys::Handle> {
        let (i, segs) = self.place(path, true)?.segs()?;
        self.dir(i, &segs)
    }

    /// `path`'s metadata, its symlinks followed by the decision.
    pub fn metadata(&self, path: &str) -> std::io::Result<FsScopedMeta> {
        let (i, segs) = self.place(path, true)?.segs()?;
        let Some((last, parents)) = segs.split_last() else {
            return std::fs::File::from(self.held(i)?).metadata().map(FsScopedMeta::from);
        };
        let dir = self.dir(i, parents)?;
        match Self::open_last(&dir, last, almide_fs_beneath_sys::Open::Meta) {
            Ok(f) => f.metadata().map(FsScopedMeta::from),
            // An entry the process may not open: described by descriptor,
            // its last name not followed (a symlink there is the refusal).
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !fs_scoped_refused_in(&e.to_string()) => {
                match almide_fs_beneath_sys::meta_at(&dir, last) {
                    Ok(m) if m.is_symlink() => Err(fs_scoped_refused()),
                    Ok(m) => Ok(m),
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
                almide_fs_beneath_sys::open_at(&dir, last, almide_fs_beneath_sys::Open::Dir)
                    .map_err(|e| fs_scoped_from_beneath(fs_beneath_imp::classify(&dir, last, e)))?
            }
            None => self.dir(i, &[])?,
        };
        almide_fs_beneath_sys::list_fd(fd)
    }

    /// `std::fs::create_dir_all`, one segment at a time from the root: a
    /// missing segment is created, an existing one opened without following.
    pub fn create_dir_all(&self, path: &str) -> std::io::Result<()> {
        // A `..` over a missing directory is fine here: the plain call
        // creates it on its way, as the lexical rest does.
        let place = self.place(path, true)?;
        let (i, segs) = (place.root, place.segs);
        if segs.is_empty() {
            // The root itself: held (it exists) — a directory is done, any
            // other entry is the plain call's EEXIST; missing, it is created
            // by its path, which is how a root is anchored.
            return match self.held(i) {
                Ok(fd) => {
                    if std::fs::File::from(fd).metadata()?.is_dir() { Ok(()) } else { Err(almide_fs_beneath_sys::err_exists()) }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir_all(self.root_path(i)),
                Err(e) => Err(e),
            };
        }
        let mut cur = self.dir(i, &[])?;
        for (k, seg) in segs.iter().enumerate() {
            let open = |cur: &almide_fs_beneath_sys::Handle| almide_fs_beneath_sys::open_at(cur, seg, almide_fs_beneath_sys::Open::Dir);
            let next = match open(&cur) {
                Ok(fd) => fd,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    match almide_fs_beneath_sys::mkdir_at(&cur, seg, 0o777) {
                        Ok(()) => {}
                        Err(m) if m.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(m) => return Err(m),
                    }
                    open(&cur).map_err(|e| fs_scoped_from_beneath(fs_beneath_imp::classify(&cur, seg, e)))?
                }
                Err(e) => {
                    if let Ok(true) = almide_fs_beneath_sys::symlink_at(&cur, seg) {
                        return Err(fs_scoped_refused());
                    }
                    // Not a directory: the plain call's mkdir answers EEXIST
                    // at the last segment and ENOTDIR on the way.
                    return Err(if almide_fs_beneath_sys::is_not_dir(&e) {
                        if k + 1 == segs.len() { almide_fs_beneath_sys::err_exists() } else { almide_fs_beneath_sys::err_not_dir() }
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
            let (pfd, name) = self.root_entry(i)?;
            almide_fs_beneath_sys::unlink_at(&pfd, &name, dir)?;
            self.release(i);
            return Ok(());
        };
        almide_fs_beneath_sys::unlink_at(&self.dir(i, parents)?, last, dir)
    }

    /// `std::fs::remove_dir_all`: the tree under `path`, walked by
    /// descriptor; a symlink anywhere is removed itself, never followed.
    pub fn remove_dir_all(&self, path: &str) -> std::io::Result<()> {
        let (i, segs) = self.place(path, false)?.segs()?;
        let Some((last, parents)) = segs.split_last() else {
            let (pfd, name) = self.root_entry(i)?;
            fs_scoped_remove_entry(&pfd, &name)?;
            self.release(i);
            return Ok(());
        };
        let dir = self.dir(i, parents)?;
        fs_scoped_remove_entry(&dir, last)
    }

    /// `std::fs::rename` of the entry `from` (not followed) to `to`.
    pub fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        let (i, a) = self.place(from, false)?.segs()?;
        let (j, b) = self.place(to, false)?.segs()?;
        // Each side as a NAME in a held directory: its parent below the root,
        // or, for a root itself, the root's parent (checked to hold it).
        let side = |k: usize, segs: &[String]| -> std::io::Result<(almide_fs_beneath_sys::Handle, String)> {
            match segs.split_last() {
                Some((last, parents)) => Ok((self.dir(k, parents)?, last.clone())),
                None => self.root_entry(k),
            }
        };
        let ((da, la), (db, lb)) = (side(i, &a)?, side(j, &b)?);
        almide_fs_beneath_sys::rename_at(&da, &la, &db, &lb)?;
        if a.is_empty() {
            self.release(i);
        }
        if b.is_empty() {
            self.release(j);
        }
        Ok(())
    }

    /// A NEW file at `path` (O_EXCL, 0o600): an existing entry there — a
    /// symlink included, never followed — is EEXIST.
    pub fn create_new(&self, path: &str) -> std::io::Result<std::fs::File> {
        let (i, segs) = self.place(path, false)?.segs()?;
        let Some((last, parents)) = segs.split_last() else {
            return Err(almide_fs_beneath_sys::err_exists());
        };
        let dir = self.dir(i, parents)?;
        almide_fs_beneath_sys::open_at(&dir, last, almide_fs_beneath_sys::Open::CreateNew).map(std::fs::File::from)
    }

    /// A NEW directory at `path` with `mode`: an existing entry is EEXIST.
    pub fn create_dir(&self, path: &str, mode: u16) -> std::io::Result<()> {
        let (i, segs) = self.place(path, false)?.segs()?;
        let Some((last, parents)) = segs.split_last() else {
            return Err(almide_fs_beneath_sys::err_exists());
        };
        almide_fs_beneath_sys::mkdir_at(&self.dir(i, parents)?, last, mode)
    }

    /// Whether `path` itself is a symlink (its last name not followed).
    pub fn is_symlink(&self, path: &str) -> bool {
        let Ok((i, segs)) = self.place(path, false).and_then(FsScopedPlace::segs) else { return false };
        let Some((last, parents)) = segs.split_last() else {
            // The root as a name in its parent (unchecked: a predicate).
            let p = self.root_path(i);
            let (Some(parent), Some(name)) = (p.parent(), p.file_name()) else { return false };
            let Ok(pfd) = almide_fs_beneath_sys::open_dir_path(parent) else { return false };
            return matches!(almide_fs_beneath_sys::symlink_at(&pfd, &name.to_string_lossy()), Ok(true));
        };
        self.dir(i, parents).is_ok_and(|d| matches!(almide_fs_beneath_sys::symlink_at(&d, last), Ok(true)))
    }
}

/// A root itself (the empty segment list), opened by its canonical path.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn fs_scoped_open_path(p: &std::path::Path, how: almide_fs_beneath_sys::Open) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    match how {
        almide_fs_beneath_sys::Open::Dir | almide_fs_beneath_sys::Open::Read | almide_fs_beneath_sys::Open::Meta => o.read(true),
        almide_fs_beneath_sys::Open::WriteTrunc => o.write(true).create(true).truncate(true),
        almide_fs_beneath_sys::Open::Append => o.append(true).create(true),
        almide_fs_beneath_sys::Open::CreateNew => o.write(true).create_new(true),
    };
    o.open(p)
}

/// Remove `name` under `dir`: a symlink or file unlinked, a directory
/// emptied by descriptor first.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn fs_scoped_remove_entry(dir: &almide_fs_beneath_sys::Handle, name: &str) -> std::io::Result<()> {
    let d = dir;
    if let Ok(true) = almide_fs_beneath_sys::symlink_at(d, name) {
        return almide_fs_beneath_sys::unlink_at(d, name, false);
    }
    match almide_fs_beneath_sys::open_at(d, name, almide_fs_beneath_sys::Open::Dir) {
        Ok(child) => {
            for n in almide_fs_beneath_sys::list_fd(child.try_clone()?)? {
                match fs_scoped_remove_entry(&child, &n.to_string_lossy()) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    r => r?,
                }
            }
            almide_fs_beneath_sys::unlink_at(d, name, true)
        }
        // A file (or an entry swapped for a symlink since the check): the
        // entry itself goes, never what it points at.
        Err(e) if almide_fs_beneath_sys::is_not_a_level(&e) => almide_fs_beneath_sys::unlink_at(d, name, false),
        Err(e) => Err(e),
    }
}

/// A walk's `Escape` is the scope's refusal; any other error is the call's.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn fs_scoped_from_beneath(e: FsBeneathError) -> std::io::Error {
    match e {
        FsBeneathError::Escape => fs_scoped_refused(),
        FsBeneathError::Io(e) => e,
    }
}

/// Elsewhere: no descriptor calls, so every scoped call is refused, inside
/// the scope or not (the toolchain refuses the scope before a build there
/// too). The path-based
/// check this replaced decided and then made the plain call, which a
/// concurrent rewrite could race.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
impl FsScoped {
    pub fn open_with(&self, _path: &str, _o: &std::fs::OpenOptions) -> std::io::Result<std::fs::File> {
        Err(fs_beneath_unsupported())
    }
    pub fn metadata(&self, _path: &str) -> std::io::Result<FsScopedMeta> {
        Err(fs_beneath_unsupported())
    }
    pub fn read_dir_names(&self, _path: &str) -> std::io::Result<Vec<std::ffi::OsString>> {
        Err(fs_beneath_unsupported())
    }
    pub fn create_dir_all(&self, _path: &str) -> std::io::Result<()> {
        Err(fs_beneath_unsupported())
    }
    pub fn remove(&self, _path: &str, _dir: bool) -> std::io::Result<()> {
        Err(fs_beneath_unsupported())
    }
    pub fn remove_dir_all(&self, _path: &str) -> std::io::Result<()> {
        Err(fs_beneath_unsupported())
    }
    pub fn rename(&self, _from: &str, _to: &str) -> std::io::Result<()> {
        Err(fs_beneath_unsupported())
    }
    pub fn is_symlink(&self, _path: &str) -> bool {
        false
    }
    pub fn create_new(&self, _path: &str) -> std::io::Result<std::fs::File> {
        Err(fs_beneath_unsupported())
    }
    pub fn create_dir(&self, _path: &str, _mode: u16) -> std::io::Result<()> {
        Err(fs_beneath_unsupported())
    }
}

/// The metadata a scoped call answers (`almide_sfs::metadata`): the fields
/// the runtime reads, from a `std::fs::Metadata` or, for an entry the process
/// may not open, from a descriptor-relative stat (`meta_at`).
#[derive(Debug, Clone)]
pub struct FsScopedMeta {
    len: u64,
    /// 1 file, 2 directory, 3 symlink, 0 anything else.
    kind: u8,
    modified: Option<std::time::SystemTime>,
    /// Device and inode (Windows: volume serial and file index, filled by
    /// `meta_at`; (0, 0) where the platform does not say).
    identity: (u64, u64),
}

impl FsScopedMeta {
    /// From a `st_mode` (its S_IFMT bits), a size, a modification time and
    /// the device and inode.
    pub fn from_mode(mode: u32, len: u64, modified: Option<std::time::SystemTime>, identity: (u64, u64)) -> Self {
        let kind = match mode & 0o170000 {
            0o100000 => 1,
            0o040000 => 2,
            0o120000 => 3,
            _ => 0,
        };
        FsScopedMeta { len, kind, modified, identity }
    }
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn is_file(&self) -> bool {
        self.kind == 1
    }
    pub fn is_dir(&self) -> bool {
        self.kind == 2
    }
    pub fn is_symlink(&self) -> bool {
        self.kind == 3
    }
    pub fn modified(&self) -> std::io::Result<std::time::SystemTime> {
        self.modified.ok_or_else(|| std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
    /// Device and inode, when the platform says (unix).
    pub fn identity(&self) -> (u64, u64) {
        self.identity
    }
}

impl From<std::fs::Metadata> for FsScopedMeta {
    fn from(m: std::fs::Metadata) -> Self {
        let t = m.file_type();
        let kind = if t.is_symlink() { 3 } else if t.is_dir() { 2 } else if t.is_file() { 1 } else { 0 };
        #[cfg(unix)]
        let identity = (std::os::unix::fs::MetadataExt::dev(&m), std::os::unix::fs::MetadataExt::ino(&m));
        #[cfg(not(unix))]
        let identity = (0, 0);
        FsScopedMeta { len: m.len(), kind, modified: m.modified().ok(), identity }
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

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
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

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
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

    /// The whole-file read the `fs.read_*` family makes (#4201): a regular
    /// file only (a device, FIFO or socket errs `Not a regular file`, a
    /// directory its read's EISDIR), at most `FS_READ_MAX_BYTES`. The entry is
    /// opened O_NONBLOCK and judged by its descriptor's fstat; scoped, the
    /// open is the scope's no-follow descriptor walk.
    pub fn read_bounded<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<Vec<u8>> {
        match scope() {
            None => super::fs_read_bounded_path(p.as_ref(), super::FS_READ_MAX_BYTES),
            Some(sc) => super::fs_read_bounded_file(&mut open_bounded(&sc, p.as_ref())?, super::FS_READ_MAX_BYTES),
        }
    }

    /// [`read_bounded`] as UTF-8 text (`read_to_string`'s invalid-UTF-8 error).
    pub fn read_to_string_bounded<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<String> {
        read_bounded(p).and_then(super::fs_read_bounded_text)
    }

    /// The scoped read-only open the bounded reader makes: `Meta` is the
    /// O_NONBLOCK read-only open of the last segment, never through a link.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    fn open_bounded(sc: &Scoped, p: &std::path::Path) -> std::io::Result<std::fs::File> {
        sc.open(&s(p), super::almide_fs_beneath_sys::Open::Meta)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    fn open_bounded(sc: &Scoped, p: &std::path::Path) -> std::io::Result<std::fs::File> {
        let mut o = std::fs::OpenOptions::new();
        o.read(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::custom_flags(&mut o, super::FS_READ_O_NONBLOCK);
        sc.open_with(&s(p), &o)
    }

    pub fn write<P: AsRef<std::path::Path>, C: AsRef<[u8]>>(p: P, data: C) -> std::io::Result<()> {
        if scope().is_none() {
            return std::fs::write(p, data);
        }
        std::io::Write::write_all(&mut File::create(p)?, data.as_ref())
    }

    /// `std::fs::metadata`'s answer as [`super::FsScopedMeta`], whose `len`,
    /// `is_dir`, `is_file` and `modified` are the ones the runtime reads.
    pub fn metadata<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<super::FsScopedMeta> {
        match scope() {
            None => std::fs::metadata(p).map(super::FsScopedMeta::from),
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
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn open_dir(p: &str) -> std::io::Result<super::almide_fs_beneath_sys::Handle> {
        match scope() {
            None => super::almide_fs_beneath_sys::open_dir_path(std::path::Path::new(p)),
            Some(sc) => sc.open_dir(p),
        }
    }

    /// A NEW file, private to its owner (O_CREAT | O_EXCL, 0o600 on unix):
    /// an existing entry at `p` — a symlink included, never followed — is
    /// EEXIST.
    pub fn create_new_file<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<std::fs::File> {
        match scope() {
            None => {
                let mut o = std::fs::OpenOptions::new();
                o.write(true).create_new(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
                o.open(p)
            }
            Some(sc) => sc.create_new(&s(p.as_ref())),
        }
    }

    /// A NEW directory with `mode` on unix (not recursive): an existing
    /// entry at `p` is EEXIST.
    pub fn create_dir_with_mode<P: AsRef<std::path::Path>>(p: P, mode: u16) -> std::io::Result<()> {
        match scope() {
            None => {
                // Only unix sets a mode on the builder.
                #[cfg_attr(not(unix), allow(unused_mut))]
                let mut b = std::fs::DirBuilder::new();
                #[cfg(unix)]
                std::os::unix::fs::DirBuilderExt::mode(&mut b, u32::from(mode));
                let _ = mode;
                b.create(p)
            }
            Some(sc) => sc.create_dir(&s(p.as_ref()), mode),
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

    impl FsScopedPathExt for super::FsScopedMeta {
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
            self.is_symlink()
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

/// `dir/rel` as `Path::join` spells it (`dir` itself for an empty `rel`):
/// the path a walk's entry and its error name.
pub fn fs_walk_join(dir: &str, rel: &str) -> String {
    if rel.is_empty() { dir.to_string() } else { std::path::Path::new(dir).join(rel).to_string_lossy().into_owned() }
}

/// The entries under the directory `dir`, each as its `/`-joined path
/// relative to `dir`, in the system's order (#4143): a directory is
/// descended into, a symlink is an entry and never followed — so a link to
/// the outside lists nothing outside and a link loop cannot repeat the walk.
/// `depth` counts the levels still to list (`Some(1)`: `dir` only; `None`:
/// unbounded). The walk holds each directory by descriptor and opens the
/// next one from it with O_NOFOLLOW, so a directory swapped for a symlink
/// mid-walk is not followed either; `dir` itself is opened as `fs.read_dir`
/// would (its own symlinks followed: the program named it), beneath the
/// granted root under `[permissions] fs`. An error names the directory it
/// came from, as `fs_walk_join(dir, rel)`.
pub fn fs_walk_nofollow(dir: &str, depth: Option<usize>) -> Result<Vec<String>, (String, std::io::Error)> {
    let mut out = Vec::new();
    fs_walk_imp(dir, depth, &mut out)?;
    Ok(out)
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn fs_walk_imp(dir: &str, depth: Option<usize>, out: &mut Vec<String>) -> Result<(), (String, std::io::Error)> {
    fn level(fd: &almide_fs_beneath_sys::Handle, rel: &str, depth: Option<usize>, out: &mut Vec<String>) -> Result<(), (String, std::io::Error)> {
        let names = fd.try_clone().and_then(almide_fs_beneath_sys::list_fd).map_err(|e| (rel.to_string(), e))?;
        for n in names {
            let name = n.to_string_lossy().into_owned();
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            out.push(r.clone());
            if depth == Some(1) {
                continue;
            }
            match almide_fs_beneath_sys::open_at(fd, &name, almide_fs_beneath_sys::Open::Dir) {
                Ok(child) => level(&child, &r, depth.map(|d| d - 1), out)?,
                // Not a directory, a symlink (never followed), or gone since
                // the listing: an entry, not a level.
                Err(e) if almide_fs_beneath_sys::is_gone(&e) || almide_fs_beneath_sys::is_not_a_level(&e) => {}
                // A directory the walk may not open: the plain walk's err.
                Err(e) => return Err((r, e)),
            }
        }
        Ok(())
    }
    let root = almide_sfs::open_dir(dir).map_err(|e| (String::new(), e))?;
    level(&root, "", depth, out)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn fs_walk_imp(dir: &str, depth: Option<usize>, out: &mut Vec<String>) -> Result<(), (String, std::io::Error)> {
    fn level(base: &str, rel: &str, depth: Option<usize>, out: &mut Vec<String>) -> Result<(), (String, std::io::Error)> {
        let here = fs_walk_join(base, rel);
        for e in almide_sfs::read_dir(&here).map_err(|e| (rel.to_string(), e))? {
            let e = e.map_err(|e| (rel.to_string(), e))?;
            let name = e.file_name().to_string_lossy().into_owned();
            let r = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            out.push(r.clone());
            let p = fs_walk_join(base, &r);
            if depth != Some(1) && std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir()) {
                level(base, &r, depth.map(|d| d - 1), out)?;
            }
        }
        Ok(())
    }
    level(dir, "", depth, out)
}

/// The random part of a temp name (#4113): 16 hex digits of OS entropy —
/// `/dev/urandom` where it can be read, else std's per-process SipHash keys
/// (themselves drawn from the OS CSPRNG) over a counter and the clock. Never
/// the clock alone: a name another user can predict is a name they can plant.
pub fn fs_temp_suffix() -> String {
    let mut b = [0u8; 8];
    let urandom = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b));
    if urandom.is_err() {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut h = std::hash::BuildHasher::build_hasher(&std::collections::hash_map::RandomState::new());
        std::hash::Hasher::write_u64(&mut h, N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        std::hash::Hasher::write_u128(&mut h, t);
        b = std::hash::Hasher::finish(&h).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A new, uniquely named file (`dir` false, 0o600) or directory (`dir`
/// true, 0o700) in the temp dir, named `<prefix><16 hex of OS entropy>`
/// (#4113). Created exclusively (O_EXCL / a plain mkdir), so an entry
/// already at the name — one another user planted, a symlink included — is
/// never followed, truncated or reused: the next name is tried, up to 64
/// times. Its path, or the error of the last try with that try's path.
pub fn fs_temp_create(prefix: &str, dir: bool) -> Result<String, (String, std::io::Error)> {
    fs_temp_create_in(&std::env::temp_dir(), prefix, dir, &mut fs_temp_suffix)
}

/// [`fs_temp_create`] under `base` with the name source exposed, for the
/// collision tests (tests/fs_scoped_core.rs).
#[doc(hidden)]
pub fn fs_temp_create_in(base: &std::path::Path, prefix: &str, dir: bool, suffix: &mut dyn FnMut() -> String) -> Result<String, (String, std::io::Error)> {
    let mut last = (String::new(), std::io::Error::from_raw_os_error(17));
    for _ in 0..64 {
        let path = base.join(format!("{prefix}{}", suffix()));
        let shown = path.to_string_lossy().replace('\\', "/");
        let r = if dir { almide_sfs::create_dir_with_mode(&path, 0o700) } else { almide_sfs::create_new_file(&path).map(drop) };
        match r {
            Ok(()) => return Ok(shown),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = (shown, e),
            Err(e) => return Err((shown, e)),
        }
    }
    Err(last)
}
