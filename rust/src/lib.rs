// TODO docs
// TODO events

use libc::c_char;
use libc::size_t;
use nix;
use nix::errno::Errno;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult};
use nix::Error::Sys;
use os_pipe::pipe;
use std::collections::HashMap;
use std::convert::TryInto;
use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::io::BufWriter;
use std::io::Read;
use std::io::Write;
use std::iter::once;
use std::net::Shutdown;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::ptr;
use std::result::Result;
use thiserror::Error;

mod child {
    // Everything here must follow the restrictions of post-fork in a multi-threaded process
    use libc::{_exit, c_char, execvp, execvpe, STDIN_FILENO, STDOUT_FILENO};
    use nix::errno::Errno;
    use nix::unistd::{dup2, fork, ForkResult};
    use os_pipe::PipeWriter;
    use std::convert::Infallible;
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::UnixStream;

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

    fn grandchild_wrapped(
        child_sock: UnixStream,
        argv: &Vec<*const c_char>,
        o_envp: &Option<Vec<*const c_char>>,
    ) -> Result<Infallible, NewError> {
        dup2(child_sock.as_raw_fd(), STDIN_FILENO).map_err(|e| NewError {
            cause: NewErrorCause::SetStdin,
            errno: e.as_errno().unwrap_or(Errno::UnknownErrno),
        })?;

        dup2(child_sock.as_raw_fd(), STDOUT_FILENO).map_err(|e| NewError {
            cause: NewErrorCause::SetStdout,
            errno: e.as_errno().unwrap_or(Errno::UnknownErrno),
        })?;

        // These unsafes are fine because we guarantee liveness and ending in a null ptr
        match o_envp {
            None => unsafe {
                execvp(argv[0], argv.as_ptr());
            },
            Some(envp) => unsafe {
                execvpe(argv[0], argv.as_ptr(), envp.as_ptr());
            },
        }
        Err(NewError {
            cause: NewErrorCause::Exec,
            errno: Errno::last(),
        })
    }

    pub fn new(
        child_sock: UnixStream,
        mut err_out: PipeWriter,
        argv: &Vec<*const c_char>,
        envp: &Option<Vec<*const c_char>>,
    ) -> ! {
        // Double-fork so we don't have to reap
        let cause: NewErrorCause;
        let errno: Errno;

        match unsafe { fork() } {
            Ok(ForkResult::Parent { .. }) => unsafe { _exit(0) },
            Ok(ForkResult::Child) => {
                let err = grandchild_wrapped(child_sock, argv, envp).unwrap_err(); // the "OK" path is noreturn
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

#[derive(Default)]
pub struct NixConfig<AI, S, EI, K, V>
where
    for<'a> &'a AI: IntoIterator<Item = &'a S>,
    S: AsRef<CStr>,
    for<'a> &'a EI: IntoIterator<Item = (&'a K, &'a V)>,
    K: AsRef<CStr>,
    V: AsRef<CStr>,
{
    pub extra_args: AI,
    pub vars: Option<EI>,
}

pub type SimpleNixConfig =
    NixConfig<Vec<CString>, CString, HashMap<CString, CString>, CString, CString>;

pub struct Nix {
    conn: BufWriter<UnixStream>,
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
    // TODO make these non-blocking, integrable into event loops
    // TODO Technically racy fork/wait
    pub fn new<AI, S, EI, K, V>(cfg: NixConfig<AI, S, EI, K, V>) -> Result<Self, NewNixError>
    where
        for<'a> &'a AI: IntoIterator<Item = &'a S>,
        S: AsRef<CStr>,
        for<'a> &'a EI: IntoIterator<Item = (&'a K, &'a V)>,
        K: AsRef<CStr>,
        V: AsRef<CStr>,
    {
        let argv: Vec<*const c_char> = {
            let ret: Vec<*const c_char> =
                vec![
		    OsStr::new("nix\0").as_bytes().as_ptr() as *const c_char,
		    OsStr::new("--extra-experimental-features\0").as_bytes().as_ptr() as *const c_char,
		    OsStr::new("nix-command\0").as_bytes().as_ptr() as *const c_char,
		    OsStr::new("--extra-plugin-files\0").as_bytes().as_ptr() as *const c_char,
		    OsStr::new(concat!(env!("NIX_FFI_PLUGIN_PREFIX", "you must set the NIX_FFI_PLUGIN_PREFIX environment variable to point to the installation prefix of the nix-ffi plugin"), "/lib/nix/plugins\0")).as_bytes().as_ptr() as *const c_char
		];

            ret.into_iter()
                .chain(
                    cfg.extra_args
                        .into_iter()
                        .map(|s| s.as_ref().as_ptr())
                        .chain(once(
                            OsStr::new("ffi-helper\0").as_bytes().as_ptr() as *const c_char
                        ))
                        .chain(once(ptr::null())),
                )
                .collect()
        };
        let envp_buf: Option<Vec<CString>> = match cfg.vars {
            None => None,
            Some(vars) => Some({
                vars.into_iter()
                    .map(|(k, v)| {
                        let k_bytes = k.as_ref().to_bytes();
                        let v_bytes = v.as_ref().to_bytes();
                        let mut ret = Vec::with_capacity(
                            k_bytes.len() + v_bytes.len() + 2, /* = and trailing nul */
                        );
                        ret.extend_from_slice(k.as_ref().to_bytes());
                        ret.push(b'=');
                        ret.extend_from_slice(v.as_ref().to_bytes());
                        // This is safe because we dropped the nuls
                        unsafe { CString::from_vec_unchecked(ret) }
                    })
                    .collect()
            }),
        };
        let envp: Option<Vec<*const c_char>> = match &envp_buf {
            None => None,
            Some(vars) => Some({
                vars.into_iter()
                    .map(|s| s.as_ptr())
                    .chain(once(ptr::null()))
                    .collect()
            }),
        };
        let (parent_sock, child_sock) = UnixStream::pair().map_err(NewNixError::CreatingChannel)?;
        let (mut err_in, err_out) = pipe().map_err(NewNixError::CreatingPipe)?;
        match unsafe { fork().map_err(NewNixError::Forking)? } {
            ForkResult::Child => child::new(child_sock, err_out, &argv, &envp),
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
            Ok(Nix {
                conn: BufWriter::new(parent_sock),
            })
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

    // TODO Error handling
    // TODO nonblocking
    pub fn add_temproot(&mut self, base_name: &OsStr) -> Result<(), io::Error> {
        let base_name_slice = base_name.as_bytes();
        let base_name_len: size_t = base_name_slice.len();
        let base_name_len_bytes = base_name_len.to_ne_bytes();
        self.conn.write(&[0])?;
        self.conn.write(&base_name_len_bytes)?;
        self.conn.write(base_name_slice)?;
        self.conn.flush()?;
        let mut result_buf: [u8; 1] = [0];
        self.conn.get_mut().read_exact(&mut result_buf)?;
        if result_buf[0] == 0 {
            Ok(())
        } else {
            unreachable!("impossible byte from ffi-helper");
        }
    }

    // TODO maybe return the fd here so waiting can be separate from closing
    pub fn wait_for_exit(mut self) -> Result<(), io::Error> {
        self.conn.get_ref().shutdown(Shutdown::Write)?;
        let mut buf = Vec::new();
        self.conn.get_mut().read_to_end(&mut buf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::Path;
    use std::process::Command;
    use tempfile::tempdir;

    fn set_testroot_envs(envs: &mut HashMap<CString, CString>, root: &Path) {
        fn insert<T1: Into<Vec<u8>>, T2: Into<Vec<u8>>>(
            envs: &mut HashMap<CString, CString>,
            key: T1,
            val: T2,
        ) {
            envs.insert(CString::new(key).unwrap(), CString::new(val).unwrap());
        }
        insert(
            envs,
            "NIX_STORE_DIR",
            root.join("store").as_os_str().as_bytes(),
        );
        insert(envs, "NIX_IGNORE_SYMLINK_STORE", "1");
        insert(
            envs,
            "NIX_LOCALSTATE_DIR",
            root.join("var").as_os_str().as_bytes(),
        );
        insert(
            envs,
            "NIX_LOG_DIR",
            root.join("var/log/nix").as_os_str().as_bytes(),
        );
        insert(
            envs,
            "NIX_STATE_DIR",
            root.join("var/nix").as_os_str().as_bytes(),
        );
        insert(
            envs,
            "NIX_CONF_DIR",
            root.join("etc").as_os_str().as_bytes(),
        );
        envs.remove(&CString::new("NIX_USER_CONF_FILES").unwrap());
    }

    fn put_in_testroot<'a>(cmd: &'a mut Command, root: &Path) -> &'a mut Command {
        let mut envs = HashMap::new();
        set_testroot_envs(&mut envs, root);
        cmd.envs(envs.iter().map(|(k, v)| {
            (
                OsStr::from_bytes(k.to_bytes()),
                OsStr::from_bytes(v.to_bytes()),
            )
        }))
        .env_remove("NIX_USER_CONF_FILES")
    }

    #[test]
    fn temproot_roots() {
        let root = tempdir().unwrap();

        // Add manifest
        let mut manifest_file = env::var_os("CARGO_MANIFEST_DIR").unwrap();
        manifest_file.push("/Cargo.toml");
        let mut cmd = Command::new("nix-store");
        cmd.args(&["--add", &manifest_file.into_string().unwrap()]);
        let add_result = put_in_testroot(&mut cmd, root.path()).output().unwrap();
        dbg!(add_result.status);
        assert!(add_result.status.success());
        let mut path_str = String::from_utf8(add_result.stdout).unwrap();
        path_str.truncate(path_str.trim_end().len());
        let path = Path::new(&path_str);
        assert!(path.exists());

        let delete_path = || {
            let mut cmd = Command::new("nix-store");
            cmd.args(&["--delete", &path.to_str().unwrap()]);
            put_in_testroot(&mut cmd, root.path()).status().unwrap();
        };

        let mut envp = env::vars_os()
            .map(|(k, v)| {
                (
                    CString::new(k.as_bytes()).unwrap(),
                    CString::new(v.as_bytes()).unwrap(),
                )
            })
            .collect();
        set_testroot_envs(&mut envp, root.path());
        let mut nix = Nix::new(SimpleNixConfig {
            extra_args: Vec::new(),
            vars: Some(envp),
        })
        .unwrap();
        nix.add_temproot(path.file_name().unwrap()).unwrap();
        delete_path();
        assert!(path.exists());

        nix.wait_for_exit().unwrap();
        delete_path();
        assert!(!path.exists());
    }
}
