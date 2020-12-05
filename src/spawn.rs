// My own lightweight async_process

use std::{
    collections::HashMap,
    rc::Rc,
    num::NonZeroU32,
    os::unix::{
        io::{AsRawFd, FromRawFd, RawFd},
    },
};

use smol::{Async, fs::File, prelude::*};


/* On The Reaping of Children

This turns out to be... frustrating.

The obvious/normal solution is to listen for SIGCHLD (either through
a signal handler or signalfd), and when we recieve a SIGCHLD reap the
child. Unfortunately linux makes this difficult, multple SIGCHLD 
signals can be coallesced into one, in which case we don't get a
complete list of dead children. We could just call a generic wait(),
but if we did that, and we're being used as a library, we might reap
some child that some other wait call wants to reap.

There are solutions to this... for example we could keep a list of
children we are responsible for waiting for, and call wait with WNOHANG
on each of those. That sounds expensive if we're running many children
though. We could call waitid(P_ALL, WNOWAIT) to find *a* child that has
died without reaping it, and then only reap it if it is ours. But this
leaves a weird condition where we are blocked on waiting for another part
of the code to reap it's child before we can reap our children.

We could also say "fuck you" to the rest of the program and just assume
it has no children... which is honestly very tempting.

However what I've decided to go with is something newer. Linux 5.3
introduced PIDFD (sometime in 2019). PIDFD's can be waited on with
epoll, so it interacts natively with our async code. No need for signal
handling at all.

But wait, it get's worse. The original implementation of pidfd didn't
support O_NONBLOCK. That only got added in 
6da73d15258a1e5e86d03d4ffba8776d17a8a287 ... merged Oct 14 2020. This
*will* be in 5.10, of which rc-6 was released about a week ago. This
isn't really "serious" code right now, so I decided to be fine with that
and consider it beneficial testing of the ecosystem. In reality, if you
want to use this code for anything serious in the near future, use something
like https://github.com/pop-os/pidfd/ and listen in another thread, or
consider going back to using signals.

*/


use libc::{size_t, dup2, execv, exit, syscall, waitid, P_PIDFD, SYS_clone3, WEXITED, __WALL};
const CLONE_CLEAR_SIGHAND: u64 = 0x100000000;
const CLONE_PIDFD: u64 = 0x00001000;

#[repr(C)]
#[derive(Default)]
struct CloneArgs {
    /// Flags bitmask
    flags: u64,
    /// Where to store PID file description (pid_t *)
    pidfd: u64,
    /// Where to store child TID in child's emmoery (pid_t *)
    child_tid: u64,
    /// Signal to deliver to parent on child termination
    ///
    /// If this signal is specified as anything other than SIGCHLD, then the
    /// parent process must specify the __WALL or __WCLONE options when waiting
    /// for the child with wait(2).  If no signal (i.e., zero) is specified,
    /// then the parent process is not signaled when the child terminates.
    exit_signal: u64,
    /// Pointer to lowest byte of stack
    ///
    /// If CLONE_VM is not set, and both stack and stack_size are 0,
    /// then the same stack space as in the parent is used.
    stack: u64,
    /// Size of stack.
    stack_size: u64,
    /// Location of new TLS
    tls: u64,
    /// Pointer to a pid_t array (since Linux 5.5)
    set_tid: u64,
    /// Number of elements in set_tid (since Linux 5.5)
    set_tid_size: u64,
    /// File descriptor for target cgroup of child (since LInux 5.7)
    cgroup: u64,
}

enum ForkResult<F: Future<Output=i32>> {
    Child,
    Parent{
        pid: NonZeroU32,
        /// Do we ever want more than this from the pidfd?
        exit_status: F,
    },
    Error(i64),
}

// It's not actually immediately clear to me *how* this
// function is unsafe, but I'm sure I have not thought of
// everything that could go wrong if you used it maliciously...
unsafe fn fork() -> ForkResult<impl Future<Output=i32>> {
    // By fork we actually mean clone, and by clone
    // we actually mean invoking the clone3 syscall
    // mostly on principle (the clone syscall is reusing
    // random pointers to return us the PIDFD).

    let mut pidfd = 0;
    let mut args = CloneArgs {
        // CLONE_CLEAR_SIGHAND is not strictly necessary, but makes us a better
        // library. If someone has set a signal handler for some reason, it is
        // not passed to the child.
        //
        // CLONE_PIDFD is needed to get a PIDFD for async child reaping.
        flags: CLONE_CLEAR_SIGHAND | CLONE_PIDFD,
        pidfd: (&mut pidfd) as *mut _ as usize as u64,
        .. Default::default()
    };

    let r = syscall(
        SYS_clone3,
        &mut args,
        std::mem::size_of::<CloneArgs>() as size_t,
    );

    if r < 0 {
        ForkResult::Error(r)
    }
    else if r == 0 {
        ForkResult::Child
    }
    else {
        ForkResult::Parent{
            pid: NonZeroU32::new(r as u32).unwrap(),
            exit_status: async move {
                let pidfd_file = File::from_raw_fd(pidfd);
                // Set NONBLOCK
                let pidfd_async = Async::new(pidfd_file).unwrap();
                // Wait until epoll considers it readable
                pidfd_async.readable().await.unwrap();
                // Reap the child
                let errno_loc = libc::__errno_location();
                let mut info: libc::siginfo_t;
                loop {
                    info = std::mem::zeroed();
                    // Should use the `waitid` syscall instead, since it gives back some
                    // cool resource usage information :).
                    let err = waitid(P_PIDFD, pidfd as u32, &mut info, WEXITED | __WALL);

                    if err == -1 && *errno_loc == libc::EINTR {
                        // Weird, but ok
                        continue;
                    }
                    // TODO: Handle err?
                    assert_eq!(err, 0, "With errno {} and pid {}", *errno_loc, r);
                    break;
                }

                // We have more information here, e.g. si_code tells us if it was
                // killed by a signal instead of exiting. But for now
                info.si_status()
            }
        }
    }
}

pub unsafe fn spawn(
    argv: *const *const i8,
    fds: &HashMap<RawFd, Rc<File>>
) -> Result<impl Future<Output=i32>, i64> {
    match fork() {
        ForkResult::Child => {
            for (fd, file) in fds.iter() {
                if file.as_raw_fd() != *fd {
                    let r = dup2(file.as_raw_fd(), *fd);
                    if r < 0 {
                        exit(127);
                    }
                }
            }
            execv(*argv, argv);
            eprintln!("Failed to execv");
            exit(127)
        },
        ForkResult::Parent{pid: _, exit_status} => Ok(exit_status),
        ForkResult::Error(e) => {
            // TODO: Should check errno/not just return
            // the -1 value that e almost certainly is.
            Err(e)
        }
    }
}