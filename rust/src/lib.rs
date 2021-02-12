use nix;
use nix::errno::Errno;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult};
use nix::Error::Sys;
use os_pipe::pipe;
use std::convert::TryInto;
use std::io;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::result::Result;
use thiserror::Error;

mod child {
    // Everything here must follow the restrictions of post-fork in a multi-threaded process
    use libc::{_exit, c_char, execvp, STDIN_FILENO, STDOUT_FILENO};
    use nix::errno::Errno;
    use nix::unistd::{dup2, fork, ForkResult};
    use os_pipe::PipeWriter;
    use std::convert::Infallible;
    use std::ffi::OsStr;
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::UnixStream;
    use std::ptr;

    #[repr(u8)]
    pub enum NewErrorCause {
        SetStdin = 0,
        SetStdout = 1,
        Exec = 2,
        DoubleFork = 3,
    }
    impl NewErrorCause {
        // TODO roundtripping proptest
        pub fn from_u8(c: u8) -> Option<Self> {
            match c {
                0 => Some(Self::SetStdin),
                1 => Some(Self::SetStdout),
                2 => Some(Self::Exec),
                3 => Some(Self::DoubleFork),
                _ => None,
            }
        }
    }

    struct NewError {
        cause: NewErrorCause,
        errno: Errno,
    }

    fn grandchild_wrapped(child_sock: UnixStream) -> Result<Infallible, NewError> {
        dup2(child_sock.as_raw_fd(), STDIN_FILENO).map_err(|e| NewError {
            cause: NewErrorCause::SetStdin,
            errno: e.as_errno().unwrap_or(Errno::UnknownErrno),
        })?;

        dup2(child_sock.as_raw_fd(), STDOUT_FILENO).map_err(|e| NewError {
            cause: NewErrorCause::SetStdout,
            errno: e.as_errno().unwrap_or(Errno::UnknownErrno),
        })?;

        let args: [*const c_char; 7] = [
            OsStr::new("nix\0").as_bytes().as_ptr() as *const c_char,
            OsStr::new("--extra-experimental-features\0").as_bytes().as_ptr() as *const c_char,
            OsStr::new("nix-command\0").as_bytes().as_ptr() as *const c_char,
            OsStr::new("--extra-plugin-files\0").as_bytes().as_ptr() as *const c_char,
            OsStr::new(concat!(env!("NIX_FFI_PLUGIN_PREFIX", "you must set the NIX_FFI_PLUGIN_PREFIX environment variable to point to the installation prefix of the nix-ffi plugin"), "/lib/nix/plugins\0")).as_bytes().as_ptr() as *const c_char,
            OsStr::new("ffi-helper\0").as_bytes().as_ptr() as *const c_char,
            ptr::null(),
        ];

        unsafe {
            execvp(args[0], args.as_ptr());
        }
        Err(NewError {
            cause: NewErrorCause::Exec,
            errno: Errno::last(),
        })
    }

    pub fn new(child_sock: UnixStream, mut err_out: PipeWriter) -> ! {
        // Double-fork so we don't have to reap
        let cause: NewErrorCause;
        let errno: Errno;

        match unsafe { fork() } {
            Ok(ForkResult::Parent { .. }) => unsafe { _exit(0) },
            Ok(ForkResult::Child) => {
                let err = grandchild_wrapped(child_sock).unwrap_err(); // the "OK" path is noreturn
                errno = err.errno;
                cause = err.cause
            }
            Err(e) => {
                errno = e.as_errno().unwrap_or(Errno::UnknownErrno);
                cause = NewErrorCause::DoubleFork
            }
        }

        let errno_bytes = (errno as i32).to_ne_bytes();
        let err_buf = [
            cause as u8,
            errno_bytes[0],
            errno_bytes[1],
            errno_bytes[2],
            errno_bytes[3],
        ];
        // We drop the result to avoid the unused warning, if we fail to write there's not much we can do.
        drop(err_out.write_all(&err_buf));
        unsafe { _exit(1) }
    }
}

pub struct Nix {
    #[allow(dead_code)] // FIXME remove after something is implemented
    conn: UnixStream,
}

#[derive(Error, Debug)]
pub enum NewNixError {
    #[error("creating communication channel with socketpair")]
    CreatingChannel(#[source] io::Error),
    #[error("creating internal error channel with pipe")]
    CreatingPipe(#[source] io::Error),
    #[error("forking ffi-helper")]
    Forking(#[source] nix::Error),
    #[error("waiting for ffi-helper to fork")]
    Waiting(#[source] nix::Error),
    #[error("double-forking ffi-helper")]
    DoubleForking(Errno),
    #[error("ffi-helper signalled")]
    HelperSignalled(Signal),
    #[error("reading from internal error channel")]
    ReadingPipe(#[source] io::Error),
    #[error("setting stdin for ffi-helper")]
    HelperStdin(Errno),
    #[error("setting stdout for ffi-helper")]
    HelperStdout(Errno),
    #[error("executing ffi-helper")]
    HelperExec(Errno),
}

impl Nix {
    // Known issues:
    // 1. Blocking waitpid, pipe read
    // 2. Technically racy fork/wait
    pub fn new() -> Result<Self, NewNixError> {
        let (parent_sock, child_sock) = UnixStream::pair().map_err(NewNixError::CreatingChannel)?;
        let (mut err_in, err_out) = pipe().map_err(NewNixError::CreatingPipe)?;
        match unsafe { fork().map_err(NewNixError::Forking)? } {
            ForkResult::Child => child::new(child_sock, err_out),
            ForkResult::Parent { child, .. } => {
                drop(err_out);
                loop {
                    match waitpid(child, None) {
                        Err(Sys(Errno::EINTR)) => continue,

                        Err(Sys(Errno::ECHILD)) => break,
                        Err(e) => return Err(NewNixError::Waiting(e)),
                        Ok(WaitStatus::Exited(_, _)) => break, // We'll read errors, if any, from the pipe.
                        Ok(WaitStatus::Signaled(_, sig, _)) => {
                            return Err(NewNixError::HelperSignalled(sig))
                        }
                        _ => unreachable!("erroneous wait status from waitpid"),
                    }
                }
            }
        }
        let mut buffer = Vec::new();
        let count = err_in
            .read_to_end(&mut buffer)
            .map_err(NewNixError::ReadingPipe)?;
        if count == 0 {
            Ok(Nix { conn: parent_sock })
        } else {
            use child::NewErrorCause;
            let errno_num = i32::from_ne_bytes(buffer[1..].try_into().unwrap_or([0, 0, 0, 0]));
            let errno = Errno::from_i32(errno_num);
            Err(match NewErrorCause::from_u8(buffer[0]) {
                Some(NewErrorCause::SetStdin) => NewNixError::HelperStdin(errno),
                Some(NewErrorCause::SetStdout) => NewNixError::HelperStdout(errno),
                Some(NewErrorCause::Exec) => NewNixError::HelperExec(errno),
                Some(NewErrorCause::DoubleFork) => NewNixError::DoubleForking(errno),
                None => unreachable!("impossible byte over internal error channel"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_connects() -> Result<(), NewNixError> {
        Nix::new().map(|_| ())
    }
}
