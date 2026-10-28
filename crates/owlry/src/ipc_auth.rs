//! Peer authentication for the Unix-socket IPC channel.
//!
//! The socket lives in a user-owned runtime directory, but mode bits alone do
//! not tell a client who is on the other end. Every accepted connection and
//! every client connection therefore checks the peer's effective UID and
//! refuses to talk to a different user.

use std::io;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;

/// Return the effective UID of `stream`'s peer.
#[cfg(target_os = "linux")]
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` and `len` are valid for writes, and the stream owns a
    // live file descriptor for the duration of the call.
    let ret = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(cred.uid)
}

/// Peer credentials are only implemented on Linux in this crate.
#[cfg(not(target_os = "linux"))]
pub fn peer_uid(_stream: &UnixStream) -> io::Result<u32> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "peer credential checks are not implemented on this platform",
    ))
}

/// Refuse a connection unless the peer runs as the same user.
pub fn ensure_same_uid(stream: &UnixStream) -> io::Result<()> {
    let peer = peer_uid(stream)?;
    let ours = unsafe { libc::geteuid() };
    if peer != ours {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("socket peer uid {peer} does not match current uid {ours}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_socket_pair_in_this_process_passes_the_uid_check() {
        let (a, _b) = UnixStream::pair().unwrap();
        ensure_same_uid(&a).unwrap();
        assert_eq!(peer_uid(&a).unwrap(), unsafe { libc::geteuid() });
    }
}
