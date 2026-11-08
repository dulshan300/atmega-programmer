//! Controlled subprocess execution.
//!
//! Programs are always started with an explicit executable path and an
//! argument array; no shell is involved, so arguments are never interpolated.

use crate::{CoreError, CoreResult};
use serde::Serialize;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
}

impl ProcessSpec {
    pub fn new(program: impl Into<PathBuf>, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        ProcessSpec {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: None,
            timeout: Duration::from_secs(120),
        }
    }
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }
    /// Human-readable command line for logs and previews. Arguments containing
    /// spaces are quoted for display only; execution never goes through a shell.
    pub fn display(&self) -> String {
        std::iter::once(self.program.to_string_lossy().to_string())
            .chain(self.args.iter().map(|a| a.to_string_lossy().to_string()))
            .map(|a| {
                if a.contains(' ') || a.is_empty() {
                    format!("\"{a}\"")
                } else {
                    a
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOutput {
    pub command: String,
    /// `None` when the process was killed (timeout / cancellation) or ended by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
    pub duration_ms: u64,
}

impl ProcessOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out && !self.cancelled
    }
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Runs a process to completion, streaming each output line to `on_line`.
pub fn run_process(
    spec: &ProcessSpec,
    cancel: &CancelToken,
    on_line: &mut dyn FnMut(Stream, &str),
) -> CoreResult<ProcessOutput> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    // Own process group, so cancellation also stops any children a wrapper
    // script started (they would otherwise keep the programmer busy).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let start = Instant::now();
    let mut child = spawn_with_retry(&mut cmd)
        .map_err(|e| CoreError::Process(format!("failed to start {}: {e}", spec.program.display())))?;

    let (tx, rx) = mpsc::channel::<(Stream, String)>();
    let mut readers = vec![];
    if let Some(out) = child.stdout.take() {
        readers.push(spawn_reader(out, Stream::Stdout, tx.clone()));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(spawn_reader(err, Stream::Stderr, tx.clone()));
    }
    drop(tx);

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut timed_out = false;
    let mut cancelled = false;
    let mut handle = |stream: Stream, line: String, stdout: &mut String, stderr: &mut String| {
        on_line(stream, &line);
        let buf = if stream == Stream::Stdout { stdout } else { stderr };
        buf.push_str(&line);
        buf.push('\n');
    };
    let status = loop {
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok((s, l)) => handle(s, l, &mut stdout, &mut stderr),
            Err(mpsc::RecvTimeoutError::Timeout) | Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }
        if let Some(status) = child.try_wait().map_err(|e| CoreError::Process(e.to_string()))? {
            break Some(status);
        }
        if cancel.is_cancelled() {
            cancelled = true;
        } else if start.elapsed() > spec.timeout {
            timed_out = true;
        }
        if cancelled || timed_out {
            kill_tree(&mut child);
            let _ = child.wait();
            break None;
        }
    };
    // Collect the remaining output. A grandchild that outlives the process can
    // hold the pipes open indefinitely, so draining is bounded; readers still
    // blocked after the deadline are detached and exit when the pipe closes.
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((s, l)) => handle(s, l, &mut stdout, &mut stderr),
            Err(_) => break,
        }
    }
    for r in readers {
        if r.is_finished() {
            let _ = r.join();
        }
    }
    Ok(ProcessOutput {
        command: spec.display(),
        exit_code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        timed_out,
        cancelled,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Spawns, retrying briefly on ETXTBSY ("text file busy"). On Linux an
/// executable that was just written can be briefly held open by a concurrent
/// fork in another thread; the condition clears within milliseconds.
fn spawn_with_retry(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    const ETXTBSY: i32 = 26;
    let mut attempt = 0;
    loop {
        match cmd.spawn() {
            Err(e) if e.raw_os_error() == Some(ETXTBSY) && cfg!(unix) && attempt < 5 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(20 * attempt));
            }
            r => return r,
        }
    }
}

fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // The child leads its own process group (see `process_group(0)`).
        if let Ok(pid) = i32::try_from(child.id()) {
            // SAFETY: plain syscall; a negative pid targets the process group.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
}

fn spawn_reader<R: Read + Send + 'static>(
    r: R,
    stream: Stream,
    tx: mpsc::Sender<(Stream, String)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(r);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();
                    if tx.send((stream, line)).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

/// Locates an executable by name in the given directories, then in PATH.
pub fn find_executable(name: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    for dir in extra_dirs.iter().chain(path_dirs.iter()) {
        for ext in exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_exit_code() {
        let spec = ProcessSpec::new("/bin/sh", ["-c", "echo out; echo err >&2; exit 3"]);
        let mut lines = vec![];
        let out = run_process(&spec, &CancelToken::new(), &mut |s, l| {
            lines.push((s, l.to_string()))
        })
        .unwrap();
        assert_eq!(out.exit_code, Some(3));
        assert!(!out.success());
        assert_eq!(out.stdout, "out\n");
        assert_eq!(out.stderr, "err\n");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn arguments_are_not_shell_interpolated() {
        let spec = ProcessSpec::new("/bin/echo", ["$(whoami)", "a;b"]);
        let out = run_process(&spec, &CancelToken::new(), &mut |_, _| {}).unwrap();
        assert_eq!(out.stdout, "$(whoami) a;b\n");
    }

    #[test]
    fn timeout_kills_process() {
        let spec = ProcessSpec::new("/bin/sleep", ["5"]).timeout(Duration::from_millis(200));
        let out = run_process(&spec, &CancelToken::new(), &mut |_, _| {}).unwrap();
        assert!(out.timed_out);
        assert!(!out.success());
        assert!(out.duration_ms < 3000);
    }

    #[test]
    fn cancellation_kills_process() {
        let spec = ProcessSpec::new("/bin/sleep", ["5"]);
        let token = CancelToken::new();
        let t2 = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            t2.cancel();
        });
        let out = run_process(&spec, &token, &mut |_, _| {}).unwrap();
        assert!(out.cancelled);
        assert_eq!(out.exit_code, None);
    }

    #[test]
    fn lingering_grandchild_does_not_block_completion() {
        let spec = ProcessSpec::new("/bin/sh", ["-c", "sleep 30 & echo done"]);
        let start = Instant::now();
        let out = run_process(&spec, &CancelToken::new(), &mut |_, _| {}).unwrap();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout, "done\n");
    }

    #[test]
    fn cancellation_kills_the_whole_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("survived");
        let script = format!("(sleep 1; touch '{}') & sleep 30", marker.display());
        let spec = ProcessSpec::new("/bin/sh", ["-c", script.as_str()]);
        let token = CancelToken::new();
        let t2 = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            t2.cancel();
        });
        let start = Instant::now();
        let out = run_process(&spec, &token, &mut |_, _| {}).unwrap();
        assert!(out.cancelled && start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1500));
        assert!(!marker.exists(), "background child must have been killed");
    }

    #[test]
    fn missing_program_is_an_error() {
        let spec = ProcessSpec::new("/definitely/not/here", Vec::<String>::new());
        assert_eq!(
            run_process(&spec, &CancelToken::new(), &mut |_, _| {})
                .unwrap_err()
                .kind(),
            "process"
        );
    }
}
