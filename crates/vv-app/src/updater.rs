//! Looks up the latest GitHub release and installs it in place.

use serde::Deserialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

const LATEST_RELEASE: &str = "https://api.github.com/repos/cpstest-tech/tandem/releases/latest";

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Release {
    pub tag_name: String,
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    /// `sha256:<hex>`; missing on releases uploaded before GitHub added it.
    #[serde(default)]
    pub digest: Option<String>,
}

/// `v1.2.3` or `1.2.3-rc1` to `(1, 2, 3)`.
pub(crate) fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches('v');
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

pub(crate) fn is_newer(tag: &str, current: &str) -> bool {
    matches!((parse_version(tag), parse_version(current)), (Some(a), Some(b)) if a > b)
}

pub(crate) fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Method {
    /// The running AppImage file, swapped for the new one.
    AppImage(PathBuf),
    /// Installed by the Inno Setup installer: the new one runs at exit.
    WindowsInstaller,
    MacDmg,
    /// Distro package, source build, portable zip: only the download page.
    Manual,
}

impl Method {
    pub fn detect() -> Self {
        if cfg!(target_os = "linux")
            && let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from)
            && appimage.is_file()
        {
            return Self::AppImage(appimage);
        }
        if cfg!(windows)
            && std::env::current_exe()
                .ok()
                .and_then(|exe| Some(exe.parent()?.join("unins000.exe")))
                .is_some_and(|uninstaller| uninstaller.is_file())
        {
            return Self::WindowsInstaller;
        }
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Self::MacDmg;
        }
        Self::Manual
    }

    pub fn asset_name(&self) -> Option<String> {
        match self {
            Self::AppImage(_) => Some(format!("Tandem-{}.AppImage", std::env::consts::ARCH)),
            Self::WindowsInstaller => Some("Tandem-windows-x64-setup.exe".into()),
            Self::MacDmg => Some("Tandem-arm64.dmg".into()),
            Self::Manual => None,
        }
    }

    /// Release lacking this platform's file: the user gets the page instead.
    pub fn asset<'a>(&self, release: &'a Release) -> Option<&'a Asset> {
        let name = self.asset_name()?;
        release.assets.iter().find(|a| a.name == name)
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .user_agent(format!("Tandem/{}", current_version()))
        .build()
        .into()
}

/// The latest release when newer than this build.
pub(crate) fn check() -> Result<Option<Release>, String> {
    let body = agent()
        .get(LATEST_RELEASE)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(4 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    let release: Release = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok(is_newer(&release.tag_name, current_version()).then_some(release))
}

#[derive(Default)]
pub(crate) struct Progress {
    pub done: AtomicU64,
    pub cancel: AtomicBool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Installed {
    /// Already in place: restarting this program runs the new version.
    Replaced(PathBuf),
    /// Downloaded installer, still to run.
    Installer(PathBuf),
    /// Opened in Finder for the user to drag into Applications.
    DiskImage,
}

pub(crate) fn install(
    method: &Method,
    asset: &Asset,
    progress: &Progress,
) -> Result<Installed, String> {
    match method {
        Method::AppImage(target) => {
            let dir = target.parent().ok_or("AppImage without a folder")?;
            // Same folder: the final rename must not cross filesystems.
            let staging = dir.join(format!(".tandem-update-{}", std::process::id()));
            let result = download(asset, &staging, progress).and_then(|()| {
                set_executable(&staging)?;
                std::fs::rename(&staging, target).map_err(|e| e.to_string())
            });
            if result.is_err() {
                let _ = std::fs::remove_file(&staging);
            }
            result.map(|()| Installed::Replaced(target.clone()))
        }
        Method::WindowsInstaller => {
            let path = std::env::temp_dir().join(&asset.name);
            download(asset, &path, progress)?;
            Ok(Installed::Installer(path))
        }
        Method::MacDmg => {
            let path = std::env::temp_dir().join(&asset.name);
            download(asset, &path, progress)?;
            std::process::Command::new("open")
                .arg(&path)
                .status()
                .map_err(|e| e.to_string())?;
            Ok(Installed::DiskImage)
        }
        Method::Manual => Err("no installable file for this platform".into()),
    }
}

fn download(asset: &Asset, path: &Path, progress: &Progress) -> Result<(), String> {
    let mut response = agent()
        .get(&asset.browser_download_url)
        .call()
        .map_err(|e| e.to_string())?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(asset.size.saturating_add(1))
        .reader();
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0; 256 * 1024];
    let mut done = 0u64;
    loop {
        if progress.cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err("Cancelled".into());
        }
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        done += count as u64;
        progress.done.store(done, Ordering::Relaxed);
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let verified = verify(asset, done, hasher.finish().as_ref());
    if verified.is_err() {
        let _ = std::fs::remove_file(path);
    }
    verified
}

pub(crate) fn verify(asset: &Asset, size: u64, sha256: &[u8]) -> Result<(), String> {
    if size != asset.size {
        return Err(format!(
            "incomplete download ({size} of {} bytes)",
            asset.size
        ));
    }
    let Some(expected) = asset.digest.as_deref() else {
        return Ok(());
    };
    let Some(expected) = expected.strip_prefix("sha256:") else {
        return Err(format!("unsupported digest {expected}"));
    };
    let actual: String = sha256.iter().map(|b| format!("{b:02x}")).collect();
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err("the downloaded file does not match its checksum".into())
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn set_executable(_: &Path) -> Result<(), String> {
    Ok(())
}

/// Started from `on_exit`, once the window is gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Relaunch {
    App(PathBuf),
    Installer(PathBuf),
}

impl Relaunch {
    pub fn spawn(&self, project: Option<&Path>) -> std::io::Result<()> {
        let mut command = match self {
            Self::App(exe) => {
                let mut command = std::process::Command::new(exe);
                command.args(project);
                strip_appimage_env(&mut command);
                command
            }
            Self::Installer(setup) => {
                let mut command = std::process::Command::new(setup);
                // /CLOSEAPPLICATIONS: this process may still hold tandem.exe.
                command.args([
                    "/SILENT",
                    "/SP-",
                    "/SUPPRESSMSGBOXES",
                    "/NORESTART",
                    "/CLOSEAPPLICATIONS",
                ]);
                command
            }
        };
        command.spawn().map(drop)
    }
}

/// The old AppImage's mount disappears with this process; its paths must not
/// leak into the new one.
fn strip_appimage_env(command: &mut std::process::Command) {
    let Some(appdir) = std::env::var_os("APPDIR") else {
        return;
    };
    for name in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
        command.env_remove(name);
    }
    if let Some(paths) = std::env::var_os("LD_LIBRARY_PATH") {
        let kept: Vec<_> = std::env::split_paths(&paths)
            .filter(|p| !p.starts_with(&appdir))
            .collect();
        match std::env::join_paths(kept) {
            Ok(joined) if !joined.is_empty() => command.env("LD_LIBRARY_PATH", joined),
            _ => command.env_remove("LD_LIBRARY_PATH"),
        };
    }
}

pub(crate) fn spawn_download(
    method: Method,
    asset: Asset,
    on_done: impl Fn() + Send + 'static,
) -> (
    Arc<Progress>,
    std::thread::JoinHandle<Result<Installed, String>>,
) {
    let progress = Arc::new(Progress::default());
    let worker = progress.clone();
    let handle = std::thread::Builder::new()
        .name("update-download".into())
        .spawn(move || {
            let result = install(&method, &asset, &worker);
            on_done();
            result
        })
        .expect("cannot spawn the update thread");
    (progress, handle)
}

#[cfg(test)]
#[path = "tests/updater.rs"]
mod tests;
