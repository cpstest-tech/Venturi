use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// A CLI on PATH. npm installs `.cmd` shims on Windows, which would go
/// through cmd.exe and its quoting: their script is run with Node instead.
pub fn program(name: &str) -> Command {
    #[cfg(windows)]
    if let Some(command) = windows_program(name) {
        return command;
    }
    Command::new(name)
}

#[cfg(windows)]
fn windows_program(name: &str) -> Option<Command> {
    let path = std::env::var_os("PATH")?;
    let found = std::env::split_paths(&path)
        .flat_map(|dir| ["exe", "cmd", "bat"].map(|ext| dir.join(format!("{name}.{ext}"))))
        .find(|candidate| candidate.is_file())?;
    if found
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Some(Command::new(found));
    }
    let dir = found.parent()?;
    let script = std::fs::read_to_string(&found)
        .ok()
        .and_then(|shim| npm_shim_script(&shim).map(|script| dir.join(script)))
        .filter(|script| script.is_file());
    let Some(script) = script else {
        return Some(Command::new(found));
    };
    let local_node = dir.join("node.exe");
    let mut command = if local_node.is_file() {
        Command::new(local_node)
    } else {
        Command::new("node")
    };
    command.arg(script);
    Some(command)
}

/// The script an npm cmd-shim runs: `"%dp0%\node_modules\pkg\cli.js" %*`.
#[cfg(any(windows, test))]
pub(crate) fn npm_shim_script(shim: &str) -> Option<&str> {
    const PREFIX: &str = "\"%dp0%\\";
    shim.lines()
        .rev()
        .filter_map(|line| line.find(PREFIX).map(|start| &line[start + PREFIX.len()..]))
        .filter_map(|rest| rest.split('"').next())
        .find(|script| !script.is_empty() && !script.ends_with(".exe"))
}

pub fn run(
    command: &mut Command,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<String, String> {
    run_with_input(command, cancel, timeout, None)
}

pub fn run_with_input(
    command: &mut Command,
    cancel: &AtomicBool,
    timeout: Duration,
    input: Option<Vec<u8>>,
) -> Result<String, String> {
    if input
        .as_ref()
        .is_some_and(|input| input.len() > 16 * 1024 * 1024)
    {
        return Err("Provider input exceeds 16 MB".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    crate::spawn::no_console(command);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start local process: {e}"))?;
    #[cfg(windows)]
    let tree = ProcessTree::new(&child);
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let descriptors = [
            child.stdout.as_ref().map(|stream| stream.as_raw_fd()),
            child.stderr.as_ref().map(|stream| stream.as_raw_fd()),
            child.stdin.as_ref().map(|stream| stream.as_raw_fd()),
        ];
        for descriptor in descriptors.into_iter().flatten() {
            let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
            if flags < 0
                || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Cannot configure nonblocking provider pipes".into());
            }
        }
    }
    let finished = Arc::new(AtomicBool::new(false));
    let writer = input.map(|input| {
        let mut stdin = child.stdin.take().unwrap();
        let finished = finished.clone();
        std::thread::spawn(move || {
            let mut offset = 0;
            while offset < input.len() {
                if finished.load(Ordering::Relaxed) {
                    return Err("Provider stopped before reading its input".to_owned());
                }
                match stdin.write(&input[offset..]) {
                    Ok(0) => return Err("Provider input pipe closed".into()),
                    Ok(count) => offset += count,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            Ok(())
        })
    });
    let overflow = Arc::new(AtomicBool::new(false));
    fn reader<R: Read + Send + 'static>(
        stream: R,
        overflow: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let mut stream = stream;
            let mut buffer = [0u8; 8192];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) if output.len() + count <= 4 * 1024 * 1024 => {
                        output.extend_from_slice(&buffer[..count])
                    }
                    Ok(_) => {
                        overflow.store(true, Ordering::Relaxed);
                        if finished.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if finished.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            output
        })
    }
    let stdout = reader(
        child.stdout.take().unwrap(),
        overflow.clone(),
        finished.clone(),
    );
    let stderr = reader(
        child.stderr.take().unwrap(),
        overflow.clone(),
        finished.clone(),
    );
    let started = Instant::now();
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("Cancelled".to_owned());
        }
        if started.elapsed() > timeout {
            break Err("Local process timed out".to_owned());
        }
        if overflow.load(Ordering::Relaxed) {
            break Err("Local process exceeded the 4 MB output limit".to_owned());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => break Err(e.to_string()),
        }
    };
    // Descendants may retain the pipes even after their parent exits.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    drop(tree);
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    finished.store(true, Ordering::Relaxed);
    let input_result = writer
        .map(|writer| {
            writer
                .join()
                .map_err(|_| "Provider input writer failed".to_owned())
                .and_then(|result| result)
        })
        .unwrap_or(Ok(()));
    let output =
        String::from_utf8_lossy(&stdout.join().map_err(|_| "Output reader failed")?).into_owned();
    let errors =
        String::from_utf8_lossy(&stderr.join().map_err(|_| "Error reader failed")?).into_owned();
    let status = result?;
    if overflow.load(Ordering::Relaxed) {
        return Err("Local process exceeded the 4 MB output limit".into());
    }
    if !status.success() {
        return Err(format!(
            "Local process exited with {status}: {}",
            errors.chars().take(8000).collect::<String>()
        ));
    }
    input_result?;
    Ok(output)
}

#[cfg(test)]
#[path = "../tests/ai_sequence/process.rs"]
mod tests;

/// Windows' counterpart of the process group: closing the job kills every
/// process the child started.
#[cfg(windows)]
pub(crate) struct ProcessTree(Option<windows_sys::Win32::Foundation::HANDLE>);

#[cfg(windows)]
impl ProcessTree {
    pub(crate) fn new(child: &std::process::Child) -> Self {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        // SAFETY: plain Win32 calls on handles owned here or by `child`.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Self(None);
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) != 0
                && AssignProcessToJobObject(job, child.as_raw_handle()) != 0;
            if !ok {
                CloseHandle(job);
                return Self(None);
            }
            Self(Some(job))
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        if let Some(job) = self.0.take() {
            // SAFETY: the handle was created in `new` and is closed once.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        }
    }
}
