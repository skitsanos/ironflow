#[cfg(not(unix))]
mod portable;
#[cfg(unix)]
mod unix;

#[cfg(not(unix))]
pub(super) use portable::RootedDir;
#[cfg(unix)]
pub(super) use unix::RootedDir;
