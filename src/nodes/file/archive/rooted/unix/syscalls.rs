use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LeafKind {
    Missing,
    Regular,
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

pub(super) fn create_file_at(parent: &File, name: &CStr) -> io::Result<File> {
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
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
    let mode = unsafe { metadata.assume_init() }.st_mode & libc::S_IFMT;
    Ok(if mode == libc::S_IFREG {
        LeafKind::Regular
    } else if mode == libc::S_IFLNK {
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
