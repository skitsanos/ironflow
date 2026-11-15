//! Run the CLI on one PDF and measure the child process from outside (Unix).

use std::path::Path;

/// One `ironflow run` of an `extract_pdf` step, measured from outside.
pub struct Measured {
    pub success: bool,
    pub killed: bool,
    pub elapsed: std::time::Duration,
    pub peak_rss_bytes: u64,
    pub output: String,
}

/// Run the CLI on `pdf` with a five-second step timeout, kill it after
/// `limit`, and report the child's own peak resident set size.
pub fn run_measured(pdf: &Path, limit: std::time::Duration) -> Measured {
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    let temp = tempfile::tempdir().unwrap();
    let flow = temp.path().join("flow.lua");
    let log = temp.path().join("output.log");
    std::fs::write(
        &flow,
        format!(
            "local flow = Flow.new(\"cmap_probe\")\n\
             flow:step(\"extract\", nodes.extract_pdf({{ path = {:?}, output_key = \"text\" }})):timeout(5)\n\
             flow:step(\"show\", nodes.log({{ message = \"TEXT=[${{ctx.text}}]\" }})):depends_on(\"extract\")\n\
             return flow\n",
            pdf.to_string_lossy()
        ),
    )
    .unwrap();
    let sink = std::fs::File::create(&log).unwrap();
    // `wait4` below reaps the child; unlike `Child::wait` it also returns the
    // child's own resource usage.
    #[allow(clippy::zombie_processes)]
    let child = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .arg("run")
        .arg(&flow)
        .arg("--store-dir")
        .arg(temp.path().join("runs"))
        .stdin(Stdio::null())
        .stdout(sink.try_clone().unwrap())
        .stderr(sink)
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    let pid = child.id() as libc::pid_t;
    let done = Arc::new(AtomicBool::new(false));
    let sampled_peak = Arc::new(AtomicU64::new(0));
    let watchdog = std::thread::spawn({
        let done = Arc::clone(&done);
        let sampled_peak = Arc::clone(&sampled_peak);
        move || {
            while started.elapsed() < limit {
                if done.load(Ordering::Acquire) {
                    return false;
                }
                if let Some(bytes) = own_peak_bytes(pid) {
                    sampled_peak.fetch_max(bytes, Ordering::AcqRel);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            // SAFETY: the child has not been reaped while `done` is unset.
            unsafe { libc::kill(pid, libc::SIGKILL) };
            true
        }
    });
    let mut status = 0;
    // SAFETY: `usage` is plain data that wait4 fills for this unreaped child.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let reaped = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
    done.store(true, Ordering::Release);
    let killed = watchdog.join().unwrap();
    assert_eq!(reaped, pid, "wait4 must reap the CLI child");
    let peak_rss_bytes = if cfg!(target_os = "linux") {
        // `ru_maxrss` cannot be used here: see `own_peak_bytes`.
        let sampled = sampled_peak.load(Ordering::Acquire);
        assert!(
            sampled > 0,
            "the CLI child ended before its memory was sampled"
        );
        sampled
    } else {
        // macOS reports bytes; other Unix systems report kilobytes.
        let scale = if cfg!(target_os = "macos") { 1 } else { 1024 };
        u64::try_from(usage.ru_maxrss).unwrap_or(0) * scale
    };
    Measured {
        success: libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        killed,
        elapsed: started.elapsed(),
        peak_rss_bytes,
        output: std::fs::read_to_string(&log).unwrap_or_default(),
    }
}

/// The peak resident set size of the CLI's own address space, on Linux.
///
/// Linux carries the spawning process's peak into the child's `ru_maxrss`
/// across `exec`, so that figure is at least the test binary's own memory at
/// spawn time. Other tests in the same binary allocate tens of megabytes
/// concurrently, which made the bound fail without any change in the CLI
/// (IF-220 item (z)). `VmHWM` belongs to the address space created by `exec`
/// and starts from zero, so it is read only once the process runs the CLI
/// image. It is sampled, so growth in the last few milliseconds before exit is
/// not seen; an unbounded expansion runs far longer than that.
#[cfg(target_os = "linux")]
fn own_peak_bytes(pid: libc::pid_t) -> Option<u64> {
    let image = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    let expected = std::path::Path::new(env!("CARGO_BIN_EXE_ironflow"));
    if image.file_name() != expected.file_name() {
        return None;
    }
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes * 1024)
}

#[cfg(not(target_os = "linux"))]
fn own_peak_bytes(_pid: libc::pid_t) -> Option<u64> {
    None
}
