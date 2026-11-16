use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start local process: {e}"))?;
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
