# Handoff — CapCut-style workflow

Spec: `docs/spec-workflow.md` · gap analysis: `docs/gap-analysis.md`.

## Phase 1 — Home, layout, top bar, saving

Status: **code written, not compiled and no tests run** (the phase rules say
"do not compile"). Formatting was checked with `/usr/bin/rustfmt --edition
2024 --check`. Note that the rustup `stable` toolchain on this machine is
broken (missing `librustc_driver`, missing manifest). `Cargo.lock` was updated
with the 1.93.1 toolchain's `cargo metadata`, which resolves dependencies
without building anything. The only addition is `egui_tiles 0.17.1`.

### Decisions taken (user answers)

- Project on disk: **one folder per project** in `~/Videos/Projects`
  (`Progetto_01/Progetto_01.vvproj`, plus a `.cover.png` cache). Media stay
  where they are, referenced by absolute path. Media copied inside the folder
  (`library/`, voiceover takes) are rebased when the folder is renamed or
  duplicated.
- Blocks host the **existing panels**: Library = media pool/effects list,
  Player = viewer, Properties = properties panel, Timeline = timeline. The
  secondary windows (keyframes, mixer, colour) and their toolbar toggles are
  unchanged.
- Old projects: **imported into the folder**. "Importa progetto…" on the Home,
  or dropping a `.vvproj` on the Home, copies the file migrated to the current
  format and opens it. File → Open, Recents and the command line still open a
  file where it is.
- The Home's "formato" filter means **aspect ratio**: 16:9, 9:16, 1:1, 4:3,
  Altro, and Non definito for a project with no timeline yet.

### Done

- **Schema version** (`vv-core/src/persistence.rs`): `Project::format_version`
  is always written as `CURRENT_FORMAT_VERSION = 1`. A file without the field
  loads as version 0 and is migrated (0 → 1 needs no step). A newer file is
  refused with `PersistenceError::NewerFormat` instead of failing to parse.
  Saves are **atomic**: the project goes to a `.<name>.tmp` file, which is
  fsynced and then renamed over the target. The settings file uses the same
  path.
- **Home** (`home.rs`, `projects.rs`):
  - card grid with cover, name, size on disk, duration and modified date;
  - search, sort (most recent / name / size) and the format filter;
  - card menu and right-click menu: Apri, Rinomina, Duplica, Elimina (with
    confirmation), Mostra nella cartella;
  - "Nessun progetto" empty state;
  - the projects folder can be changed from the Home ("Cambia…");
  - "Nuovo progetto" creates `Progetto_NN` (one past the highest number) on
    disk and opens it, without asking anything.
- **Workspace** (`workspace.rs`): an `egui_tiles` tree.
  - Default layout: Library | Player | Properties on top (1:2:1), Timeline
    full width below (3:2).
  - Dividers resize the blocks, tab headers drag them, and the tab "×" closes
    one.
  - "🗗" moves a block to its own OS window (egui immediate viewport). "Riaggancia",
    or closing that window, puts it back.
  - The layout, including the detached windows and their size and position,
    is saved in `settings.json` (`workspace`) and restored at startup.
  - Blocks are drawn after the tree, in the frame's logical order (library →
    timeline → playback logic → properties → player), so no block lags a
    frame behind another.
- **Top bar** (`app_menu.rs`):
  - left: 🏠 Home, then the existing menus plus a new **Layout** menu
    ("Ripristina layout di default", reopen the closed blocks, dock the
    detached ones);
  - centre: the project name (click to rename it, which moves the folder too)
    and «Salvataggio…» / «Salvato» / «Salvataggio non riuscito»;
  - right: **Esporta**, which opens an empty modal. The old export dialog is
    still under File → Export.
- **Autosave** (`autosave.rs`):
  - saves ~2 s after the last confirmed edit, and at least every 30 s while
    edits are pending;
  - nothing is saved while a gesture holds an undo group open, so a drag is
    still one undo step and one save;
  - the save is written on a background thread from a copy of the project;
  - before Home, project switch, rename and window close, a blocking flush
    replaces the old "save changes?" question. That question remains only for
    untitled projects or a failed save;
  - a failed save is retried after 10 s.
- **Crash recovery**: `settings.open_project` is set while a project is open
  and cleared on a clean exit or on going Home. If it is still set at startup,
  that project is reopened from its last save, a leftover temp file is
  removed, and a notice is shown.
- UI strings: `locales/home.yml`, `locales/workspace.yml`, plus additions to
  `project.yml` and `menu.yml` (it/en).
- Tests:
  - new: `tests/{projects,home,workspace,autosave}.rs` and new cases in
    `tests/settings.rs`, `vv-core/src/tests/persistence.rs` and
    `tests/main.rs` (new project, Home, crash recovery);
  - updated: the old `new_project_discards_after_confirmation` test (replaced:
    New no longer asks) and the legacy-slotmap test (it checked that the word
    "version" was absent, which `format_version` now breaks).

### Remains / known limits

- **Compile, then `cargo clippy --all-targets -D warnings` and `cargo test`.**
  Then check the "Fatto quando" criteria by hand: create a project in one
  click; move a block to a second monitor; close and reopen; open an old
  `.vvproj`.
- Wayland does not let the app place windows. A detached block comes back
  detached and at the right size, but not on the monitor it was on (X11,
  Windows and macOS restore the position).
- Detached windows:
  - shortcuts work only while that window has focus;
  - Enter / Alt+← (enter or leave a compound clip) typed in a detached
    Properties or Player window can be lost;
  - files dropped onto a detached window are ignored;
  - the "🗗" glyph comes from egui's emoji font; check that it renders.
- While the Home is shown, the detached windows close. They come back at the
  geometry they were first opened with in this session.
- "Elimina" deletes the folder permanently (no system trash).
- Sizes use a decimal point; the date format comes from the locale file.
- `menu.inspector` and the panel-width settings (`inspector_open`,
  `left_column_width`, `inspector_width`, `timeline_height`) are gone. Their
  old keys in `settings.json` are ignored.
- Open questions for later phases: Q1, Q3, Q5, Q6 and Q7 in the gap analysis.
