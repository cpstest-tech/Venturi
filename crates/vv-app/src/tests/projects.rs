use super::*;
use vv_media::test_support::TempDir;

fn temp_root(name: &str) -> TempDir {
    TempDir::new(&format!("vv-app-projects-test-{name}"))
}

fn media_item(path: PathBuf) -> vv_core::MediaItem {
    vv_core::MediaItem {
        path,
        meta: vv_core::MediaMeta {
            duration_frames: 100,
            fps: vv_core::Rational::new(30, 1),
            width: 1920,
            height: 1080,
            has_video: true,
            has_audio: false,
            sample_rate: 0,
            channels: 0,
            audio_streams: 0,
            file: Default::default(),
        },
        content_hash: 7,
        compound: None,
        folder: None,
    }
}

#[test]
fn new_projects_are_numbered_past_the_highest_one() {
    let root = temp_root("numbering");
    assert_eq!(next_project_name(&root, "Progetto"), "Progetto_01");
    create_project(&root, "Progetto_01").unwrap();
    create_project(&root, "Progetto_07").unwrap();
    create_project(&root, "Altro").unwrap();
    assert_eq!(next_project_name(&root, "Progetto"), "Progetto_08");
}

#[test]
fn a_created_project_is_a_folder_with_a_loadable_file_named_after_it() {
    let root = temp_root("create");
    let file = create_project(&root, "Progetto_01").unwrap();
    assert_eq!(file, root.join("Progetto_01").join("Progetto_01.vvproj"));
    assert!(vv_core::load_project(&file).is_ok());
    assert_eq!(list_projects(&root), vec![file]);
    assert!(matches!(
        create_project(&root, "Progetto_01"),
        Err(ProjectFsError::AlreadyExists)
    ));
}

#[test]
fn a_dot_in_the_name_is_kept_in_the_file_name() {
    assert_eq!(
        project_file(Path::new("/p/Clip 1.5")),
        PathBuf::from("/p/Clip 1.5/Clip 1.5.vvproj")
    );
}

#[test]
fn names_that_cannot_be_folders_are_refused() {
    for bad in ["", "   ", ".hidden", "a/b", "a\\b", "what?"] {
        assert!(validate_name(bad).is_err(), "{bad:?}");
    }
    assert_eq!(validate_name("  Vacanze 2026 ").unwrap(), "Vacanze 2026");
}

#[test]
fn renaming_moves_the_folder_and_the_media_copied_into_it() {
    let root = temp_root("rename");
    let file = create_project(&root, "Progetto_01").unwrap();
    let dir = file.parent().unwrap().to_path_buf();
    let mut project = vv_core::load_project(&file).unwrap();
    let inside = project
        .media_pool
        .insert(media_item(dir.join("library").join("a.mp4")));
    let outside = project
        .media_pool
        .insert(media_item(PathBuf::from("/elsewhere/b.mp4")));
    vv_core::save_project(&project, &file).unwrap();

    let renamed = rename_project(&file, "Vacanze").unwrap();

    assert_eq!(renamed, root.join("Vacanze").join("Vacanze.vvproj"));
    assert!(!dir.exists());
    let project = vv_core::load_project(&renamed).unwrap();
    assert_eq!(
        project.media_pool[inside].path,
        root.join("Vacanze").join("library").join("a.mp4")
    );
    assert_eq!(
        project.media_pool[outside].path,
        PathBuf::from("/elsewhere/b.mp4")
    );
}

#[test]
fn renaming_onto_an_existing_project_is_refused() {
    let root = temp_root("rename-taken");
    let file = create_project(&root, "A").unwrap();
    create_project(&root, "B").unwrap();
    assert!(matches!(
        rename_project(&file, "B"),
        Err(ProjectFsError::AlreadyExists)
    ));
    assert!(file.is_file());
}

#[test]
fn duplicating_copies_the_folder_under_a_free_name() {
    let root = temp_root("duplicate");
    let file = create_project(&root, "Progetto_01").unwrap();
    std::fs::write(file.parent().unwrap().join("note.txt"), "x").unwrap();

    let first = duplicate_project(&file, "copia").unwrap();
    let second = duplicate_project(&file, "copia").unwrap();

    assert_eq!(first, project_file(&root.join("Progetto_01 copia")));
    assert_eq!(second, project_file(&root.join("Progetto_01 copia 2")));
    assert!(first.parent().unwrap().join("note.txt").is_file());
    assert!(vv_core::load_project(&second).is_ok());
    assert!(file.is_file());
}

#[test]
fn deleting_removes_only_a_project_folder() {
    let root = temp_root("delete");
    let file = create_project(&root, "Progetto_01").unwrap();
    let stray = root.join("loose.vvproj");
    std::fs::write(&stray, "()").unwrap();

    assert!(matches!(
        delete_project(&stray),
        Err(ProjectFsError::NotAProjectFolder)
    ));
    delete_project(&file).unwrap();
    assert!(!file.parent().unwrap().exists());
    assert!(root.exists());
}

#[test]
fn importing_copies_an_old_project_into_its_own_folder() {
    let root = temp_root("import");
    let outside = temp_root("import-src");
    let src = outside.join("Matrimonio.vvproj");
    // A file from before the format version: no `format_version` field.
    let old = vv_core::project_to_string(&Project::default())
        .unwrap()
        .replace(
            &format!("format_version: {},", vv_core::CURRENT_FORMAT_VERSION),
            "",
        );
    assert!(!old.contains("format_version"));
    std::fs::write(&src, old).unwrap();

    let imported = import_project(&src, &root).unwrap();

    assert_eq!(imported, project_file(&root.join("Matrimonio")));
    assert_eq!(
        vv_core::load_project(&imported).unwrap().format_version(),
        vv_core::CURRENT_FORMAT_VERSION
    );
    assert!(src.is_file());
    let again = import_project(&src, &root).unwrap();
    assert_eq!(again, project_file(&root.join("Matrimonio 2")));
}

#[test]
fn aspect_classes_follow_the_presets() {
    assert_eq!(Aspect::of(Some((1920, 1080))), Aspect::Wide);
    assert_eq!(Aspect::of(Some((1080, 1920))), Aspect::Vertical);
    assert_eq!(Aspect::of(Some((1080, 1080))), Aspect::Square);
    assert_eq!(Aspect::of(Some((1440, 1080))), Aspect::Classic);
    assert_eq!(Aspect::of(Some((2560, 1080))), Aspect::Other);
    assert_eq!(Aspect::of(None), Aspect::Undefined);
}

#[test]
fn a_summary_reads_name_size_and_an_undefined_format_for_an_empty_project() {
    let root = temp_root("summary");
    let file = create_project(&root, "Progetto_01").unwrap();
    let summary = summarize(&file);
    assert_eq!(summary.name, "Progetto_01");
    assert!(summary.size_bytes > 0);
    assert!(summary.modified.is_some());
    assert_eq!(summary.duration_secs, None);
    assert_eq!(summary.aspect, Aspect::Undefined);
    assert!(summary.error.is_none());
}

#[test]
fn a_project_opened_from_elsewhere_is_renamed_in_place() {
    let root = temp_root("rename-loose-root");
    let dir = temp_root("rename-loose");
    let file = dir.join("vecchio.vvproj");
    vv_core::save_project(&Project::default(), &file).unwrap();
    assert!(!is_in_projects_folder(&file, &root));
    let managed = create_project(&root, "Progetto_01").unwrap();
    assert!(is_in_projects_folder(&managed, &root));

    let renamed = rename_loose_file(&file, "nuovo").unwrap();

    assert_eq!(renamed, dir.join("nuovo.vvproj"));
    assert!(renamed.is_file());
    assert!(!file.exists());
}
