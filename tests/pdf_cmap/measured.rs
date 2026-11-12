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
    use std::sync::atomic::{AtomicBool, Ordering};

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
    let watchdog = std::thread::spawn({
        let done = Arc::clone(&done);
        move || {
            while started.elapsed() < limit {
                if done.load(Ordering::Acquire) {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
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
    // Linux reports kilobytes, macOS bytes.
    let scale = if cfg!(target_os = "macos") { 1 } else { 1024 };
    Measured {
        success: libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        killed,
        elapsed: started.elapsed(),
        peak_rss_bytes: u64::try_from(usage.ru_maxrss).unwrap_or(0) * scale,
        output: std::fs::read_to_string(&log).unwrap_or_default(),
    }
}
