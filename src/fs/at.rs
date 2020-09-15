//! POSIX-style `*at` functions.

#[cfg(any(target_os = "macos", target_os = "ios"))]
use crate::fs::CloneFlags;
use crate::{
    fs::{Access, AtFlags, LibcStat, Mode, OFlags, PathArg},
    negone_err, zero_ok,
};
#[cfg(not(any(target_os = "linux", target_os = "emscripten", target_os = "l4re")))]
use libc::{fstatat as libc_fstatat, openat as libc_openat};
#[cfg(any(target_os = "linux", target_os = "emscripten", target_os = "l4re"))]
use libc::{fstatat64 as libc_fstatat, openat64 as libc_openat};
#[cfg(unix)]
use std::os::unix::{
    ffi::OsStrExt,
    io::{AsRawFd, FromRawFd, RawFd},
};
#[cfg(target_os = "wasi")]
use std::os::wasi::{
    ffi::OsStringExt,
    io::{AsRawFd, FromRawFd, RawFd},
};
use std::{
    convert::TryInto,
    ffi::{CStr, OsString},
    fs, io,
    mem::MaybeUninit,
};
#[cfg(not(target_os = "wasi"))]
use std::{ffi::OsStr, mem::ManuallyDrop};

/// Return a "file" which holds a handle which refers to the process current
/// directory (`AT_FDCWD`). It is a `ManuallyDrop`, however the caller should
/// not drop it explicitly, as it refers to an ambient authority rather than
/// an open resource.
#[cfg(not(target_os = "wasi"))]
pub fn cwd() -> ManuallyDrop<fs::File> {
    ManuallyDrop::new(unsafe { fs::File::from_raw_fd(libc::AT_FDCWD) })
}

/// `openat(dirfd, path, oflags, mode)`
#[inline]
pub fn openat<P: PathArg, Fd: AsRawFd>(
    dirfd: &Fd,
    path: P,
    oflags: OFlags,
    mode: Mode,
) -> io::Result<fs::File> {
    let dirfd = dirfd.as_raw_fd();
    let path = path.as_cstr()?;
    unsafe { _openat(dirfd, &path, oflags, mode) }
}

unsafe fn _openat(dirfd: RawFd, path: &CStr, oflags: OFlags, mode: Mode) -> io::Result<fs::File> {
    #[allow(clippy::useless_conversion)]
    let fd = negone_err(libc_openat(
        dirfd as libc::c_int,
        path.as_ptr(),
        oflags.bits(),
        libc::c_uint::from(mode.bits()),
    ))?;

    Ok(fs::File::from_raw_fd(fd as RawFd))
}

/// `readlinkat(fd, path)`
#[inline]
pub fn readlinkat<P: PathArg, Fd: AsRawFd>(dirfd: &Fd, path: P) -> io::Result<OsString> {
    let dirfd = dirfd.as_raw_fd();
    let path = path.as_cstr()?;
    unsafe { _readlinkat(dirfd, &path) }
}

/// Implement `readlinkat` on platforms which have `PATH_MAX` that we can rely on.
#[cfg(not(target_os = "wasi"))]
unsafe fn _readlinkat(dirfd: RawFd, path: &CStr) -> io::Result<OsString> {
    let buffer = &mut [0_u8; libc::PATH_MAX as usize + 1];
    let nread = negone_err(libc::readlinkat(
        dirfd,
        path.as_ptr(),
        buffer.as_mut_ptr() as *mut libc::c_char,
        buffer.len(),
    ))?;

    let nread = nread.try_into().unwrap();
    let link = OsStr::from_bytes(&buffer[0..nread]);
    Ok(link.into())
}

/// Implemented `readlinkat` on platforms where we dyamically allocate the buffer instead.
#[cfg(target_os = "wasi")]
unsafe fn _readlinkat(dirfd: RawFd, path: &CStr) -> io::Result<OsString> {
    // Start with a buffer big enough for the vast majority of paths.
    let mut buffer = Vec::with_capacity(256);
    loop {
        let nread = negone_err(unsafe {
            libc::readlinkat(
                dirfd as libc::c_int,
                path.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_char,
                buffer.capacity(),
            )
        })?;

        let nread = nread.try_into().unwrap();
        buffer.set_len(nread);
        if nread < buffer.capacity() {
            return Ok(OsString::from_vec(buffer));
        }

        // This would be a good candidate for `try_reserve`.
        // https://github.com/rust-lang/rust/issues/48043
        buffer.reserve(1);
    }
}

/// `mkdirat(fd, path, mode)`
#[inline]
pub fn mkdirat<P: PathArg, Fd: AsRawFd>(dirfd: &Fd, path: P, mode: Mode) -> io::Result<()> {
    let dirfd = dirfd.as_raw_fd();
    let path = path.as_cstr()?;
    unsafe { _mkdirat(dirfd, &path, mode) }
}

unsafe fn _mkdirat(dirfd: RawFd, path: &CStr, mode: Mode) -> io::Result<()> {
    zero_ok(libc::mkdirat(
        dirfd as libc::c_int,
        path.as_ptr(),
        mode.bits(),
    ))
}

/// `linkat(old_dirfd, old_path, new_dirfd, new_path, flags)`
#[inline]
pub fn linkat<P: PathArg, Q: PathArg, PFd: AsRawFd, QFd: AsRawFd>(
    old_dirfd: &PFd,
    old_path: P,
    new_dirfd: &QFd,
    new_path: Q,
    flags: AtFlags,
) -> io::Result<()> {
    let old_dirfd = old_dirfd.as_raw_fd();
    let new_dirfd = new_dirfd.as_raw_fd();
    let old_path = old_path.as_cstr()?;
    let new_path = new_path.as_cstr()?;
    unsafe { _linkat(old_dirfd, &old_path, new_dirfd, &new_path, flags) }
}

unsafe fn _linkat(
    old_dirfd: RawFd,
    old_path: &CStr,
    new_dirfd: RawFd,
    new_path: &CStr,
    flags: AtFlags,
) -> io::Result<()> {
    zero_ok(libc::linkat(
        old_dirfd as libc::c_int,
        old_path.as_ptr(),
        new_dirfd as libc::c_int,
        new_path.as_ptr(),
        flags.bits(),
    ))
}

/// `unlinkat(fd, path, flags)`
#[inline]
pub fn unlinkat<P: PathArg, Fd: AsRawFd>(dirfd: &Fd, path: P, flags: AtFlags) -> io::Result<()> {
    let dirfd = dirfd.as_raw_fd();
    let path = path.as_cstr()?;
    unsafe { _unlinkat(dirfd, &path, flags) }
}

unsafe fn _unlinkat(dirfd: RawFd, path: &CStr, flags: AtFlags) -> io::Result<()> {
    zero_ok(libc::unlinkat(
        dirfd as libc::c_int,
        path.as_ptr(),
        flags.bits(),
    ))
}

/// `renameat(old_dirfd, old_path, new_dirfd, new_path)`
#[inline]
pub fn renameat<P: PathArg, Q: PathArg, PFd: AsRawFd, QFd: AsRawFd>(
    old_dirfd: &PFd,
    old_path: P,
    new_dirfd: &QFd,
    new_path: Q,
) -> io::Result<()> {
    let old_dirfd = old_dirfd.as_raw_fd();
    let new_dirfd = new_dirfd.as_raw_fd();
    let old_path = old_path.as_cstr()?;
    let new_path = new_path.as_cstr()?;
    unsafe { _renameat(old_dirfd, &old_path, new_dirfd, &new_path) }
}

unsafe fn _renameat(
    old_dirfd: RawFd,
    old_path: &CStr,
    new_dirfd: RawFd,
    new_path: &CStr,
) -> io::Result<()> {
    zero_ok(libc::renameat(
        old_dirfd as libc::c_int,
        old_path.as_ptr(),
        new_dirfd as libc::c_int,
        new_path.as_ptr(),
    ))
}

/// `symlinkat(old_dirfd, old_path, new_dirfd, new_path)`
#[inline]
pub fn symlinkat<P: PathArg, Q: PathArg, Fd: AsRawFd>(
    old_path: P,
    new_dirfd: &Fd,
    new_path: Q,
) -> io::Result<()> {
    let new_dirfd = new_dirfd.as_raw_fd();
    let old_path = old_path.as_cstr()?;
    let new_path = new_path.as_cstr()?;
    unsafe { _symlinkat(&old_path, new_dirfd, &new_path) }
}

unsafe fn _symlinkat(old_path: &CStr, new_dirfd: RawFd, new_path: &CStr) -> io::Result<()> {
    zero_ok(libc::symlinkat(
        old_path.as_ptr(),
        new_dirfd as libc::c_int,
        new_path.as_ptr(),
    ))
}

/// `fstatat(dirfd, path, flags)`
#[inline]
pub fn statat<P: PathArg, Fd: AsRawFd>(
    dirfd: &Fd,
    path: P,
    flags: AtFlags,
) -> io::Result<LibcStat> {
    let dirfd = dirfd.as_raw_fd();
    let path = path.as_cstr()?;
    unsafe { _statat(dirfd, &path, flags) }
}

unsafe fn _statat(dirfd: RawFd, path: &CStr, flags: AtFlags) -> io::Result<LibcStat> {
    let mut stat = MaybeUninit::<LibcStat>::uninit();
    zero_ok(libc_fstatat(
        dirfd as libc::c_int,
        path.as_ptr(),
        stat.as_mut_ptr(),
        flags.bits(),
    ))?;
    Ok(stat.assume_init())
}

/// `faccessat(dirfd, path, access, flags)`
#[inline]
pub fn accessat<P: PathArg, Fd: AsRawFd>(
    dirfd: &Fd,
    path: P,
    access: Access,
    flags: AtFlags,
) -> io::Result<()> {
    let dirfd = dirfd.as_raw_fd();
    let path = path.as_cstr()?;
    unsafe { _accessat(dirfd, &path, access, flags) }
}

#[cfg(not(target_os = "emscripten"))]
unsafe fn _accessat(dirfd: RawFd, path: &CStr, access: Access, flags: AtFlags) -> io::Result<()> {
    zero_ok(libc::faccessat(
        dirfd as libc::c_int,
        path.as_ptr(),
        access.bits(),
        flags.bits(),
    ))
}

// Temporarily disable on Emscripten until https://github.com/rust-lang/libc/pull/1836
// is available.
#[cfg(target_os = "emscripten")]
unsafe fn _accessat(
    _dirfd: RawFd,
    _path: &CStr,
    _access: Access,
    _flags: AtFlags,
) -> io::Result<()> {
    Ok(())
}

/// `utimensat(dirfd, path, times, flags)`
pub fn utimensat<P: PathArg, Fd: AsRawFd>(
    dirfd: &Fd,
    path: P,
    times: &[libc::timespec; 2],
    flags: AtFlags,
) -> io::Result<()> {
    let dirfd = dirfd.as_raw_fd();
    let path = path.as_cstr()?;
    unsafe { _utimensat(dirfd, &path, times, flags) }
}

unsafe fn _utimensat(
    dirfd: RawFd,
    path: &CStr,
    times: &[libc::timespec; 2],
    flags: AtFlags,
) -> io::Result<()> {
    zero_ok(libc::utimensat(
        dirfd as libc::c_int,
        path.as_ptr(),
        times.as_ptr(),
        flags.bits(),
    ))
}

/// `fchmodat(dirfd, path, mode, 0)`
///
/// The flags argument is fixed to 0, so `AT_SYMLINK_NOFOLLOW` is not supported.
/// <details>
/// Platform support for this flag varies widely.
/// </details>
///
/// Note that this implementation does not support `O_PATH` file descriptors,
/// even on platforms where the host libc emulates it.
#[cfg(not(target_os = "wasi"))]
pub fn chmodat<P: PathArg, Fd: AsRawFd>(dirfd: &Fd, path: P, mode: Mode) -> io::Result<()> {
    let dirfd = dirfd.as_raw_fd();
    let path = path.as_cstr()?;
    unsafe { _chmodat(dirfd, &path, mode) }
}

#[cfg(not(any(target_os = "linux", target_os = "wasi")))]
unsafe fn _chmodat(dirfd: RawFd, path: &CStr, mode: Mode) -> io::Result<()> {
    zero_ok(libc::fchmodat(
        dirfd as libc::c_int,
        path.as_ptr(),
        mode.bits(),
        0,
    ))
}

#[cfg(target_os = "linux")]
unsafe fn _chmodat(dirfd: RawFd, path: &CStr, mode: Mode) -> io::Result<()> {
    // Note that Linux's `fchmodat` does not have a flags argument.
    zero_ok(libc::syscall(
        libc::SYS_fchmodat,
        dirfd as libc::c_int,
        path.as_ptr(),
        mode.bits(),
    ))
}

/// `fclonefileat(src, dst_dir, dst, flags)`
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[inline]
pub fn fclonefileat<Fd: AsRawFd, DstFd: AsRawFd, P: PathArg>(
    src: &Fd,
    dst_dir: &DstFd,
    dst: P,
    flags: CloneFlags,
) -> io::Result<()> {
    let srcfd = src.as_raw_fd();
    let dst_dirfd = dst_dir.as_raw_fd();
    let dst = dst.as_cstr()?;
    unsafe { _fclonefileat(srcfd, dst_dirfd, &dst, flags) }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
unsafe fn _fclonefileat(
    srcfd: libc::c_int,
    dst_dirfd: libc::c_int,
    dst: &CStr,
    flags: CloneFlags,
) -> io::Result<()> {
    syscall! {
        fn fclonefileat(
            srcfd: libc::c_int,
            dst_dirfd: libc::c_int,
            dst: *const libc::c_char,
            flags: libc::c_int
        ) -> libc::c_int
    }

    zero_ok(fclonefileat(srcfd, dst_dirfd, dst.as_ptr(), flags.bits()))
}
