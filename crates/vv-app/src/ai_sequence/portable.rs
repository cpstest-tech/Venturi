use std::path::{Path, PathBuf};

pub fn directory(project_file: &Path) -> PathBuf {
    let mut name = project_file.file_name().unwrap_or_default().to_os_string();
    name.push(".ai-sequences");
    project_file.with_file_name(name)
}

pub fn rebase(project: &mut vv_core::Project, old_file: &Path, new_file: &Path) -> bool {
    let old = directory(old_file);
    let new = directory(new_file);
    if old == new {
        return false;
    }
    let mut changed = false;
    for item in project
        .media_pool
        .values_mut()
        .filter(|item| item.compound.is_none())
    {
        if let Ok(relative) = item.path.strip_prefix(&old) {
            item.path = new.join(relative);
            changed = true;
        }
    }
    changed
}

pub fn copy(old_file: &Path, new_file: &Path) -> std::io::Result<bool> {
    let from = directory(old_file);
    let to = directory(new_file);
    if from == to || !from.try_exists()? {
        return Ok(false);
    }
    if std::fs::symlink_metadata(&from)?.file_type().is_symlink() {
        return Err(std::io::Error::other(
            "AI source storage must not be a symbolic link",
        ));
    }
    std::fs::create_dir_all(to.parent().unwrap_or(Path::new(".")))?;
    std::fs::create_dir(&to)?;
    if let Err(error) = copy_contents(&from, &to) {
        let _ = std::fs::remove_dir_all(&to);
        return Err(error);
    }
    Ok(true)
}

fn copy_contents(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if matches!(
            entry.file_name().to_str(),
            Some("node_modules" | "job.pending" | ".queue.lock")
        ) {
            continue;
        }
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_symlink() {
            return Err(std::io::Error::other("AI sources contain a symbolic link"));
        }
        if kind.is_dir() {
            std::fs::create_dir(&target)?;
            copy_contents(&entry.path(), &target)?;
        } else if kind.is_file() {
            let mut input = std::fs::File::open(entry.path())?;
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
        } else {
            return Err(std::io::Error::other("AI sources contain a special file"));
        }
    }
    Ok(())
}

pub fn rename_in_directory(old_file: &Path, new_file: &Path) -> std::io::Result<()> {
    let old_name = directory(old_file)
        .file_name()
        .unwrap_or_default()
        .to_os_string();
    let from = new_file.parent().unwrap_or(Path::new(".")).join(old_name);
    let to = directory(new_file);
    if from == to || !from.try_exists()? {
        return Ok(());
    }
    if to.try_exists()? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "AI source storage already exists at the destination",
        ));
    }
    std::fs::rename(from, to)
}

pub fn check_rename(old_file: &Path, new_file: &Path) -> std::io::Result<()> {
    let from = directory(old_file);
    if !from.try_exists()? {
        return Ok(());
    }
    if std::fs::symlink_metadata(&from)?.file_type().is_symlink() {
        return Err(std::io::Error::other(
            "AI source storage must not be a symbolic link",
        ));
    }
    let name = directory(new_file)
        .file_name()
        .unwrap_or_default()
        .to_os_string();
    let target = old_file.parent().unwrap_or(Path::new(".")).join(name);
    if target != from && target.try_exists()? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "AI source storage already exists under the new project name",
        ));
    }
    Ok(())
}
