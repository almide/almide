// net_error_core (#3620, the net/tcp design §4): the ONE table that turns a
// socket failure into the text a program sees, on every leg. A failure is
// classified into its `wasi:sockets` `error-code` case first, the way the
// stock WASI 0.3 host classifies it (wasmtime-wasi 47 `p3/sockets/conv.rs`:
// the errno, then the `ErrorKind`; `BrokenPipe` on a send is
// `connection-broken`), and the text is the case's, never the platform's
// errno string. So `tcp_connect(127.0.0.1:1): connection refused` reads the
// same on Linux, macOS, Windows, the embedded host and the p3 component.
//
// Splice discipline (lib.rs): no `crate::` paths, no `mod`, no `use` lines.

/// A `wasi:sockets` `error-code` case, in the order sockets.wit 0.3 declares
/// them (the p3 shim derives its discriminants from the same list).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetErrorClass {
    AccessDenied,
    NotSupported,
    InvalidArgument,
    OutOfMemory,
    Timeout,
    InvalidState,
    AddressNotBindable,
    AddressInUse,
    RemoteUnreachable,
    ConnectionRefused,
    ConnectionBroken,
    ConnectionReset,
    ConnectionAborted,
    DatagramTooLarge,
    Other,
}

/// The class's text: what follows `<fn>: ` in an err.
pub fn net_class_text(c: NetErrorClass) -> &'static str {
    match c {
        NetErrorClass::AccessDenied => "permission denied",
        NetErrorClass::NotSupported => "operation not supported",
        NetErrorClass::InvalidArgument => "invalid argument",
        NetErrorClass::OutOfMemory => "out of memory",
        NetErrorClass::Timeout => "timed out",
        NetErrorClass::InvalidState => "invalid socket state",
        NetErrorClass::AddressNotBindable => "address not available",
        NetErrorClass::AddressInUse => "address already in use",
        NetErrorClass::RemoteUnreachable => "host unreachable",
        NetErrorClass::ConnectionRefused => "connection refused",
        NetErrorClass::ConnectionBroken => "connection broken",
        NetErrorClass::ConnectionReset => "connection reset",
        NetErrorClass::ConnectionAborted => "connection aborted",
        NetErrorClass::DatagramTooLarge => "datagram too large",
        NetErrorClass::Other => "network error",
    }
}

/// The errnos `ErrorKind` leaves uncategorized but the stock host classifies
/// (conv.rs's `Errno` arm). Values are the platform's; a platform not listed
/// classifies them by kind only, i.e. as `Other`.
fn net_errno_class(errno: i32) -> Option<NetErrorClass> {
    #[cfg(target_os = "linux")]
    const ROWS: &[(i32, NetErrorClass)] = &[
        (106, NetErrorClass::InvalidState),     // EISCONN
        (89, NetErrorClass::InvalidState),      // EDESTADDRREQ
        (90, NetErrorClass::DatagramTooLarge),  // EMSGSIZE
        (105, NetErrorClass::OutOfMemory),      // ENOBUFS
        (92, NetErrorClass::NotSupported),      // ENOPROTOOPT
        (96, NetErrorClass::NotSupported),      // EPFNOSUPPORT
        (93, NetErrorClass::NotSupported),      // EPROTONOSUPPORT
        (91, NetErrorClass::NotSupported),      // EPROTOTYPE
        (94, NetErrorClass::NotSupported),      // ESOCKTNOSUPPORT
        (97, NetErrorClass::NotSupported),      // EAFNOSUPPORT
        (95, NetErrorClass::NotSupported),      // EOPNOTSUPP
        (112, NetErrorClass::RemoteUnreachable), // EHOSTDOWN
        (64, NetErrorClass::RemoteUnreachable), // ENONET
    ];
    #[cfg(target_vendor = "apple")]
    const ROWS: &[(i32, NetErrorClass)] = &[
        (56, NetErrorClass::InvalidState),      // EISCONN
        (39, NetErrorClass::InvalidState),      // EDESTADDRREQ
        (40, NetErrorClass::DatagramTooLarge),  // EMSGSIZE
        (55, NetErrorClass::OutOfMemory),       // ENOBUFS
        (42, NetErrorClass::NotSupported),      // ENOPROTOOPT
        (46, NetErrorClass::NotSupported),      // EPFNOSUPPORT
        (43, NetErrorClass::NotSupported),      // EPROTONOSUPPORT
        (41, NetErrorClass::NotSupported),      // EPROTOTYPE
        (44, NetErrorClass::NotSupported),      // ESOCKTNOSUPPORT
        (47, NetErrorClass::NotSupported),      // EAFNOSUPPORT
        (102, NetErrorClass::NotSupported),     // EOPNOTSUPP
        (64, NetErrorClass::RemoteUnreachable), // EHOSTDOWN
    ];
    #[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
    const ROWS: &[(i32, NetErrorClass)] = &[];
    ROWS.iter().find(|(e, _)| *e == errno).map(|(_, c)| *c)
}

/// Classify an io error as the stock host does. `WouldBlock` is a read or
/// write that outlived `SO_RCVTIMEO`/`SO_SNDTIMEO` (Unix answers EAGAIN where
/// Windows answers `TimedOut`); `BrokenPipe` is a send to a closed peer.
pub fn net_classify(e: &std::io::Error) -> NetErrorClass {
    type K = std::io::ErrorKind;
    match e.kind() {
        K::PermissionDenied => NetErrorClass::AccessDenied,
        K::AddrInUse => NetErrorClass::AddressInUse,
        K::AddrNotAvailable => NetErrorClass::AddressNotBindable,
        K::ConnectionAborted => NetErrorClass::ConnectionAborted,
        K::ConnectionRefused => NetErrorClass::ConnectionRefused,
        K::ConnectionReset => NetErrorClass::ConnectionReset,
        K::BrokenPipe => NetErrorClass::ConnectionBroken,
        K::InvalidInput => NetErrorClass::InvalidArgument,
        K::NotConnected => NetErrorClass::InvalidState,
        K::OutOfMemory => NetErrorClass::OutOfMemory,
        K::TimedOut | K::WouldBlock => NetErrorClass::Timeout,
        K::Unsupported => NetErrorClass::NotSupported,
        K::HostUnreachable | K::NetworkUnreachable | K::NetworkDown => NetErrorClass::RemoteUnreachable,
        _ => e.raw_os_error().and_then(net_errno_class).unwrap_or(NetErrorClass::Other),
    }
}

/// `<head>: <class text>` — `head` is `tcp_read`, or `tcp_connect(h:p)`.
pub fn net_io_error_text(head: &str, e: &std::io::Error) -> String {
    format!("{head}: {}", net_class_text(net_classify(e)))
}

/// The texts decided before any transport call, shared so every leg prints
/// them from one place.
pub fn net_err_invalid_handle(kind: &str, h: i64) -> String {
    format!("invalid {kind} handle: {h}")
}
pub fn net_err_closed_handle(kind: &str, h: i64) -> String {
    format!("{kind} handle {h} is closed")
}
pub fn net_err_negative_len(f: &str, len: i64) -> String {
    format!("{f}: negative length {len}")
}
pub fn net_err_timeout_not_positive(ms: i64) -> String {
    format!("tcp_read_timeout: timeout must be positive (got {ms} ms)")
}
pub fn net_err_short_read(got: usize, want: i64) -> String {
    format!("tcp_read_exact: connection closed after {got} of {want} bytes")
}
pub fn net_err_port_range(head: &str) -> String {
    format!("{head}: port out of range 0..=65535")
}
pub fn net_err_unresolvable(f: &str, host: &str) -> String {
    format!("{f}: cannot resolve host \"{host}\"")
}
