#![allow(dead_code, mutable_transmutes, non_camel_case_types, non_snake_case,
         non_upper_case_globals, unused_assignments, unused_mut)]
#![register_tool(c2rust)]
#![feature(const_raw_ptr_to_usize_cast, extern_types, main,
           register_tool)]

use std::{
    io,
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    pin::Pin
};
use smol::{
    block_on,
    LocalExecutor,
    future::{self, Future, FutureExt},
    prelude::*,
};

extern "C" {
    pub type _IO_wide_data;
    pub type _IO_codecvt;
    pub type _IO_marker;
    fn dprintf(__fd: libc::c_int, __fmt: *const libc::c_char, _: ...)
     -> libc::c_int;
    fn fgets(__s: *mut libc::c_char, __n: libc::c_int, __stream: *mut FILE)
     -> *mut libc::c_char;
    fn memset(_: *mut libc::c_void, _: libc::c_int, _: libc::c_ulong)
     -> *mut libc::c_void;
    fn strchr(_: *const libc::c_char, _: libc::c_int) -> *mut libc::c_char;
    fn strlen(_: *const libc::c_char) -> libc::c_ulong;
    fn malloc(_: libc::c_ulong) -> *mut libc::c_void;
    fn close(__fd: libc::c_int) -> libc::c_int;
    fn pipe(__pipedes: *mut libc::c_int) -> libc::c_int;
    fn chdir(__path: *const libc::c_char) -> libc::c_int;
    fn dup(__fd: libc::c_int) -> libc::c_int;
    fn execv(__path: *const libc::c_char, __argv: *const *mut libc::c_char)
     -> libc::c_int;
    fn fork() -> __pid_t;
    fn open(__file: *const libc::c_char, __oflag: libc::c_int, _: ...)
     -> libc::c_int;
}
pub type size_t = libc::c_ulong;
pub type __off_t = libc::c_long;
pub type __off64_t = libc::c_long;
pub type __pid_t = libc::c_int;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct _IO_FILE {
    pub _flags: libc::c_int,
    pub _IO_read_ptr: *mut libc::c_char,
    pub _IO_read_end: *mut libc::c_char,
    pub _IO_read_base: *mut libc::c_char,
    pub _IO_write_base: *mut libc::c_char,
    pub _IO_write_ptr: *mut libc::c_char,
    pub _IO_write_end: *mut libc::c_char,
    pub _IO_buf_base: *mut libc::c_char,
    pub _IO_buf_end: *mut libc::c_char,
    pub _IO_save_base: *mut libc::c_char,
    pub _IO_backup_base: *mut libc::c_char,
    pub _IO_save_end: *mut libc::c_char,
    pub _markers: *mut _IO_marker,
    pub _chain: *mut _IO_FILE,
    pub _fileno: libc::c_int,
    pub _flags2: libc::c_int,
    pub _old_offset: __off_t,
    pub _cur_column: libc::c_ushort,
    pub _vtable_offset: libc::c_schar,
    pub _shortbuf: [libc::c_char; 1],
    pub _lock: *mut libc::c_void,
    pub _offset: __off64_t,
    pub _codecvt: *mut _IO_codecvt,
    pub _wide_data: *mut _IO_wide_data,
    pub _freeres_list: *mut _IO_FILE,
    pub _freeres_buf: *mut libc::c_void,
    pub __pad5: size_t,
    pub _mode: libc::c_int,
    pub _unused2: [libc::c_char; 20],
}
pub type _IO_lock_t = ();
pub type FILE = _IO_FILE;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct cmd {
    pub type_0: libc::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct execcmd {
    pub type_0: libc::c_int,
    pub argv: [*mut libc::c_char; 10],
    pub eargv: [*mut libc::c_char; 10],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct redircmd {
    pub type_0: libc::c_int,
    pub cmd: *mut cmd,
    pub file: *mut libc::c_char,
    pub efile: *mut libc::c_char,
    pub mode: libc::c_int,
    pub fd: libc::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct pipecmd {
    pub type_0: libc::c_int,
    pub left: *mut cmd,
    pub right: *mut cmd,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct listcmd {
    pub type_0: libc::c_int,
    pub left: *mut cmd,
    pub right: *mut cmd,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct backcmd {
    pub type_0: libc::c_int,
    pub cmd: *mut cmd,
}

// Lifetime is a lie, lives as long as s.
unsafe fn make_osstr(s: *mut i8) -> &'static OsStr {
    let len = strlen(s);
    let bytes = std::slice::from_raw_parts(s as *mut u8, len as usize);
    OsStr::from_bytes(bytes)
}

async unsafe fn spawn_child(argv: *mut *mut i8) -> i32 {
    let arg0 = make_osstr(*argv);
    let mut args = argv;
    let mut args_iter = std::iter::from_fn(|| {
        args = args.offset(1);
        if (*args).is_null() {
            None
        }
        else {

            Some(make_osstr(*args))
        }
    });
    smol::process::Command::new(arg0)
        .args(&mut args_iter)
        .status()
        .await
        .map(|e| e.code().unwrap_or(127))
        .unwrap_or_else(|_| {
            dprintf(2 as libc::c_int,
                b"exec %s failed\n\x00" as *const u8 as
                    *const libc::c_char,
                arg0);
            1
        })
}

type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
impl Shell {
    // Execute cmd.  Conceptually runs a forked shell, returns the forked shells
    // exit status.
    #[no_mangle]
    pub unsafe fn runcmd(&'static self, mut cmd: *mut cmd) -> LocalBoxFuture<libc::c_int> {
        async move {
            let mut p: [libc::c_int; 2] = [0; 2];
            let mut bcmd: *mut backcmd = 0 as *mut backcmd;
            let mut ecmd: *mut execcmd = 0 as *mut execcmd;
            let mut lcmd: *mut listcmd = 0 as *mut listcmd;
            let mut pcmd: *mut pipecmd = 0 as *mut pipecmd;
            let mut rcmd: *mut redircmd = 0 as *mut redircmd;
            if cmd.is_null() { return 1 as libc::c_int; }
            match (*cmd).type_0 {
                1 => {
                    ecmd = cmd as *mut execcmd;
                    if (*ecmd).argv[0 as libc::c_int as usize].is_null() {
                        return 1 as libc::c_int;
                    }
                    return spawn_child((*ecmd).argv.as_mut_ptr()).await;
                }
                2 => {
                    rcmd = cmd as *mut redircmd;
                    close((*rcmd).fd);
                    if open((*rcmd).file, (*rcmd).mode) < 0 as libc::c_int {
                        dprintf(2 as libc::c_int,
                                b"open %s failed\n\x00" as *const u8 as
                                    *const libc::c_char, (*rcmd).file);
                        return 1 as libc::c_int;
                    }
                    self.runcmd((*rcmd).cmd).await;
                }
                4 => {
                    lcmd = cmd as *mut listcmd;
                    self.runcmd((*lcmd).left).await;
                    self.runcmd((*lcmd).right).await;
                }
                3 => {
                    pcmd = cmd as *mut pipecmd;
                    if pipe(p.as_mut_ptr()) < 0 as libc::c_int {
                        panic(b"pipe\x00" as *const u8 as *const libc::c_char as
                                *mut libc::c_char);
                    }
                    let fut1 = async {
                        close(1 as libc::c_int);
                        dup(p[1 as libc::c_int as usize]);
                        close(p[0 as libc::c_int as usize]);
                        close(p[1 as libc::c_int as usize]);
                        self.runcmd((*pcmd).left).await;
                    };
                    let fut2 = async {
                        close(0 as libc::c_int);
                        dup(p[0 as libc::c_int as usize]);
                        close(p[0 as libc::c_int as usize]);
                        close(p[1 as libc::c_int as usize]);
                        self.runcmd((*pcmd).right).await;
                    };
                    close(p[0 as libc::c_int as usize]);
                    close(p[1 as libc::c_int as usize]);
                    future::zip(fut1, fut2).await;
                }
                5 => {
                    bcmd = cmd as *mut backcmd;
                    self.executor.spawn(self.runcmd((*bcmd).cmd)).detach();
                }
                _ => {
                    panic(b"runcmd\x00" as *const u8 as *const libc::c_char as
                            *mut libc::c_char);
                }
            }
            return 1 as libc::c_int;
        }.boxed_local()
    }
}

// Returns false if we reached EOF. Errors if we fail to read.
pub async fn getcmd(mut input: impl AsyncRead + Unpin, buf: &mut [u8]) -> Result<bool, io::Error> {
    eprint!("$ " );
    let n = input.read(buf).await?;
    buf[n] = 0;
    Ok(n != 0)
}

unsafe fn init() {
    let mut fd: libc::c_int = 0;
    loop
         // Ensure that three file descriptors are open.
         {
        fd =
            open(b"console\x00" as *const u8 as *const libc::c_char,
                 0o2 as libc::c_int);
        if !(fd >= 0 as libc::c_int) { break ; }
        if !(fd >= 3 as libc::c_int) { continue ; }
        close(fd);
        break ;
    }
}


impl Shell {
    async unsafe fn exec_string(&'static self, buf: &mut [i8]) {
        // Read and run input commands.
        if buf[0 as libc::c_int as usize] as libc::c_int == 'c' as i32 &&
                buf[1 as libc::c_int as usize] as libc::c_int == 'd' as i32 &&
                buf[2 as libc::c_int as usize] as libc::c_int == ' ' as i32 {
            // Chdir must be called by the parent, not the child.
            buf[strlen(buf.as_mut_ptr()).wrapping_sub(1 as libc::c_int as
                                                            libc::c_ulong) as
                    usize] = 0 as libc::c_int as libc::c_char; // chop \n
            if chdir(buf.as_mut_ptr().offset(3 as libc::c_int as isize)) <
                    0 as libc::c_int {
                dprintf(2 as libc::c_int,
                        b"cannot cd %s\n\x00" as *const u8 as
                            *const libc::c_char,
                        buf.as_mut_ptr().offset(3 as libc::c_int as isize));
            }
        } else {
            self.runcmd(parsecmd(buf.as_mut_ptr())).await;
        }
    }
}

// Fork but panics on failure.
#[no_mangle]
pub unsafe extern "C" fn panic(mut s: *mut libc::c_char) -> libc::c_int {
    dprintf(2 as libc::c_int, b"%s\n\x00" as *const u8 as *const libc::c_char,
            s);
    return 1 as libc::c_int;
}
#[no_mangle]
pub unsafe extern "C" fn fork1() -> libc::c_int {
    let mut pid: libc::c_int = 0;
    pid = fork();
    if pid == -(1 as libc::c_int) {
        panic(b"fork\x00" as *const u8 as *const libc::c_char as
                  *mut libc::c_char);
    }
    return pid;
}
//PAGEBREAK!
// Constructors
#[no_mangle]
pub unsafe extern "C" fn execcmd() -> *mut cmd {
    let mut cmd: *mut execcmd = 0 as *mut execcmd;
    cmd =
        malloc(::std::mem::size_of::<execcmd>() as libc::c_ulong) as
            *mut execcmd;
    memset(cmd as *mut libc::c_void, 0 as libc::c_int,
           ::std::mem::size_of::<execcmd>() as libc::c_ulong);
    (*cmd).type_0 = 1 as libc::c_int;
    return cmd as *mut cmd;
}
#[no_mangle]
pub unsafe extern "C" fn redircmd(mut subcmd: *mut cmd,
                                  mut file: *mut libc::c_char,
                                  mut efile: *mut libc::c_char,
                                  mut mode: libc::c_int, mut fd: libc::c_int)
 -> *mut cmd {
    let mut cmd: *mut redircmd = 0 as *mut redircmd;
    cmd =
        malloc(::std::mem::size_of::<redircmd>() as libc::c_ulong) as
            *mut redircmd;
    memset(cmd as *mut libc::c_void, 0 as libc::c_int,
           ::std::mem::size_of::<redircmd>() as libc::c_ulong);
    (*cmd).type_0 = 2 as libc::c_int;
    (*cmd).cmd = subcmd;
    (*cmd).file = file;
    (*cmd).efile = efile;
    (*cmd).mode = mode;
    (*cmd).fd = fd;
    return cmd as *mut cmd;
}
#[no_mangle]
pub unsafe extern "C" fn pipecmd(mut left: *mut cmd, mut right: *mut cmd)
 -> *mut cmd {
    let mut cmd: *mut pipecmd = 0 as *mut pipecmd;
    cmd =
        malloc(::std::mem::size_of::<pipecmd>() as libc::c_ulong) as
            *mut pipecmd;
    memset(cmd as *mut libc::c_void, 0 as libc::c_int,
           ::std::mem::size_of::<pipecmd>() as libc::c_ulong);
    (*cmd).type_0 = 3 as libc::c_int;
    (*cmd).left = left;
    (*cmd).right = right;
    return cmd as *mut cmd;
}
#[no_mangle]
pub unsafe extern "C" fn listcmd(mut left: *mut cmd, mut right: *mut cmd)
 -> *mut cmd {
    let mut cmd: *mut listcmd = 0 as *mut listcmd;
    cmd =
        malloc(::std::mem::size_of::<listcmd>() as libc::c_ulong) as
            *mut listcmd;
    memset(cmd as *mut libc::c_void, 0 as libc::c_int,
           ::std::mem::size_of::<listcmd>() as libc::c_ulong);
    (*cmd).type_0 = 4 as libc::c_int;
    (*cmd).left = left;
    (*cmd).right = right;
    return cmd as *mut cmd;
}
#[no_mangle]
pub unsafe extern "C" fn backcmd(mut subcmd: *mut cmd) -> *mut cmd {
    let mut cmd: *mut backcmd = 0 as *mut backcmd;
    cmd =
        malloc(::std::mem::size_of::<backcmd>() as libc::c_ulong) as
            *mut backcmd;
    memset(cmd as *mut libc::c_void, 0 as libc::c_int,
           ::std::mem::size_of::<backcmd>() as libc::c_ulong);
    (*cmd).type_0 = 5 as libc::c_int;
    (*cmd).cmd = subcmd;
    return cmd as *mut cmd;
}
//PAGEBREAK!
// Parsing
#[no_mangle]
pub static mut whitespace: [libc::c_char; 6] =
    unsafe {
        *::std::mem::transmute::<&[u8; 6],
                                 &[libc::c_char; 6]>(b" \t\r\n\x0b\x00")
    };
#[no_mangle]
pub static mut symbols: [libc::c_char; 8] =
    unsafe {
        *::std::mem::transmute::<&[u8; 8], &[libc::c_char; 8]>(b"<|>&;()\x00")
    };
#[no_mangle]
pub unsafe extern "C" fn gettoken(mut ps: *mut *mut libc::c_char,
                                  mut es: *mut libc::c_char,
                                  mut q: *mut *mut libc::c_char,
                                  mut eq: *mut *mut libc::c_char)
 -> libc::c_int {
    let mut s: *mut libc::c_char = 0 as *mut libc::c_char;
    let mut ret: libc::c_int = 0;
    s = *ps;
    while s < es && !strchr(whitespace.as_ptr(), *s as libc::c_int).is_null()
          {
        s = s.offset(1)
    }
    if !q.is_null() { *q = s }
    ret = *s as libc::c_int;
    match *s as libc::c_int {
        0 => { }
        124 | 40 | 41 | 59 | 38 | 60 => { s = s.offset(1) }
        62 => {
            s = s.offset(1);
            if *s as libc::c_int == '>' as i32 {
                ret = '+' as i32;
                s = s.offset(1)
            }
        }
        _ => {
            ret = 'a' as i32;
            while s < es &&
                      strchr(whitespace.as_ptr(), *s as libc::c_int).is_null()
                      && strchr(symbols.as_ptr(), *s as libc::c_int).is_null()
                  {
                s = s.offset(1)
            }
        }
    }
    if !eq.is_null() { *eq = s }
    while s < es && !strchr(whitespace.as_ptr(), *s as libc::c_int).is_null()
          {
        s = s.offset(1)
    }
    *ps = s;
    return ret;
}
#[no_mangle]
pub unsafe extern "C" fn peek(mut ps: *mut *mut libc::c_char,
                              mut es: *mut libc::c_char,
                              mut toks: *mut libc::c_char) -> libc::c_int {
    let mut s: *mut libc::c_char = 0 as *mut libc::c_char;
    s = *ps;
    while s < es && !strchr(whitespace.as_ptr(), *s as libc::c_int).is_null()
          {
        s = s.offset(1)
    }
    *ps = s;
    return (*s as libc::c_int != 0 &&
                !strchr(toks, *s as libc::c_int).is_null()) as libc::c_int;
}
#[no_mangle]
pub unsafe extern "C" fn parsecmd(mut s: *mut libc::c_char) -> *mut cmd {
    let mut es: *mut libc::c_char = 0 as *mut libc::c_char;
    let mut cmd: *mut cmd = 0 as *mut cmd;
    es = s.offset(strlen(s) as isize);
    cmd = parseline(&mut s, es);
    peek(&mut s, es,
         b"\x00" as *const u8 as *const libc::c_char as *mut libc::c_char);
    if s != es {
        dprintf(2 as libc::c_int,
                b"leftovers: %s\n\x00" as *const u8 as *const libc::c_char,
                s);
        panic(b"syntax\x00" as *const u8 as *const libc::c_char as
                  *mut libc::c_char);
    }
    nulterminate(cmd);
    return cmd;
}
#[no_mangle]
pub unsafe extern "C" fn parseline(mut ps: *mut *mut libc::c_char,
                                   mut es: *mut libc::c_char) -> *mut cmd {
    let mut cmd: *mut cmd = 0 as *mut cmd;
    cmd = parsepipe(ps, es);
    while peek(ps, es,
               b"&\x00" as *const u8 as *const libc::c_char as
                   *mut libc::c_char) != 0 {
        gettoken(ps, es, 0 as *mut *mut libc::c_char,
                 0 as *mut *mut libc::c_char);
        cmd = backcmd(cmd)
    }
    if peek(ps, es,
            b";\x00" as *const u8 as *const libc::c_char as *mut libc::c_char)
           != 0 {
        gettoken(ps, es, 0 as *mut *mut libc::c_char,
                 0 as *mut *mut libc::c_char);
        cmd = listcmd(cmd, parseline(ps, es))
    }
    return cmd;
}
#[no_mangle]
pub unsafe extern "C" fn parsepipe(mut ps: *mut *mut libc::c_char,
                                   mut es: *mut libc::c_char) -> *mut cmd {
    let mut cmd: *mut cmd = 0 as *mut cmd;
    cmd = parseexec(ps, es);
    if peek(ps, es,
            b"|\x00" as *const u8 as *const libc::c_char as *mut libc::c_char)
           != 0 {
        gettoken(ps, es, 0 as *mut *mut libc::c_char,
                 0 as *mut *mut libc::c_char);
        cmd = pipecmd(cmd, parsepipe(ps, es))
    }
    return cmd;
}
#[no_mangle]
pub unsafe extern "C" fn parseredirs(mut cmd: *mut cmd,
                                     mut ps: *mut *mut libc::c_char,
                                     mut es: *mut libc::c_char) -> *mut cmd {
    let mut tok: libc::c_int = 0;
    let mut q: *mut libc::c_char = 0 as *mut libc::c_char;
    let mut eq: *mut libc::c_char = 0 as *mut libc::c_char;
    while peek(ps, es,
               b"<>\x00" as *const u8 as *const libc::c_char as
                   *mut libc::c_char) != 0 {
        tok =
            gettoken(ps, es, 0 as *mut *mut libc::c_char,
                     0 as *mut *mut libc::c_char);
        if gettoken(ps, es, &mut q, &mut eq) != 'a' as i32 {
            panic(b"missing file for redirection\x00" as *const u8 as
                      *const libc::c_char as *mut libc::c_char);
        }
        match tok {
            60 => {
                cmd = redircmd(cmd, q, eq, 0 as libc::c_int, 0 as libc::c_int)
            }
            62 => {
                cmd =
                    redircmd(cmd, q, eq,
                             0o1 as libc::c_int | 0o100 as libc::c_int,
                             1 as libc::c_int)
            }
            43 => {
                // >>
                cmd =
                    redircmd(cmd, q, eq,
                             0o1 as libc::c_int | 0o100 as libc::c_int,
                             1 as libc::c_int)
            }
            _ => { }
        }
    }
    return cmd;
}
#[no_mangle]
pub unsafe extern "C" fn parseblock(mut ps: *mut *mut libc::c_char,
                                    mut es: *mut libc::c_char) -> *mut cmd {
    let mut cmd: *mut cmd = 0 as *mut cmd;
    if peek(ps, es,
            b"(\x00" as *const u8 as *const libc::c_char as *mut libc::c_char)
           == 0 {
        panic(b"parseblock\x00" as *const u8 as *const libc::c_char as
                  *mut libc::c_char);
    }
    gettoken(ps, es, 0 as *mut *mut libc::c_char,
             0 as *mut *mut libc::c_char);
    cmd = parseline(ps, es);
    if peek(ps, es,
            b")\x00" as *const u8 as *const libc::c_char as *mut libc::c_char)
           == 0 {
        panic(b"syntax - missing )\x00" as *const u8 as *const libc::c_char as
                  *mut libc::c_char);
    }
    gettoken(ps, es, 0 as *mut *mut libc::c_char,
             0 as *mut *mut libc::c_char);
    cmd = parseredirs(cmd, ps, es);
    return cmd;
}
#[no_mangle]
pub unsafe extern "C" fn parseexec(mut ps: *mut *mut libc::c_char,
                                   mut es: *mut libc::c_char) -> *mut cmd {
    let mut q: *mut libc::c_char = 0 as *mut libc::c_char;
    let mut eq: *mut libc::c_char = 0 as *mut libc::c_char;
    let mut tok: libc::c_int = 0;
    let mut argc: libc::c_int = 0;
    let mut cmd: *mut execcmd = 0 as *mut execcmd;
    let mut ret: *mut cmd = 0 as *mut cmd;
    if peek(ps, es,
            b"(\x00" as *const u8 as *const libc::c_char as *mut libc::c_char)
           != 0 {
        return parseblock(ps, es)
    }
    ret = execcmd();
    cmd = ret as *mut execcmd;
    argc = 0 as libc::c_int;
    ret = parseredirs(ret, ps, es);
    while peek(ps, es,
               b"|)&;\x00" as *const u8 as *const libc::c_char as
                   *mut libc::c_char) == 0 {
        tok = gettoken(ps, es, &mut q, &mut eq);
        if tok == 0 as libc::c_int { break ; }
        if tok != 'a' as i32 {
            panic(b"syntax\x00" as *const u8 as *const libc::c_char as
                      *mut libc::c_char);
        }
        (*cmd).argv[argc as usize] = q;
        (*cmd).eargv[argc as usize] = eq;
        argc += 1;
        if argc >= 10 as libc::c_int {
            panic(b"too many args\x00" as *const u8 as *const libc::c_char as
                      *mut libc::c_char);
        }
        ret = parseredirs(ret, ps, es)
    }
    (*cmd).argv[argc as usize] = 0 as *mut libc::c_char;
    (*cmd).eargv[argc as usize] = 0 as *mut libc::c_char;
    return ret;
}
// NUL-terminate all the counted strings.
#[no_mangle]
pub unsafe extern "C" fn nulterminate(mut cmd: *mut cmd) -> *mut cmd {
    let mut i: libc::c_int = 0;
    let mut bcmd: *mut backcmd = 0 as *mut backcmd;
    let mut ecmd: *mut execcmd = 0 as *mut execcmd;
    let mut lcmd: *mut listcmd = 0 as *mut listcmd;
    let mut pcmd: *mut pipecmd = 0 as *mut pipecmd;
    let mut rcmd: *mut redircmd = 0 as *mut redircmd;
    if cmd.is_null() { return 0 as *mut cmd }
    match (*cmd).type_0 {
        1 => {
            ecmd = cmd as *mut execcmd;
            i = 0 as libc::c_int;
            while !(*ecmd).argv[i as usize].is_null() {
                *(*ecmd).eargv[i as usize] = 0 as libc::c_int as libc::c_char;
                i += 1
            }
        }
        2 => {
            rcmd = cmd as *mut redircmd;
            nulterminate((*rcmd).cmd);
            *(*rcmd).efile = 0 as libc::c_int as libc::c_char
        }
        3 => {
            pcmd = cmd as *mut pipecmd;
            nulterminate((*pcmd).left);
            nulterminate((*pcmd).right);
        }
        4 => {
            lcmd = cmd as *mut listcmd;
            nulterminate((*lcmd).left);
            nulterminate((*lcmd).right);
        }
        5 => { bcmd = cmd as *mut backcmd; nulterminate((*bcmd).cmd); }
        _ => { }
    }
    return cmd;
}

/* Api */
pub struct Shell{
    executor: LocalExecutor<'static>,
}
impl Shell {
    pub fn new() -> Shell {
        unsafe {
            // The only global state here being touched is fd's
            // which can be moved inside the shell later.
            init();
        }

        Shell{ executor: LocalExecutor::new() }
    }
}

#[main]
pub fn main() {
    let shell: &'static Shell = Box::leak(Box::new(Shell::new()));
    let shutdown = event_listener::Event::new();

    let fut = async {
        let mut stdin = smol::Async::new(std::io::stdin()).unwrap();
        let mut buf = [0u8; 100];

        while unsafe{ getcmd(&mut stdin, &mut buf).await.unwrap() } {
            unsafe {
                let buf_ptr: &mut [i8; 100] = std::mem::transmute(&mut buf);
                shell.exec_string(buf_ptr).await;
            }
        }
        shutdown.notify(usize::MAX);
    };

    // Not sure why shutdown.listen() is needed to get tasks to spawn...
    // Only reason I even realized it is that in more real executors I always
    // have something like that anyways.
    block_on(shell.executor.run(future::zip(
        shutdown.listen(),
        fut
    )));
}