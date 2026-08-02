#[cfg(not(unix))]
mod portable;
#[cfg(unix)]
mod unix;

#[cfg(not(unix))]
pub(crate) use portable::RootedDir;
#[cfg(unix)]
pub(crate) use unix::RootedDir;
