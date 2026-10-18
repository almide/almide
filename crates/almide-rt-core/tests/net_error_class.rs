//! The tcp error classifier answers the `wasi:sockets` case the stock WASI
//! 0.3 host answers for the same failure (wasmtime-wasi 47
//! `p3/sockets/conv.rs`), so a failure reads the same on every leg. One row
//! per `ErrorKind` the host's table names, plus the kinds a socket call
//! produces that it reaches through the errno instead.
use almide_rt_core::net_core::{net_class_text, net_classify, net_io_error_text, NetErrorClass as C};
use std::io::{Error, ErrorKind as K};

#[test]
fn every_kind_the_host_names_maps_to_its_case() {
    let rows = [
        (K::PermissionDenied, C::AccessDenied),
        (K::AddrInUse, C::AddressInUse),
        (K::AddrNotAvailable, C::AddressNotBindable),
        (K::ConnectionAborted, C::ConnectionAborted),
        (K::ConnectionRefused, C::ConnectionRefused),
        (K::ConnectionReset, C::ConnectionReset),
        (K::BrokenPipe, C::ConnectionBroken),
        (K::InvalidInput, C::InvalidArgument),
        (K::NotConnected, C::InvalidState),
        (K::OutOfMemory, C::OutOfMemory),
        (K::TimedOut, C::Timeout),
        (K::WouldBlock, C::Timeout),
        (K::Unsupported, C::NotSupported),
        (K::HostUnreachable, C::RemoteUnreachable),
        (K::NetworkUnreachable, C::RemoteUnreachable),
        (K::NetworkDown, C::RemoteUnreachable),
        (K::Other, C::Other),
        (K::UnexpectedEof, C::Other),
    ];
    for (kind, class) in rows {
        assert_eq!(net_classify(&Error::from(kind)), class, "{kind:?}");
    }
}

#[cfg(unix)]
#[test]
fn errnos_without_a_kind_classify_as_the_host_does() {
    // EISCONN and EAFNOSUPPORT carry no ErrorKind of their own.
    #[cfg(target_os = "linux")]
    let (eisconn, eafnosupport) = (106, 97);
    #[cfg(target_vendor = "apple")]
    let (eisconn, eafnosupport) = (56, 47);
    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    {
        assert_eq!(net_classify(&Error::from_raw_os_error(eisconn)), C::InvalidState);
        assert_eq!(net_classify(&Error::from_raw_os_error(eafnosupport)), C::NotSupported);
    }
    // An errno std maps to a kind follows the kind: ECONNREFUSED.
    #[cfg(target_os = "linux")]
    let econnrefused = 111;
    #[cfg(target_vendor = "apple")]
    let econnrefused = 61;
    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    assert_eq!(net_classify(&Error::from_raw_os_error(econnrefused)), C::ConnectionRefused);
}

#[test]
fn the_text_is_the_class_never_the_platform_message() {
    let e = Error::new(K::ConnectionRefused, "Connection refused (os error 61)");
    assert_eq!(net_io_error_text("tcp_connect(127.0.0.1:9)", &e), "tcp_connect(127.0.0.1:9): connection refused");
    assert_eq!(net_class_text(C::Other), "network error");
}
