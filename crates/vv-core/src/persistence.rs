//! Saving and loading the project in RON.

use crate::model::Project;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Bumped whenever a file needs a step in `migrate` to load right.
pub const CURRENT_FORMAT_VERSION: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("RON error: {0}")]
    Ron(#[from] ron::Error),
    #[error("RON error: {0}")]
    RonParse(#[from] ron::error::SpannedError),
    #[error(
        "saved by a newer version of Tandem (format {found}, this one reads up to {supported})"
    )]
    NewerFormat { found: u32, supported: u32 },
}

pub(crate) fn serialize_current_format_version<S: serde::Serializer>(
    _: &u32,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_u32(CURRENT_FORMAT_VERSION)
}

pub fn project_to_string(project: &Project) -> Result<String, PersistenceError> {
    Ok(ron::ser::to_string_pretty(
        project,
        ron::ser::PrettyConfig::default(),
    )?)
}

pub fn save_project(project: &Project, path: &Path) -> Result<(), PersistenceError> {
    write_atomic(path, project_to_string(project)?.as_bytes())?;
    Ok(())
}

/// Written beside `path` and renamed over it: a crash mid-write leaves the
/// previous file whole.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let tmp = temp_path(path);
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written?;
    // The rename itself is durable only once the directory is synced.
    #[cfg(unix)]
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty())
        && let Ok(dir) = std::fs::File::open(dir)
    {
        let _ = dir.sync_all();
    }
    Ok(())
}

pub fn temp_path(path: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(".tmp");
    path.with_file_name(name)
}

/// Read before the project itself, so a file from a newer version is told
/// apart from a broken one.
#[derive(serde::Deserialize)]
#[serde(rename = "Project")]
struct VersionProbe {
    #[serde(default)]
    format_version: u32,
}

pub fn load_project(path: &Path) -> Result<Project, PersistenceError> {
    let contents = std::fs::read_to_string(path)?;
    project_from_str(&contents)
}

pub fn project_from_str(contents: &str) -> Result<Project, PersistenceError> {
    let found = ron::from_str::<VersionProbe>(contents).map_or(0, |p| p.format_version);
    if found > CURRENT_FORMAT_VERSION {
        return Err(PersistenceError::NewerFormat {
            found,
            supported: CURRENT_FORMAT_VERSION,
        });
    }
    let mut project: Project = ron::from_str(contents)?;
    migrate(&mut project, found);
    // `Clip::rate` is derived from the fps: recomputing it here fixes
    // projects saved before the field existed (clips at an fps different
    // from the timeline's, out of sync).
    project.refresh_clip_rates();
    Ok(project)
}

/// Brings a project read from format `from` up to the current one.
fn migrate(project: &mut Project, from: u32) {
    // 0 → 1 introduced the version, 1 → 2 the text style, 2 → 3 the text
    // animations: serde defaults fill older files.
    debug_assert!(from <= CURRENT_FORMAT_VERSION);
    if from < 4 {
        crate::fx::mark_effects_tracks(project);
    }
    project.set_format_version(CURRENT_FORMAT_VERSION);
}

#[cfg(test)]
#[path = "tests/persistence.rs"]
mod tests;
