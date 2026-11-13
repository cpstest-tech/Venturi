//! Projects on disk: one folder per project in the projects folder, with the
//! `.vvproj` inside named like the folder. Media stay where they are: the
//! folder holds the project file, its cover and what the editor copies next
//! to it (library media, voiceover takes).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vv_core::{ClipSource, Project, TimelineId, TrackKind};

pub const PROJECT_EXTENSION: &str = "vvproj";
const COVER_FILE: &str = ".cover.png";
const COVER_WIDTH: u32 = 320;

#[derive(Debug, thiserror::Error)]
pub enum ProjectFsError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Persistence(#[from] vv_core::PersistenceError),
    #[error("invalid name")]
    InvalidName,
    #[error("a project with this name already exists")]
    AlreadyExists,
    #[error("not a project folder")]
    NotAProjectFolder,
}

pub fn project_file(dir: &Path) -> PathBuf {
    // Not `with_extension`: a dot in the name would lose what follows it.
    let mut name = dir.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(PROJECT_EXTENSION);
    dir.join(name)
}

/// The folder `file` is the project file of, if it follows the layout.
fn project_dir(file: &Path) -> Result<&Path, ProjectFsError> {
    file.parent()
        .filter(|dir| project_file(dir) == file)
        .ok_or(ProjectFsError::NotAProjectFolder)
}

pub fn project_name(file: &Path) -> String {
    file.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The project files in `root`, in no particular order.
pub fn list_projects(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| project_file(&e.path()))
        .filter(|file| file.is_file())
        .collect()
}

/// A name usable as a folder on every system, trimmed.
pub fn validate_name(name: &str) -> Result<String, ProjectFsError> {
    let name = name.trim();
    let forbidden = |c: char| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
    if name.is_empty()
        || name.starts_with('.')
        || name.chars().any(|c| forbidden(c) || c.is_control())
    {
        return Err(ProjectFsError::InvalidName);
    }
    Ok(name.to_owned())
}

/// `{prefix}_NN`, one past the highest number already in `root`.
pub fn next_project_name(root: &Path, prefix: &str) -> String {
    let taken = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_prefix(prefix)?
                .strip_prefix('_')?
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    let mut n = taken + 1;
    loop {
        let name = format!("{prefix}_{n:02}");
        if !root.join(&name).exists() {
            return name;
        }
        n += 1;
    }
}

/// `base`, or `base 2`, `base 3`… whichever is free in `root`.
pub fn unique_name(root: &Path, base: &str) -> String {
    if !root.join(base).exists() {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !root.join(name).exists())
        .unwrap_or_else(|| base.to_owned())
}

/// An empty project, saved at once: it exists on disk from its first moment.
pub fn create_project(root: &Path, name: &str) -> Result<PathBuf, ProjectFsError> {
    let name = validate_name(name)?;
    std::fs::create_dir_all(root)?;
    let dir = root.join(&name);
    if dir.exists() {
        return Err(ProjectFsError::AlreadyExists);
    }
    std::fs::create_dir(&dir)?;
    let file = project_file(&dir);
    vv_core::save_project(&Project::default(), &file)?;
    Ok(file)
}

/// Media copied into the project folder are referenced by absolute path:
/// when the folder moves they must follow it.
pub fn rebase_media_paths(project: &mut Project, old_dir: &Path, new_dir: &Path) -> bool {
    let mut changed = false;
    for item in project.media_pool.values_mut() {
        if item.compound.is_some() {
            continue;
        }
        if let Ok(rest) = item.path.strip_prefix(old_dir) {
            item.path = new_dir.join(rest);
            changed = true;
        }
    }
    changed
}

fn rebase_file(file: &Path, old_dir: &Path, new_dir: &Path) -> Result<(), ProjectFsError> {
    let mut project = vv_core::load_project(file)?;
    if rebase_media_paths(&mut project, old_dir, new_dir) {
        vv_core::save_project(&project, file)?;
    }
    Ok(())
}

/// Whether `file` is a project of the Home in `root`, not one opened from
/// elsewhere: only those have a folder of their own to rename.
pub fn is_in_projects_folder(file: &Path, root: &Path) -> bool {
    project_dir(file).is_ok_and(|dir| dir.parent() == Some(root))
}

/// Renames the folder and the file in it; returns the new project file.
pub fn rename_project(file: &Path, new_name: &str) -> Result<PathBuf, ProjectFsError> {
    let new_name = validate_name(new_name)?;
    let dir = project_dir(file)?;
    let root = dir.parent().ok_or(ProjectFsError::NotAProjectFolder)?;
    let new_dir = root.join(&new_name);
    if new_dir == dir {
        return Ok(file.to_path_buf());
    }
    // A case-only rename on a case-insensitive disk finds itself.
    if new_dir.exists() && !same_file(&new_dir, dir) {
        return Err(ProjectFsError::AlreadyExists);
    }
    std::fs::rename(dir, &new_dir)?;
    let moved = new_dir.join(file.file_name().unwrap_or_default());
    let new_file = project_file(&new_dir);
    if moved != new_file {
        std::fs::rename(&moved, &new_file)?;
    }
    rebase_file(&new_file, dir, &new_dir)?;
    Ok(new_file)
}

/// Renames just the file, for a project opened from elsewhere.
pub fn rename_loose_file(file: &Path, new_name: &str) -> Result<PathBuf, ProjectFsError> {
    let mut name = std::ffi::OsString::from(validate_name(new_name)?);
    name.push(".");
    name.push(PROJECT_EXTENSION);
    let new_file = file.with_file_name(name);
    if new_file == file {
        return Ok(new_file);
    }
    if new_file.exists() && !same_file(&new_file, file) {
        return Err(ProjectFsError::AlreadyExists);
    }
    std::fs::rename(file, &new_file)?;
    Ok(new_file)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// A copy of the whole folder next to it, named `"{name} {suffix}"`.
pub fn duplicate_project(file: &Path, suffix: &str) -> Result<PathBuf, ProjectFsError> {
    let dir = project_dir(file)?;
    let root = dir.parent().ok_or(ProjectFsError::NotAProjectFolder)?;
    let name = format!("{} {suffix}", project_name(file));
    let new_dir = root.join(unique_name(root, &name));
    copy_dir(dir, &new_dir)?;
    let copied = new_dir.join(file.file_name().unwrap_or_default());
    let new_file = project_file(&new_dir);
    std::fs::rename(&copied, &new_file)?;
    rebase_file(&new_file, dir, &new_dir)?;
    Ok(new_file)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Removes the project folder. The original media are elsewhere and stay.
pub fn delete_project(file: &Path) -> Result<(), ProjectFsError> {
    let dir = project_dir(file)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

/// Copies a `.vvproj` saved anywhere into a new folder of `root`,
/// migrated to the current format. The original stays untouched.
pub fn import_project(src: &Path, root: &Path) -> Result<PathBuf, ProjectFsError> {
    let project = vv_core::load_project(src)?;
    let base = validate_name(&project_name(src))
        .unwrap_or_else(|_| t!("home.new_project_prefix").into_owned());
    std::fs::create_dir_all(root)?;
    let dir = root.join(unique_name(root, &base));
    std::fs::create_dir(&dir)?;
    let file = project_file(&dir);
    vv_core::save_project(&project, &file)?;
    Ok(file)
}

/// The timeline the editor left open (always a top-level one), else the
/// first.
pub fn main_timeline(project: &Project) -> Option<TimelineId> {
    project
        .view
        .active_timeline
        .filter(|id| project.timelines.contains_key(*id))
        .or_else(|| project.timelines.keys().next())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Aspect {
    Wide,
    Vertical,
    Square,
    Classic,
    Other,
    /// No timeline yet: the first media will decide.
    Undefined,
}

impl Aspect {
    pub const ALL: [Aspect; 6] = [
        Aspect::Wide,
        Aspect::Vertical,
        Aspect::Square,
        Aspect::Classic,
        Aspect::Other,
        Aspect::Undefined,
    ];

    pub fn of(resolution: Option<(u32, u32)>) -> Self {
        let Some((w, h)) = resolution.filter(|&(w, h)| w > 0 && h > 0) else {
            return Aspect::Undefined;
        };
        let ratio = w as f64 / h as f64;
        let near = |target: f64| (ratio / target - 1.0).abs() < 0.01;
        if near(16.0 / 9.0) {
            Aspect::Wide
        } else if near(9.0 / 16.0) {
            Aspect::Vertical
        } else if near(1.0) {
            Aspect::Square
        } else if near(4.0 / 3.0) {
            Aspect::Classic
        } else {
            Aspect::Other
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cover {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What a Home card shows.
#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub file: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    pub modified: Option<SystemTime>,
    pub duration_secs: Option<f64>,
    pub aspect: Aspect,
    pub cover: Option<Cover>,
    /// The file could not be read.
    pub error: Option<String>,
}

/// Reads the project: slow (it parses the file and may decode a frame),
/// meant for a background thread.
pub fn summarize(file: &Path) -> ProjectSummary {
    let dir = file.parent().unwrap_or(file);
    let modified = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    let mut summary = ProjectSummary {
        file: file.to_path_buf(),
        name: project_name(file),
        size_bytes: dir_size(dir),
        modified,
        duration_secs: None,
        aspect: Aspect::Undefined,
        cover: None,
        error: None,
    };
    match vv_core::load_project(file) {
        Ok(project) => {
            if let Some(id) = main_timeline(&project) {
                let timeline = &project.timelines[id];
                let fps = timeline.fps.as_f64();
                if fps > 0.0 {
                    summary.duration_secs = Some(timeline.total_frames() as f64 / fps);
                }
                summary.aspect = Aspect::of(Some(timeline.resolution));
            }
            summary.cover = cover(&project, dir, modified);
        }
        Err(e) => summary.error = Some(e.to_string()),
    }
    summary
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

/// The first video media of the main timeline, cached in the folder until
/// the project changes again.
fn cover(project: &Project, dir: &Path, project_modified: Option<SystemTime>) -> Option<Cover> {
    let cached = dir.join(COVER_FILE);
    let cache_modified = std::fs::metadata(&cached).and_then(|m| m.modified()).ok();
    if cache_modified.is_some()
        && cache_modified >= project_modified
        && let Some(cover) = read_png(&cached)
    {
        return Some(cover);
    }
    let Some(item) = first_video_media(project) else {
        let _ = std::fs::remove_file(&cached);
        return None;
    };
    let secs = item.meta.duration_frames as f64 / item.meta.fps.as_f64().max(1.0);
    let thumb = vv_media::generate_thumbnail(&item.path, secs, COVER_WIDTH).ok()?;
    let cover = Cover {
        width: thumb.width,
        height: thumb.height,
        rgba: thumb.rgba,
    };
    if let Some(png) = encode_png(&cover) {
        let _ = vv_core::write_atomic(&cached, &png);
    }
    Some(cover)
}

fn first_video_media(project: &Project) -> Option<&vv_core::MediaItem> {
    let timeline = &project.timelines[main_timeline(project)?];
    timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video)
        .flat_map(|t| t.clips.iter())
        .filter_map(|clip| match clip.source {
            ClipSource::Media(id) => Some((clip.timeline_start, id)),
            _ => None,
        })
        .filter_map(|(start, id)| {
            let item = project.media_pool.get(id)?;
            (item.compound.is_none() && item.meta.has_video).then_some((start, item))
        })
        .min_by_key(|(start, _)| *start)
        .map(|(_, item)| item)
}

fn encode_png(cover: &Cover) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, cover.width, cover.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&cover.rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

fn read_png(path: &Path) -> Option<Cover> {
    let bytes = std::fs::read(path).ok()?;
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    Some(Cover {
        width: info.width,
        height: info.height,
        rgba: buf,
    })
}

#[cfg(test)]
#[path = "tests/projects.rs"]
mod tests;
