//! Installs the local Remotion runtime on first use: portable Node, the pinned
//! npm packages and headless Chromium, all inside one data directory.
use super::{Runtime, process};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

const NODE_VERSION: &str = "v22.12.0";
const PACKAGE_JSON: &str = include_str!("../../../../tools/remotion-runtime/package.json");
const PACKAGE_LOCK: &str = include_str!("../../../../tools/remotion-runtime/package-lock.json");
const SETUP_BROWSER: &str = include_str!("../../../../tools/remotion-runtime/setup-browser.cjs");
const INSTALL_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Node,
    Packages,
    Browser,
    Isolation,
}

impl Step {
    pub fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::Node => t!("ai_sequence.setup_node"),
            Self::Packages => t!("ai_sequence.setup_packages"),
            Self::Browser => t!("ai_sequence.setup_browser"),
            Self::Isolation => t!("ai_sequence.setup_isolation"),
        }
    }
}

pub struct Setup {
    step: Arc<AtomicU8>,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<Runtime, String>>,
}

impl Setup {
    pub fn start(directory: PathBuf) -> Self {
        let step = Arc::new(AtomicU8::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let progress = step.clone();
        let stop = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("remotion-setup".into())
            .spawn(move || {
                let _ = tx.send(install(&directory, &progress, &stop));
            });
        if let Err(error) = spawned {
            let (tx, failed) = mpsc::channel();
            let _ = tx.send(Err(format!("Cannot start the runtime setup: {error}")));
            return Self {
                step,
                cancel,
                rx: failed,
            };
        }
        Self { step, cancel, rx }
    }

    pub fn step(&self) -> Step {
        match self.step.load(Ordering::Relaxed) {
            0 => Step::Node,
            1 => Step::Packages,
            2 => Step::Browser,
            _ => Step::Isolation,
        }
    }

    pub fn poll(&self) -> Option<Result<Runtime, String>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("The runtime setup stopped unexpectedly".into()))
            }
        }
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub fn default_directory() -> Option<PathBuf> {
    // Unit tests render the panel: they must never start a real download.
    if cfg!(test) {
        return None;
    }
    let library = crate::library::Library::default_root()?;
    Some(library.parent()?.join("remotion-runtime"))
}

/// Cheap file checks only; the sandbox itself is probed when a job starts.
pub fn ready(runtime: &Runtime) -> bool {
    let Ok(root) = runtime.directory.canonicalize() else {
        return false;
    };
    runtime.node.is_file()
        && runtime
            .browser
            .canonicalize()
            .is_ok_and(|browser| browser.is_file() && browser.starts_with(&root))
        && root.join("node_modules/@remotion/renderer").is_dir()
        && root.join("node_modules/@remotion/bundler").is_dir()
}

fn install(directory: &Path, step: &AtomicU8, cancel: &AtomicBool) -> Result<Runtime, String> {
    if !cfg!(target_os = "linux") {
        return Err("Local infographic rendering currently requires Linux".into());
    }
    std::fs::create_dir_all(directory).map_err(|e| format!("Cannot create {directory:?}: {e}"))?;
    let node = install_node(directory, cancel)?;
    step.store(1, Ordering::Relaxed);
    install_packages(directory, &node, cancel)?;
    step.store(2, Ordering::Relaxed);
    let browser = install_browser(directory, &node, cancel)?;
    step.store(3, Ordering::Relaxed);
    process::run(
        Command::new("bwrap").arg("--version"),
        cancel,
        Duration::from_secs(10),
    )
    .map_err(|_| {
        "Bubblewrap is missing: install the `bubblewrap` package (e.g. `sudo pacman -S bubblewrap`) and try again".to_owned()
    })?;
    Ok(Runtime {
        directory: directory.to_path_buf(),
        node,
        browser,
    })
}

fn node_platform() -> Result<&'static str, String> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("linux-x64"),
        "aarch64" => Ok("linux-arm64"),
        other => Err(format!(
            "No portable Node build for the {other} architecture"
        )),
    }
}

fn install_node(directory: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let target = directory.join("node");
    let executable = target.join("bin/node");
    let marker = target.join(".version");
    if executable.is_file() && std::fs::read_to_string(&marker).is_ok_and(|v| v == NODE_VERSION) {
        return Ok(executable);
    }
    let name = format!("node-{NODE_VERSION}-{}", node_platform()?);
    let base = format!("https://nodejs.org/dist/{NODE_VERSION}");
    let sums = String::from_utf8(download(
        &format!("{base}/SHASUMS256.txt"),
        1 << 20,
        cancel,
    )?)
    .map_err(|_| "Invalid Node checksum list")?;
    let archive_name = format!("{name}.tar.xz");
    let expected = sums
        .lines()
        .find_map(|line| {
            let (hash, file) = line.split_once(char::is_whitespace)?;
            (file.trim() == archive_name).then(|| hash.to_ascii_lowercase())
        })
        .ok_or("Node checksum not found")?;
    let archive = download(&format!("{base}/{archive_name}"), 200 << 20, cancel)?;
    let digest = ring::digest::digest(&ring::digest::SHA256, &archive);
    let actual: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected {
        return Err("The downloaded Node archive failed its checksum".into());
    }
    let staging = directory.join("node.partial");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let file = staging.join(&archive_name);
    std::fs::write(&file, &archive).map_err(|e| format!("Cannot save Node: {e}"))?;
    process::run(
        Command::new("tar")
            .arg("-xJf")
            .arg(&file)
            .arg("-C")
            .arg(&staging),
        cancel,
        Duration::from_secs(300),
    )
    .map_err(|e| format!("Cannot extract Node: {e}"))?;
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(staging.join(&name), &target).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::write(&marker, NODE_VERSION).map_err(|e| e.to_string())?;
    Ok(executable)
}

fn install_packages(directory: &Path, node: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let lock = directory.join("package-lock.json");
    let installed = directory.join("node_modules/.tandem-lock.json");
    std::fs::write(directory.join("setup-browser.cjs"), SETUP_BROWSER)
        .map_err(|e| e.to_string())?;
    if std::fs::read_to_string(&installed).is_ok_and(|text| text == PACKAGE_LOCK)
        && directory.join("node_modules/@remotion/renderer").is_dir()
    {
        return Ok(());
    }
    std::fs::write(directory.join("package.json"), PACKAGE_JSON).map_err(|e| e.to_string())?;
    std::fs::write(&lock, PACKAGE_LOCK).map_err(|e| e.to_string())?;
    let npm = node
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("lib/node_modules/npm/bin/npm-cli.js"))
        .ok_or("Node installation is incomplete")?;
    process::run(
        node_command(node)
            .arg(npm)
            .args(["ci", "--no-audit", "--no-fund", "--loglevel=error"])
            .current_dir(directory),
        cancel,
        INSTALL_TIMEOUT,
    )
    .map_err(|e| format!("Cannot install the Remotion packages: {e}"))?;
    std::fs::write(installed, PACKAGE_LOCK).map_err(|e| e.to_string())
}

fn install_browser(directory: &Path, node: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let output = process::run(
        node_command(node)
            .arg("setup-browser.cjs")
            .current_dir(directory),
        cancel,
        INSTALL_TIMEOUT,
    )
    .map_err(|e| format!("Cannot install Chromium: {e}"))?;
    let browser = output
        .lines()
        .find_map(|line| line.strip_prefix("Chromium executable:"))
        .map(|path| PathBuf::from(path.trim()))
        .ok_or("Chromium setup did not report its executable")?;
    if !browser.is_file() {
        return Err("Chromium setup reported a missing executable".into());
    }
    Ok(browser)
}

/// npm lifecycle scripts call `node` by name, so the portable one must come first.
fn node_command(node: &Path) -> Command {
    let mut command = Command::new(node);
    let bin = node.parent().unwrap_or(Path::new("."));
    let mut path = std::ffi::OsString::from(bin);
    if let Some(system) = std::env::var_os("PATH") {
        path.push(":");
        path.push(system);
    }
    command.env("PATH", path);
    command
}

fn download(url: &str, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| format!("Download failed ({url}): {e}"))?;
    let mut reader = response.body_mut().with_config().limit(limit).reader();
    let mut bytes = Vec::new();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|e| format!("Download failed ({url}): {e}"))?;
        if count == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

#[cfg(test)]
#[path = "../tests/ai_sequence/setup.rs"]
mod tests;
