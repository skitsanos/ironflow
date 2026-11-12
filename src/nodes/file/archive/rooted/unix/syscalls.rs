use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use anyhow::{Context, Result};

/// The nine permission bits of a file mode. The file type and the setuid,
/// setgid and sticky bits are never carried to a published file.
pub(super) type Mode = libc::mode_t;
const PERMISSION_BITS: Mode = 0o777;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LeafKind {
    Missing,
    /// A regular file and its permission bits.
    Regular(Mode),
    Symlink,
    Other,
}

pub(super) fn open_directory_path(path: &Path, operation: &str) -> Result<File> {
    let path = c_name(path.as_os_str(), operation)?;
    let descriptor = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    file_from_descriptor(descriptor).map_err(Into::into)
}

pub(super) fn open_or_create_directory(
    parent: &File,
    name: &OsStr,
    operation: &'static str,
) -> Result<File> {
    let name_c = c_name(name, operation)?;
    match open_directory_at(parent, &name_c) {
        Ok(directory) => Ok(directory),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let created = unsafe { libc::mkdirat(parent.as_raw_fd(), name_c.as_ptr(), 0o755) };
            if created != 0 {
                let create_error = io::Error::last_os_error();
                if create_error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(directory_error(operation, Path::new(name), create_error));
                }
            }
            open_directory_at(parent, &name_c)
                .map_err(|error| directory_error(operation, Path::new(name), error))
        }
        Err(error) => Err(directory_error(operation, Path::new(name), error)),
    }
}

fn open_directory_at(parent: &File, name: &CStr) -> io::Result<File> {
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    file_from_descriptor(descriptor)
}

/// Create the staged file. It is private to the owner until it is published.
pub(super) fn create_file_at(parent: &File, name: &CStr) -> io::Result<File> {
    create_exclusive(parent, name, 0o600)
}

fn create_exclusive(parent: &File, name: &CStr, mode: libc::c_uint) -> io::Result<File> {
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            mode,
        )
    };
    file_from_descriptor(descriptor)
}

/// The permission bits the platform gives a file newly created in `parent`:
/// `0666` masked by the process umask, or shaped by a default ACL. The umask
/// cannot be read without changing it for every thread, so one empty file is
/// created as `name`, measured and removed again.
pub(super) fn default_file_mode(parent: &File, name: &CStr) -> io::Result<Mode> {
    let probe = create_exclusive(parent, name, 0o666)?;
    let mode = descriptor_mode(&probe);
    let removed = unlink_at(parent, name);
    let mode = mode?;
    removed?;
    Ok(mode)
}

fn descriptor_mode(file: &File) -> io::Result<Mode> {
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    syscall_result(unsafe { libc::fstat(file.as_raw_fd(), metadata.as_mut_ptr()) })?;
    Ok(unsafe { metadata.assume_init() }.st_mode & PERMISSION_BITS)
}

/// Give the open staged file the permission bits it is published with.
pub(super) fn set_mode(file: &File, mode: Mode) -> io::Result<()> {
    syscall_result(unsafe { libc::fchmod(file.as_raw_fd(), mode & PERMISSION_BITS) })
}

pub(super) fn read_file_at(parent: &File, name: &CStr) -> io::Result<File> {
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    file_from_descriptor(descriptor)
}

fn file_from_descriptor(descriptor: libc::c_int) -> io::Result<File> {
    if descriptor < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }
}

pub(super) fn inspect_leaf(parent: &File, name: &CStr) -> io::Result<LeafKind> {
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        return if error.kind() == io::ErrorKind::NotFound {
            Ok(LeafKind::Missing)
        } else {
            Err(error)
        };
    }
    let mode = unsafe { metadata.assume_init() }.st_mode;
    let kind = mode & libc::S_IFMT;
    Ok(if kind == libc::S_IFREG {
        LeafKind::Regular(mode & PERMISSION_BITS)
    } else if kind == libc::S_IFLNK {
        LeafKind::Symlink
    } else {
        LeafKind::Other
    })
}

pub(super) fn rename_at(parent: &File, source: &CStr, destination: &CStr) -> io::Result<()> {
    syscall_result(unsafe {
        libc::renameat(
            parent.as_raw_fd(),
            source.as_ptr(),
            parent.as_raw_fd(),
            destination.as_ptr(),
        )
    })
}

/// Rename `source`, resolved like any process path, onto `leaf` inside the
/// pinned `parent`. The kernel never follows an existing destination link.
pub(super) fn rename_into_directory(source: &CStr, parent: &File, leaf: &CStr) -> io::Result<()> {
    syscall_result(unsafe {
        libc::renameat(
            libc::AT_FDCWD,
            source.as_ptr(),
            parent.as_raw_fd(),
            leaf.as_ptr(),
        )
    })
}

pub(super) fn link_at(parent: &File, source: &CStr, destination: &CStr) -> io::Result<()> {
    syscall_result(unsafe {
        libc::linkat(
            parent.as_raw_fd(),
            source.as_ptr(),
            parent.as_raw_fd(),
            destination.as_ptr(),
            0,
        )
    })
}

pub(super) fn unlink_at(parent: &File, name: &CStr) -> io::Result<()> {
    syscall_result(unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) })
}

fn syscall_result(result: libc::c_int) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(super) fn c_name(name: &OsStr, operation: &str) -> Result<CString> {
    CString::new(name.as_bytes())
        .with_context(|| format!("{operation}: path contains an embedded NUL byte"))
}

fn directory_error(operation: &str, path: &Path, error: io::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "{operation}: destination component '{}' is not a safe directory: {error}",
        path.display()
    )
}
