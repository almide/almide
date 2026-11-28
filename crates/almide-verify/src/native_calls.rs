//! The native call-binding checker — the Rust mirror of
//! `proofs/NativeCalls.v`'s `check_native_calls_cert` (#4006, class P).
//!
//! The witness is one block per function of a natively rendered program,
//! `#`-separated: `<modes>|<sites>|<marks>`, a newline, then the function's
//! ownership lines (heap parameters first), with an optional fourth header
//! field `|<hosts>`: one id per native host call (#4006 class E), each an
//! index into [`NATIVE_FS_HOST_CALLS`] followed by [`NATIVE_UNIX_HOST_CALLS`] and
//! [`NATIVE_ROW_HOST_CALLS`]. The program's signature table is
//! the list of every block's modes. A block is accepted iff its modes are 0
//! or 1, every site agrees with its callee block's modes, the marks are as
//! many as the arguments the sites move, each names a line of the block, and
//! every line holds exactly as many `M` bytes as marks name it, and its lines
//! balance from the references their modes declare, and every host id names a
//! function of the named set.

use crate::nat::{pnats, split_bar, split_semi, Nat};

struct Block<'a> {
    modes: Vec<Nat>,
    sites: Vec<Vec<Nat>>,
    marks: Vec<Nat>,
    hosts: Vec<Nat>,
    body: &'a [u8],
}

/// `split_first is_nl`: the header before the first newline, and the rest.
fn split_nl(b: &[u8]) -> (&[u8], &[u8]) {
    match b.iter().position(|&c| c == b'\n') {
        Some(i) => (&b[..i], &b[i + 1..]),
        None => (b, &[]),
    }
}

/// `parse_block`.
fn parse_block(b: &[u8]) -> Block<'_> {
    let (header, body) = split_nl(b);
    let (modes, rest) = split_bar(header);
    let (sites, rest) = split_bar(rest);
    let (marks, hosts) = split_bar(rest);
    Block {
        modes: pnats(modes),
        sites: split_semi(sites).into_iter().map(pnats).filter(|s| !s.is_empty()).collect(),
        marks: pnats(marks),
        hosts: pnats(hosts),
        body,
    }
}

/// `native_fs_host_calls` (NativeCalls.v): the trusted base's named set of
/// native fs host calls, in the order of `almide_mir::host_ops`'s list
/// (tests/native_fs_host_calls_test.rs holds the copies to one list).
pub const NATIVE_FS_HOST_CALLS: &[&str] = &[
    "almide_rt_fs_read_text",
    "almide_rt_fs_read_bytes",
    "almide_rt_fs_read_lines",
    "almide_rt_fs_read_text_if_exists",
    "almide_rt_fs_list_dir",
    "almide_rt_fs_exists",
    "almide_rt_fs_is_dir",
    "almide_rt_fs_is_file",
    "almide_rt_fs_is_symlink",
    "almide_rt_fs_walk",
    "almide_rt_fs_glob",
    "almide_rt_fs___root_read_text",
    "almide_rt_fs___root_read_bytes",
    "almide_rt_fs___root_list_dir",
    "almide_rt_fs___root_exists",
    "almide_rt_fs_temp_dir",
    "almide_rt_fs_write",
    "almide_rt_fs_write_bytes",
    "almide_rt_fs_mkdir_p",
    "almide_rt_fs_remove_all",
    "almide_rt_fs___root_write_text",
    "almide_rt_fs_rename",
    "almide_rt_fs_copy",
    "almide_rt_fs_append",
    "almide_rt_fs_append_bytes",
    "almide_rt_fs_remove",
    "almide_rt_fs___root_remove",
    "almide_rt_fs_create_temp_dir",
    "almide_rt_fs_create_temp_file",
    "almide_rt_fs_read_bytes_if_exists",
    "almide_rt_fs_read_lines_if_exists",
    "almide_rt_fs_stat",
    "almide_rt_fs_fold_lines",
    "almide_rt_fs_for_each_line",
    "almide_rt_fs_fold_lines_effect",
    "almide_rt_fs_for_each_line_effect",
    "almide_rt_fs_fold_lines_range",
    "almide_rt_fs_fold_lines_chunked",
];

/// `native_unix_host_calls` (NativeCalls.v): the trusted base's named set of
/// native unix host calls, in the order of `almide_mir::host_ops`'s list.
pub const NATIVE_UNIX_HOST_CALLS: &[&str] = &[
    "almide_rt_unix___os_connect",
    "almide_rt_unix___os_listen",
    "almide_rt_unix___os_accept",
    "almide_rt_unix___os_send",
    "almide_rt_unix___os_recv",
    "almide_rt_unix___os_close",
    "almide_rt_unix___os_wait",
    "almide_rt_unix___os_take_fds",
    "almide_rt_unix___os_shm_create",
    "almide_rt_unix___os_shm_resize",
    "almide_rt_unix___os_shm_write",
    "almide_rt_unix___os_shm_read",
];

/// `native_row_host_calls` (NativeCalls.v): every other capability module's
/// host ops, in the order `almide_mir::host_ops::native_row_host_calls`
/// derives them from the host-op table (tests/native_fs_host_calls_test.rs
/// holds the copies to that derivation).
pub const NATIVE_ROW_HOST_CALLS: &[&str] = &[
    "almide_rt_datetime_now",
    "almide_rt_datetime_monotonic_ns",
    "almide_rt_datetime_now_ms",
    "almide_rt_env_args",
    "almide_rt_env_get",
    "almide_rt_env___set",
    "almide_rt_env___environ",
    "almide_rt_env_cwd",
    "almide_rt_env_sleep_ms",
    "almide_http_get",
    "almide_http_request",
    "almide_http_get_response",
    "almide_http_request_response",
    "almide_rt_http___call_start",
    "almide_http_call_poll",
    "almide_http_call_read_new",
    "almide_http_call_wait",
    "almide_http_call_cancel",
    "almide_rt_http_serve",
    "almide_rt_io_print",
    "almide_rt_io_eprint",
    "almide_rt_io___write_stdout",
    "almide_rt_io___write_stderr",
    "almide_rt_io_read_line",
    "almide_rt_io_read_all",
    "almide_rt_io___read_stdin_bytes",
    "almide_rt_net___os_tcp_connect",
    "almide_rt_net___os_tcp_read",
    "almide_rt_net___os_tcp_write",
    "almide_rt_net___os_tcp_close",
    "almide_rt_net___os_tcp_is_open",
    "almide_rt_net___os_tcp_read_timeout",
    "almide_rt_net___os_tcp_set_timeout",
    "almide_rt_net___os_tcp_available",
    "almide_rt_net___os_tcp_listen",
    "almide_rt_net___os_tcp_local_port",
    "almide_rt_net___os_tcp_accept",
    "almide_rt_net___os_tcp_close_listener",
    "almide_rt_net___os_udp_bind",
    "almide_rt_net___os_udp_local_port",
    "almide_rt_net___os_udp_send_to",
    "almide_rt_net___os_udp_recv_from",
    "almide_rt_net___os_udp_close",
    "almide_rt_net___os_lookup_host",
    "almide_rt_process_exec",
    "almide_rt_process_kill",
    "almide_rt_process___os_output",
    "almide_rt_process___os_run",
    "almide_rt_process___os_start",
    "almide_rt_process___os_poll",
    "almide_rt_process___os_wait",
    "almide_rt_process___os_stop",
    "almide_rt_process___os_read_new",
    "almide_rt_process___os_child_pid",
    "almide_rt_process_pid",
    "almide_rt_random_int",
    "almide_rt_random_float",
    "almide_rt_random_choice",
    "almide_rt_random_shuffle",
    "almide_rt_random_bytes",
    "almide_rt_test_abort_site_run",
    "almide_rt_test_abort_site_selected",
    "almide_rt_uuid___v7_stamp",
];

/// `hosts_ok`: every host id names a function of `native_host_calls`, the
/// fs set, then the unix set, then the row calls.
fn hosts_ok(b: &Block<'_>) -> bool {
    let named = NATIVE_FS_HOST_CALLS.len() + NATIVE_UNIX_HOST_CALLS.len() + NATIVE_ROW_HOST_CALLS.len();
    b.hosts.iter().all(|h| h.as_index().is_some_and(|i| i < named))
}

/// `site_ok` (CallModes.v).
fn site_ok(sigs: &[&[Nat]], site: &[Nat]) -> bool {
    match site.split_first() {
        Some((callee, actual)) => callee.as_index().and_then(|i| sigs.get(i)).is_some_and(|sig| *sig == actual),
        None => false,
    }
}

/// `site_moves`: the arguments a site moves (its `1` modes).
fn site_moves(site: &[Nat]) -> usize {
    site.split_first().map_or(0, |(_, actual)| actual.iter().filter(|m| m.as_index() == Some(1)).count())
}

/// `marks_ok`.
fn marks_ok(b: &Block<'_>) -> bool {
    let raw: Vec<&[u8]> = b.body.split(|&c| c == b'\n').collect();
    let moved: usize = b.sites.iter().map(|s| site_moves(s)).sum();
    let idx: Vec<Option<usize>> = b.marks.iter().map(Nat::as_index).collect();
    b.marks.len() == moved
        && idx.iter().all(|j| j.is_some_and(|j| j < raw.len()))
        && raw
            .iter()
            .enumerate()
            .all(|(j, l)| l.iter().filter(|&&c| c == b'M').count() == idx.iter().filter(|m| **m == Some(j)).count())
}

/// `check_native_calls_cert`.
pub(crate) fn check_native_calls_cert(witness: &[u8]) -> bool {
    let blocks: Vec<Block<'_>> = witness.split(|&c| c == b'#').map(parse_block).collect();
    let sigs: Vec<&[Nat]> = blocks.iter().map(|b| b.modes.as_slice()).collect();
    blocks.iter().all(|b| {
        b.modes.iter().all(Nat::le_one)
            && b.sites.iter().all(|s| site_ok(&sigs, s))
            && marks_ok(b)
            && hosts_ok(b)
            && crate::ownership::native_lines_ok(&b.modes, b.body)
    })
}

#[cfg(test)]
mod tests {
    use super::check_native_calls_cert as c;

    #[test]
    fn the_coq_examples_verdict_for_verdict() {
        // proofs/NativeCalls.v's examples.
        assert!(c(b"|1 1|0\niM\nid#1||\nm"));
        assert!(c(b"|1 0|\nid\nid#0||\nam"));
        assert!(!c(b"|1 1|0\nid\nid#1||\nm"));
        assert!(!c(b"|1 0|0\niM\nid#0||\nam"));
        assert!(!c(b"|1 0|\nid\nid#1||\nm"));
        assert!(!c(b"|1 1|0\niMb\nid#1||\nm"));
        assert!(!c(b"|1 1|0\niM\nid#1||\n"));
        assert!(!c(b"|1 1|0\niM\nid#1||\ndam"));
        assert!(!c(b"|5 0|\nid"));
        assert!(c(b"||| 0\nid"));
        assert!(c(b"||| 29\nid"));
        assert!(c(b"||| 30 31\nid"));
        assert!(c(b"||| 32 33 34 35 36 37\nid"));
        assert!(c(b"||| 38 43 49\nid"));
        assert!(c(b"||| 50 76 112\nid"));
        assert!(!c(b"||| 113\nid"));
    }
}
