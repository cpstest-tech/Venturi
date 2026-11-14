# Handoff — CapCut-style workflow

## Build status (phases 1, 2, 4; phase 5 at the end)

`cargo build --release` is clean, `cargo test --workspace` passes (the audio
test `timeline_audio::seeking_outside_the_window_restarts_fast_forward_from_there`
is timing sensitive and failed once under load, passing alone). Installed with
`scripts/install-linux.sh` into `~/.local` (`~/.local/bin` is not in `PATH`).
Not tried by hand yet: everything GUI-related below.

Fixes made while compiling: an unescaped quote in `locales/ribbon.yml` (it
broke the whole `i18n!` macro), a double borrow in `timeline_commands.rs`,
and the preview audio staying loaded when leaving a media preview with no
timeline (`TimelineAudio::clear_media`). Tests that assumed the old behaviour
now switch the Main Track magnet off (`timeline_state.magnet = false`), since
the magnet packs clips to the start by default.

### UI pass after the first build

- **Home card**: the date overflowed the card; the card is taller and every
  text line truncates.
- **Multifunction ribbon** (Library block): tabs Media, Audio, Text, Sticker,
  **Effects, Curves, Mixer, Color**. Effects shows the effects/presets list
  inline. Curves, Mixer and Color open inside the block (see "Fixes after
  the hand test"). The five toolbar toggles and the pool/effects column
  splitter were removed (`media_pool_open`, `effects_open`,
  `media_pool_fraction` remain in the settings file, unused).
- **Text previews**: cards in Add text, Yours and Favorites show the title as
  the compositor draws it (`vv_render::text::render_title`, project
  proportions, dark frame), cached per template and refreshed when the
  template or timeline size changes.

Spec: `docs/spec-workflow.md` · gap analysis: `docs/gap-analysis.md`.

## Phase 1 — Home, layout, top bar, saving

Status: **compiled and tested** (see "Build status" below).

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
- Open questions for later phases: Q1, Q6 and Q7 in the gap analysis (Q3 and Q5 were answered in phase 2).

## Phase 2 — Timeline

Status: **compiled and tested** (see "Build status" below).

### Decisions taken (user answers)

- **Q3, audio of video clips**: unchanged model. Video and audio stay two clips
  on separate tracks joined by `linked_group`; the waveform is drawn on the
  audio row. "Extract audio" unlinks the audio from the picture (or recreates
  it as unlinked clips on a free audio row if it was missing).
- **Q5, Magnet**: with the Magnet on, only the **Main Track** is kept free of
  holes. The global ripple stays as the separate `<` command (RippleDelete).
- **Main Track**: `Track::main` (serde default `false`). A new timeline gets
  `Track::new_main()`; `Timeline::main_track_index()` falls back to the first
  video track, so old projects need no migration and no version bump.
- **Linkage**: computed on the fly, nothing stored. A clip on another track
  follows the Main Track clip that contains its start
  (`workflow::linkage_anchors`).

### Done

- `vv-core/src/workflow.rs` (tests in `tests/workflow.rs`): `delete_side`
  (Q/W), `compact_main_track` (Magnet + Linkage, one undo step), `extract_audio`,
  `freeze_frame` (3 s still, later clips slide right), `duplicate_after`
  (Ctrl+D inserts after the selection), `edit_points` (↑/↓).
- Bar above the timeline (`timeline_bar.rs`): mouse-mode menu (Select, Blade,
  Select left/right, plus the existing Slip), Undo/Redo, Split, Delete
  left/right, Delete; on the right Magnet, Snapping, Linkage, Preview Axis and
  a log-scale zoom slider. Snapping moved here from the player bar.
- Empty state: ruler, empty Main Track and the hint (also drawn when the
  project has no timeline yet; the first media still creates it).
- Track header: 120 px fixed column, camera/note/T icon, "Main Track" label,
  Lock, Eye, Speaker. Eye = hides the layer (video tracks), Speaker = mutes
  (audio tracks); the other one is shown disabled with a tooltip. Solo, Arm for
  recording and Remove track moved to the header's right-click menu (the Main
  Track cannot be removed).
- Clips: colours per kind (video blue, audio green, text orange, effects
  indigo); filmstrip on video clips; thick `[` `]` bracket on the trim handle
  with the existing ↔ cursor; white border on selection (already there); fade
  dots always visible on audio clips.
- Modes: Blade cuts the clicked clip (and its linked group) where the pointer
  is, snapping included; Select left/right take every clip starting before/after
  the clicked one. In these modes a drag does not move clips.
- Magnet: after Move, Trim and Speed gestures, Del, Q/W, Cut and media drops,
  inside the same undo group as the gesture. Linkage: dragging a Main Track clip
  drags the clips anchored to it.
- Preview Axis: line under the pointer; the player shows that frame while the
  playhead stays (the playhead is swapped for the composition only).
- Right-click menu on clips: Freeze Frame, Reverse (disabled, «Presto
  disponibile»), Mirror, Extract audio, Copy, Cut, Paste, Duplicate,
  Disable/Enable, Delete. Right-click on an unselected clip selects it first.
- Shortcuts (`settings.rs`): Ctrl+B, Q, W, A/B/[/], Ctrl+D, P, N, S, Shift+Z,
  Home/End, ↑/↓, Shift+←/→, J/K/L, Ctrl+Shift+S, Ctrl+G, V, Ctrl+Y.
  Zoom is Ctrl+wheel (egui `zoom_modifier`), horizontal scroll moved to
  Alt+wheel.
- Tests: `vv-core/src/tests/workflow.rs`, gesture tests in
  `tests/timeline_ui_gestures.rs` (the harness now starts with Magnet and
  Linkage off, to keep the old gesture tests valid), app tests in
  `tests/main.rs`, defaults in `tests/settings.rs`.

### Remains / known limits

- **Compile, then `cargo clippy --all-targets -D warnings` and `cargo test`.**
  Then try the "Fatto quando" montage by hand: split, Q/W, Magnet ripple,
  snapping, Linkage, lock/hide/mute, zoom, undo of every gesture.
- Default shortcut changes that may surprise: Split T → Ctrl+B, Disable D → V,
  Select V → A, Slip B → Shift+A, Save As Ctrl+Shift+S → Ctrl+Alt+S (the spec
  gives Ctrl+Shift+S to Extract audio), "A" fast playback is unbound (L does it).
  Users with saved keymaps keep their old keys.
- **Filmstrip** tiles the single pool thumbnail: real per-position frames need
  a decoder job (the only public helper, `vv_media::generate_thumbnail`, seeks
  at most 1 s in and is engine code).
- **Waveform on video clips** is not drawn (Q3 kept the separate audio clip).
- **Reverse** is a placeholder (needs backward decoding in the engine). **J**
  scrubs the playhead backwards at 1×/2×/4×/8× without audio; the player does
  not play in reverse.
- Dragging a Main Track clip over a neighbour still overwrites it (existing
  behaviour) and the Magnet then closes the hole, instead of reordering the
  clips as CapCut does.
- Freeze Frame slides everything starting at the playhead on the unlocked
  tracks; a clip on another track that merely crosses the playhead stays put.
- Magnet, Linkage and Preview Axis are per session, not saved in the settings.
- Header: the Eye on audio tracks and the Speaker on video tracks are disabled.
- Pinch zoom uses egui's `zoom_delta` (already there); not tried on a trackpad.
- Compound Clip, keyframe diamonds and Ungroup (Alt+Shift+G) are later phases.

## Phase 4 — Base library (Media, Audio, Text, Sticker)

Status: **compiled and tested** (see "Build status" below).

### Decisions taken (user answers)

- **First media**: the project format is set when the first media is *inserted*
  in the timeline, not on import (import no longer creates a timeline).
- **Rotation**: read outside the engine with `ffprobe` (`first_media::probe_rotation`,
  cached per file). The clip is turned upright through its own Transform
  (rotation + zoom, one undo group with the insertion), so preview and export
  stay identical.
- **Stickers v1**: static images only (PNG/WebP...). SVG and GIF are listed in the
  file dialog and refused with a «presto disponibile» message.
- **Audio and Sticker storage**: the user's global library (`library.rs`);
  each category is a reserved top-level library folder (`ribbon-audio-music`,
  `ribbon-audio-sfx`, `ribbon-sticker-<id>`), created on first import.
- **Layout**: tab bar Media/Audio/Testo/Sticker on top of the Library block
  (`library_ribbon.rs`), then the category chips (Preferiti first). Media →
  «Media del progetto» is the old pool (with its Project/Library side tabs, so
  the raw user library stays reachable). The Effects list below is unchanged.

### Done

- **Rule of the first media** (`first_media.rs`): native size capped at 4K
  (same orientation, even sides), effective orientation with rotation, fps
  rounded to 24/25/30/50/60 (30 for a photo), ratio preset derived from the
  size (`Ratio::of`, «Original» if none matches).
- **Favorites** (`settings.favorites`, keys `media:<path>` / `asset:<id>` /
  `text:<id>`): star on hover on every item; saved in `settings.json`, so they
  outlive the app and every project.
- **«+» on hover** inserts at the playhead; drag inserts at the drop point.
  Photos 5 s, stickers 3 s, text 3 s. Stickers and text go on a free video
  track above the Main Track (or a new one) unless the timeline is empty.
- **Media**: search (pool header), «Importa» button (system file dialog),
  drop onto the block, Ctrl+I, dashed empty state «Trascina qui video, audio e
  foto o clicca per importare». Favorite media can be outside the project: they
  join the pool when inserted or dragged.
- **Audio**: Musica / Sound effect; a click previews (plays) the sound through
  the pool preview; Import / drop / Ctrl+I add files to the category shown.
- **Testo**: «Aggiungi testo», the template categories (Phase 10), «Tuoi»
  (templates in `settings.text_templates`; «Salva come template» button under
  the text properties; right click removes one).
- **Sticker**: Forme, Emoji, Frecce, Social, Altro; user content only.
- Strings in `locales/ribbon.yml` (en/it).
- Tests: `tests/first_media.rs`, `tests/library_ribbon.rs`, two cases in
  `vv-core/src/tests/edit.rs`. Updated: tests that expected import to create the
  timeline (`tests/main.rs`, `tests/main_export.rs`), the photo length (now
  5 s at 30 fps), `tests/pool_view.rs` (new `hover` field).

### Second pass (spec re-check)

- Category chips replaced by a **left sidebar** (Preferiti first; the Media and
  Sticker lists sit under a collapsible group) in `library_ribbon.rs`.
- Grid cards: **«Aggiunto»** tag top left (media already in a timeline),
  duration top right (`PoolEntry::added`, `paint_tile_tags`).
- Empty state: cyan «+», text «Trascina e rilascia video, foto e file audio qui».
  «Sound effect» renamed «Effetti sonori».
- Clippy and `cargo test -p vv-app` pass.
- Still not matching the spec: tools row (Importa + search + view/sort/filter
  icons in one line: Importa is on the tab row, search/sort live in the pool
  header); Audio rows as «Autore · durata» with round «+» (no author field in
  the library); Audio/Text top segments (they are sidebar entries); Text
  sidebar order «Aggiungi testo / Tuoi / Preferiti» (kept Preferiti first, the
  spec contradicts itself); tabs Transitions/Subtitles/Filters.

### Remains / known limits

- **Compile, then `cargo clippy --all-targets -D warnings` and `cargo test`.**
  Then try by hand: import by button, drag from the file manager and Ctrl+I;
  drag and «+» into the timeline; a portrait phone video as first media;
  stars after an app restart; click on a sound.
- Rotation needs `ffprobe` in `PATH`; without it the file is treated as upright.
  Later media with a rotation are turned upright too, but only the first one
  changes the project format.
- Audio-only first media keeps the old default (25 fps, 1920x1080).
- The ratio preset is derived, not stored: the Player's Ratio button and the
  project properties (next phases) own that value.
- «+» of a video goes to the first free video track at the playhead like a
  drop does: it overwrites what is under it on the Main Track.
- The «Tuoi» template keeps the text style only (not the Drop Shadow filter).
- SVG/animated stickers and sticker packs are placeholders.
- A removed category folder in the raw Library tab moves its assets to the
  library root (they leave the ribbon).
- `MediaDrag` payloads of favorites are imported into the pool (a pool step
  in the undo history) when the drag leaves the block.

## Phase D — Graphic style (CapCut look)

Spec: `docs/design-spec.md` (written from the user's text; values are estimates).

**Rule from now on:** every new piece of UI uses only the tokens in
`crates/vv-app/src/theme.rs` and follows `docs/design-spec.md`. No hardcoded
colour, radius, spacing or font size elsewhere. Icons: one open-source set
(Lucide or Phosphor); never CapCut assets, Pro/Free badges or EditPilot.

### Step 1 — tokens (done, not compiled, waiting for the user's OK)

- `theme.rs` now holds all tokens (surfaces, text, accent, clip colours,
  status, spacing, radii, font sizes, control heights, menu shadow) and
  `apply()` sets the whole egui style from them (widgets, selection, windows,
  menus, text sizes). Accent changed from the logo orange to cyan `#1fc8d6`.
- Replaced hardcoded values: clip colours, track/ruler backgrounds, selected
  clip border, playhead (now white), favorite gold, audio-meter colours.
- Not yet done (Step 2, per group): the many literal radii (2/3/4/6), grey
  `from_gray` overlays, white/black painter strokes, transition/crossing colours
  in `timeline_ui.rs`, and the DSP panels (`eq_panel`, `compressor_panel`,
  `mixer_panel`, `color_window`, `grade_panel`).
- Font: egui's default sans is used; Inter/Roboto is not bundled yet.

### Step 2 (done in one pass, compiled, tests run, installed)

- Inter Regular bundled (`assets/fonts`, OFL licence alongside), set as the
  first proportional font in `theme::apply`.
- Layout: window `BG_WINDOW`, blocks drawn as `BG_PANEL` cards with 6 px
  corners and an 8 px gap; tab bars restyled through the `egui_tiles` behavior
  (`workspace.rs`). Top bar: `BG_WINDOW`, Export is the solid cyan primary button.
- Player: header «Lettore – <timeline>» with the ≡ menu (zoom entries moved
  there); bottom bar with HH:MM:SS:FF timecode (current cyan / total grey),
  centred triangle Play/Pause, «Piena» and «Rapporto» menus, fit-to-view and
  full-screen icons (`transport.rs`). **Piena (1/2, 1/4) and Rapporto
  (16:9…) options were disabled here; Phase 3 wired them (see below).
- Home: cyan gradient «Nuovo progetto» banner replaces the button, «Progetti»
  section title, teal backdrop gradient, cards `size | duration`.
- Icons are drawn by hand (Lucide-like 1.5 px strokes); no icon font added.
- Not done: Properties tabs restructure (Phase 3), Home sidebar, export modal
  content (still the empty placeholder), 300 px context menus, per-literal radii
  in `timeline_ui.rs` and DSP panels (they follow the global egui style).

## Phase 3 — Player, Properties, Keyframes

Compiled; `cargo clippy --workspace --all-targets -D warnings` clean; all tests
pass (`timeline_audio::seeking_outside_the_window…` is timing sensitive: it
fails under load, passes alone). Not tried by hand: all GUI behaviour.

### Done

- **Player**: «Piena» (Piena/1/2/1/4 → `settings.preview_resolution`),
  «Rapporto» (Original/16:9/9:16/1:1/4:3, active one checked, label fixed) via
  the new undoable `SetTimelineResolution` (`vv-core`); `Ratio::resolution` keeps
  the longer side, Original = first video/photo upright. «Dettagli» uses the
  same `set_project_ratio`. Safe-zones icon = placeholder.
- **Canvas overlay** (`viewer_overlay.rs`): white 1 px outline, round white
  handles; dashed cyan guides only while moving a clip whose centre sits exactly
  on the canvas centre (snap within 6 px). Crop bars, rotation knob and pivot
  existed already and are unchanged: say if they must be hidden.
- **Dettagli** (`project_details.rs`): shown with nothing selected; «Modifica»
  opens Aspect Ratio (works), Frame Rate and colour space (disabled).
- **Properties tabs** (`properties_tabs.rs`, wired in `properties_panel.rs`):
  Video (Base / Rimuovi lo sfondo / Maschera / Ritocco), Audio, Velocità,
  Animazione, Regola. The old Selection tab was removed.
  - Base: Trasforma (Ridimensiona % slider, «Dimensione uniforme», position,
    rotation, anchor, flip, 6 alignment icons that move the clip), Ritaglio,
    Fusione (blend, matte, opacity), Stabilizza and Canvas [placeholder], text
    editor + «Salva come template» for text clips.
  - Maschera: the existing masks (rectangle/ellipse/path, feather, invert);
    Split/Filmstrip/Star/Heart [placeholder].
  - Audio: volume with keyframes, Fade In/Out in seconds (same value as the
    timeline dots, `SetClipFade`), Noise Reduction / Voice Changer [placeholder].
  - Velocità / Normale: 0.1×–100×, resulting duration, Keep pitch, ↺; committed
    on release (one undo step, `SetClipSpeed` with ripple). Curva [placeholder].
  - Regola: the existing filters and colour wheels; LUT, Basic, HSL, RGB curves
    [placeholder]. Animazione In/Out/Combo and text Bubble/Effects/Animation
    [placeholder]. Chroma Key, Auto Cutout, Enhance [placeholder].
- **Reset / keyframes**: ↺ icon in every section header and at the end of every
  parameter row (double click on the slider knob or the label also resets);
  Alt+click on a label or slider toggles the keyframe at the playhead; ‹ ◇ ›
  and "edit updates/creates the keyframe" already existed. Keyframe diamonds are
  drawn on the clip in the timeline (transform keyframes). Number fields take
  Up/Down arrows while focused.
- Tests: ratio/resolution (`tests/first_media.rs`), snap (`tests/viewer_overlay.rs`),
  diamonds (`tests/timeline_ui.rs`), command undo (`vv-core` `tests/workflow.rs`).

### Remains

- Frame Rate change (needs a command that conforms the clips to the new fps)
  and colour space: disabled in «Dettagli».
- Overlay and the panel work on the first selected *video* clip; photo/sticker/
  text clips use the same path but were not checked.
- Not wired to the engine (placeholders): stabilise, canvas blur/colour, chroma
  key, auto cutout, enhance, noise reduction, voice changer, speed curve,
  in/out/combo animations, LUT .cube, basic adjustments, HSL, RGB curves,
  star/heart/split/filmstrip masks, text bubble/effects/animation, safe zones.
- Diamonds show transform keyframes only (not gain/text/colour/filters).
- Easing between keyframes: linear only (as asked).

## Phase 5 — Effects, Filters, Transitions, Compound Clip

Status: **compiled, `cargo clippy --workspace --all-targets -D warnings` clean,
`cargo test --workspace` passes** (`timeline_audio::seeking_outside_the_window…`
is the known timing-sensitive one: fails under load, passes alone). Not tried by
hand: everything GUI (hover loops, drag and drop, context menu, shortcuts).

### Decisions taken (user answers)

- **Q1, engine boundary**: *adding* to `vv-render` is allowed, changing the
  existing pipeline is not. Done as a separate pass, see below.

### Done

- **Engine (additive)**: `vv-render/src/assets.rs` (manifest + library, built-ins
  embedded by `build.rs`, user folder `$XDG_DATA_HOME/venturi/assets`),
  `custom_shader.rs` (WGSL prelude + pipeline cache with error scope) and
  `Compositor::render_custom_pass`. The existing passes are untouched. Format and
  shader contract: `docs/ASSETS.md`. 18 assets in `vv-render/assets/`: 6 effects
  (Pulse, Glitch, Shake, Soft blur, Chromatic aberration, Mirror), 6 filters
  (Vintage, Noir, Warm, Cool, Vivid, Cinematic), 6 transitions (Dissolve, Dip to
  black, Wipe, Slide, Zoom, Pixelate).
- **Model** (`vv-core`): `EffectStack::assets: Vec<AssetFx>` (id, enabled, params)
  and `Transition::asset: Option<AssetFx>` (kind stays `CrossDissolve` for readers
  that do not know assets). `set_clip_assets` command. `fx.rs`:
  `adjacent_neighbor`, `adjacent_pairs`, `asset_crossing`, `apply_asset_transition`,
  `ungroup_compound_clip`. Adjacency tolerance is 1 frame (`ADJACENT_TOLERANCE`,
  also in `Track::crossing_at`).
- **Rendering, preview = export** (`vv-session/src/frame_provider.rs`): one code
  path for both. An effect on a clip composes that clip alone, runs the passes and
  puts it back as a texture (it sees transform, filters and masks, not opacity or
  blend). An effect *element* is an adjustment clip carrying assets: `stack_layers`
  composes everything below it, runs the passes and replaces the stack; its fades
  and opacity scale how much of the effect shows. An asset transition merges both
  sides (each live, held at the edges of its media) into one layer.
- **Library tabs** (`fx_library.rs`): Effects, Filters, Transitions in the ribbon.
  Full-width search, sidebar (Preferiti first, then the manifest categories; the
  old hand-made list stays under «Classici» in Effects), square cells with the
  label below (truncated), star and «+» on hover. Favorites key `fx:<id>`.
- **Previews**: 160×90 bundled sample pictures drawn in code, the same shader
  through the compositor. Cells show a still; on hover a 36-frame loop at 18 fps
  (transitions go A→B, hold, B→A) is rendered 6 frames per UI frame and cached as
  textures, so the first hover does not stall.
- **Use**: drag a cell onto a video clip (the whole selection if the clip is in
  it); onto free timeline space it becomes an adjustment clip (default 3 s) that
  processes the tracks below; «+» does the same at the playhead (new track).
  Transitions: dropping on a clip picks the nearest cut; no adjacent clip there →
  «Nessuna coppia di clip adiacenti trovata» (toast). «+» uses the selected pair,
  else the cut nearest to the playhead. Default 0.5 s, clamped to twice the
  shorter clip (what the existing crossing model can lend).
- **Properties**: «Effetti e filtri» section in the Regola tab (switch, sliders,
  double click resets, remove); the transition panel shows the asset name and its
  parameters. Slider drags are one undo step (the app merges commands while the
  pointer is down).
- **Compound Clip**: Alt+G (and the right-click entry) groups the selection with
  its linked audio; Alt+Shift+G (and right-click) ungroups: each nested layer goes
  back with its own effects and position, trimmed to what the container showed,
  link groups and transitions remapped, unused container removed from the pool,
  one undo step. New actions `MakeCompound` / `UngroupCompound` in the keymap.
- Tests: `vv-render` (assets, every shader compiles, transitions reach A at 0 and
  B at 1), `vv-core/tests/fx.rs`, `vv-session` (effect on a clip, element acts only
  below and only for its length, asset transition, tolerance), `vv-app/tests/fx_library.rs`.

### Remains / known limits

- **Try by hand**: hover loop smoothness, drag onto clip / empty space / non
  adjacent clips, Alt+G round trip on a real project, export of a clip with an
  effect, a transition and an element.
- Asset parameters are constants (no keyframes yet).
- Assets are read at startup: no hot reload of the user folder.
- An asset effect on a clip with a matte, or an element with a matte, is ignored.
- Ungrouping refuses a container with its own effects, fades, speed or freeze
  (toast) instead of dropping them.
- A one-frame gap between clips still takes a transition, but that gap frame has
  no clip and shows nothing.
- OTIO export writes an asset transition as a plain dissolve; the MCP tools do not
  know assets.
- Effects use `fx_time()` from the timeline fps; very long effect elements keep
  running time-based shaders without looping.
- The old single-edge transitions and the built-in filters are still reachable
  (Effects → Classici) and unchanged.

## Phase 6 — Subtitles and Export

Written after a crash that cut the session: the code was re-read and the
missing pieces rebuilt. **Nothing was compiled** (rule of the phase): expect
a first round of compiler errors.

### Done

- **Transcription** (`crates/vv-transcribe`, new): whisper.cpp through
  `whisper-rs`; the model is downloaded on first use into `models/` next to the
  library root; language automatic or manual; progress and cancel; runs on a
  worker thread (`subtitles.rs`). `vv-app` now depends on it (Cargo.toml and
  Cargo.lock edited by hand).
- **Words are kept apart from the style** (`vv-core/src/captions.rs`,
  `Project::captions`): choosing a template rebuilds the clips from the saved
  words, no new transcription, one undo step (`SetCaptions`). Templates are
  split into «word by word» and «riga». «Aggiungi sottotitoli» is a placeholder.
- **Subtitles tab** in the library ribbon (`subtitles_ui.rs`, `subtitles.yml`).
- **Export modal** (`export_modal.rs`, logic in `export_plan.rs`): two columns,
  cover thumbnail with «✎ Modifica copertina», name/path/browse, Video, Audio and
  Captions sections (checkbox + collapse arrow), footer with the live estimate
  `(video + audio Mbps) × s ÷ 8`, progress with frame preview, ETA, cancel
  (partial files removed) and «Apri cartella». Ctrl+E now opens it (was
  Ctrl+Shift+E). Match the best video: bitrate by ffprobe on a thread at open,
  resolution and fps from the media meta.
- **Engine** (done before the crash, needed by the modal): target bitrate
  (CBR/VBR), export fps, separate audio file, preview frames, cleanup of
  partial files.
- **Cover**: the Main Track pencil and the modal button open the same picker
  (`cover.rs`), now wired in `main.rs`. Missing locale keys added.

### Remains / known limits

- **Compile and run**; then try: generate, switch template (no re-transcription),
  export with Match the best video on clips of different bitrates, compare the
  file with the preview.
- Only H.264 is offered; HEVC and AV1 are hidden until the engine has them.
  MOV goes through the same encoder (extension picks the muxer): check it.
- `export_dialog.rs`, `show_export_dialog` and `tests/export_dialog.rs` are now
  unused: delete after the first build. The old progress window still serves
  exports started by MCP.
- The modal exports the In/Out range when marks are set (the estimate follows).
- No unit tests for `export_modal.rs` (it needs the GPU/ffprobe); the plan
  logic is covered in `tests/export_plan.rs`.

## Phase 7 — AI features and final pass

**Nothing was compiled or run** (rule of the phase): the checks below are a
code read plus three small fixes. Expect a first round of compiler errors
(`transport.rs`, `app_menu.rs` were touched).

### Done

- **AI features**: checked against the code and the gap analysis. The engine has
  none of them (no denoiser, voice effect, segmentation, enhancer or video
  stabilizer; Silero VAD and the optical flow of Smooth Cut are not the same
  thing), so Auto Cutout, Noise Reduction, Voice Changer, Enhance and Stabilize
  stay disabled [placeholder] with «Presto disponibile»
  (`properties_tabs.rs`). Nothing was wired.
- **Dead controls**: the only clickable one found was the player's safe-zones
  icon (hover text only). It is now disabled with «Presto disponibile».
  The other placeholders were already disabled.
- **Shortcuts**:
  - defaults are unique (existing test `default_shortcuts_are_unique`);
  - keys are ignored while a text field has focus, a menu is open or a shortcut
    is being captured (already there);
  - new: they are also ignored while the export modal, the cover picker or the
    unsaved-changes dialog is open (Del/Q/W/Space used to reach the timeline
    behind them).
- **Layout menu** (reset, reopen closed blocks, re-dock detached ones), empty
  states (Home, Media, Timeline) and the undo grouping of properties gestures
  (`apply_effect_changes`, closed every frame once the pointer is up) were read
  and look right; not exercised.

### Placeholders left

Stabilize, Canvas (blur/colour), Chroma Key, Auto Cutout, Enhance, Noise
Reduction, Voice Changer, Speed curve, Animation In/Out/Combo, text
Bubble/Effects/Animation, LUT/Basic/HSL/RGB curves, masks Split/Filmstrip/Star/
Heart, Reverse, safe zones, «Aggiungi sottotitoli»
(manual), SVG/GIF stickers and sticker packs, Frame Rate and colour space in
«Dettagli», HEVC/AV1 export.

### Known bugs / limits

- **Not verified (needs a build and a hand run)**: project save → reopen →
  export of a test project; preview identical to export (one code path in
  `frame_provider.rs` by design, but never compared on a file); opening old
  `.vvproj` files; undo/redo of every new action.
- Space/Enter on a focused egui button both press it and fire the shortcut.
- If the Home is opened during a held gesture, its undo group closes only on
  return to the editor.
- Everything under "Remains / known limits" of phases 1–6 still applies
  (Wayland window placement, detached-window shortcuts, filmstrip tiling one
  thumbnail, no reverse playback, asset params not keyframeable, old export
  dialog files to delete after the first build, timing-sensitive audio test).

## Phase 8 — Text style model

Branch `feat/text-style-model`. A text clip is still a normal clip (keyframes,
transform, ratio); only `TitleParams` grew.

### Done

- **Model** (`vv-core/src/text_style.rs`, `TitleParams::style`):
  - Fill: solid (the old `color`), linear or radial gradient with N stops and
    angle; the gradient `offset` is a `Keyframed<f32>` (new
    `KeyframeValue/Target::GradientOffset`, shown in the keyframe editor).
  - Stacked strokes (colour, width, join round/miter/bevel), stacked shadows
    (offset, blur, colour, opacity; blur 0 = hard shadow), stacked glows
    (radius, intensity, colour), 3D extrusion (depth, angle, colour).
  - Background (`TitleBackground`): shape rounded rect / pill, colour or
    gradient, padding (in font sizes, was a constant 0.2), border (already
    there), rotation. Typography was already complete (family, weight,
    tracking, line spacing, case, align).
- **Schema**: `CURRENT_FORMAT_VERSION` 1 → 2. Every new field has a serde
  default, so no migration step is needed: old files load as plain text
  (tests in `vv-core/src/tests/persistence.rs`: title without `style`, a
  format-1 project, a styled round trip). The old title shadow still migrates
  to the Drop Shadow filter.
- **Rendering** (`vv-render/src/text.rs`, new `text_fx.rs`): the style is baked
  into the existing stack of solid-colour coverage masks, so the compositor,
  decoder and encoder are untouched and preview = export. Layer order, bottom
  up: background, shadows, glows, extrusion, strokes (widest first), fill.
  A gradient is N stacked layers (layer k fades in between stop k-1 and k:
  exact in the interior, edges are antialiased per layer). Only
  `text.rs` and the new file changed in `vv-render`, plus `register_font`.
- **Fonts** (`crates/vv-app/assets/fonts`, one `OFL-<family>.txt` each,
  registered at startup by `fonts.rs`): Anton, Bebas Neue, Bangers, Bungee,
  Fredoka, Montserrat, Poppins (Regular/Bold/Black), Playfair Display,
  Abril Fatface, Pacifico, Lobster, Caveat, Shrikhand, Monoton, Righteous,
  Rubik Mono One, Press Start 2P, Space Grotesk, Archivo Black.
  **Permanent Marker is not included**: upstream it is Apache 2.0, not OFL.
  Fredoka, Montserrat, Playfair, Caveat and Space Grotesk are variable fonts
  (weight works, tested on Montserrat). A one-weight font asked for a weight
  it does not have (e.g. 700 on Anton) falls back to another font: the panel
  already resets the weight when the family changes.
- **Panel** (`text_style_panel.rs`, called from `title_editor`): live preview
  (the same render as the library cards), then Fill / Stroke / Shadow / Glow /
  3D sections (add/remove items, reset per section) and the extra Background
  rows (shape, gradient, padding, rotation). It uses the existing row widgets,
  so tokens come from `theme.rs`. Editing one field only touches that field on
  the other selected titles (`apply_title_edit`), and never drops the offset
  keyframes. Strings in `locales/style.yml`.
- **Tests**: `vv-render/src/tests/text.rs::styles` renders and checks every
  effect (set `VV_TEXT_SHEET_DIR=<dir>` to also dump a PPM per case, for a
  visual check); `vv-app/src/tests/fonts.rs` (families + licences);
  command undo and `apply_title_edit` cases.

### Remains / known limits

- Not tried by hand: the panel, live preview and an export of a styled title.
- (Done in Phase 10: the library now ships 16 ready-made text templates.)
- The old Drop Shadow filter (Text properties) and the new Shadow section are
  independent; both can be on.
- Gradient stop alpha is honoured only per layer (no alpha interpolation
  between stops). Gradient offset wraps (a seam shows unless the first and
  last stop match). Background gradient offset is not animatable.
- Masks are full frame: a title with many layers costs memory at 4K (cache is
  6 titles). `grow`/`blur` work on the title's bounding box only.
- OTIO export ignores the style (Resolve text has no equivalent).
- Not run clean: `cargo clippy` flags dead code in `export_dialog.rs` /
  `export_modal.rs` and `vv-transcribe` (existing, untouched).

## Phase 9 — Text animations

Branch `feat/text-animations` (from `feat/text-style-model`). Engine
(compositor, decoder, encoder) untouched.

### Done

- **Easing library** (`vv-core/src/easing.rs`, `Easing`): linear, quad/cubic
  in/out/in-out, back, elastic, bounce (each in/out/in-out), spring.
  `Interpolation::Curve(Easing)` plugs it into keyframes (old files unchanged);
  the keyframe editor toolbar got an «Easing» dropdown (`locales/easing.yml`).
- **Descriptors as data** (`vv-core/src/text_anim.rs`, `TitleParams::anim`,
  serialised with the project, format version 2 -> 3, all fields default):
  `TextAnimations { enter, exit, looped, karaoke }`. An `AnimDescriptor` has
  duration, stagger (0-0.95 of the duration spread over the units), order
  (forward / backward / random / from centre), granularity (block / line /
  word / char), easing, a list of `PropTrack`s (position, scale, rotation,
  flip, opacity, blur, tint, tracking, reveal, gradient offset, glow; `jitter`
  and loop `wave`), an optional `Decor` (underline, scribble, bar, side bar,
  trim-path) and `mirror` (Out = In played backwards). Presets are plain
  descriptors built in `text_anim_presets.rs`: In fade, slide, pop, bounce,
  typewriter, slam, mask reveal, wave, blur, flip 3D, glitch + underline,
  scribble, expanding bar; Out = the same mirrored + dissolve, fall; Loop
  pulse, float, wave, shake, glow pulse, shimmer, flicker, gradient flow.
  Picking one copies the descriptor into the title (templates keep it).
- **Evaluation** is a pure function: `plan(anims, text, AnimState)`; the state
  comes from `TitleParams::animated(secs, clip_secs)` (called in
  `frame_provider::build_layer`, so preview and export share it). Noise is a
  hash of (unit, 1/24 s step): no RNG state. States stop changing once an
  animation is over, so caches keep hitting.
- **Rendering** (`vv-render/src/text_anim_render.rs`): each glyph group gets
  its whole style (strokes, shadows, glow, 3D, gradient) as a cropped sprite,
  cached; per frame the sprites are moved/scaled/rotated/blurred/wiped into
  the usual layer stack (`TitleRender`). Layout is the same as the static
  path, so the last frame of an In equals the plain title (tested).
- **Karaoke**: `Karaoke { color, scale, keep_past, word_ms }`; `word_ms` are
  the transcription words (`captions::clip_word_ms`, filled by «Sync with
  transcription» in the panel); without a matching transcript the words are
  spread evenly over the clip.
- **Panel** (`text_anim_panel.rs`, Animation tab of a text clip): In / Out /
  Loop sections with preset buttons, duration, stagger, easing, split-by and
  order, a Karaoke section, and an animated preview box that plays the preset
  under the pointer (or the current animations). Strings in `locales/anim.yml`.
- **Tests**: `vv-core` `tests/easing.rs`, `tests/text_anim*.rs` (units, orders,
  stagger, mirror, loop period, determinism for every preset, karaoke, RON
  round trip); `vv-render` `tests/text_anim_render.rs` (same state = same
  pixels after clearing the caches, finished In = static title, reveal,
  typewriter, decor growth, karaoke tint, every preset renders).

### Remains / known limits

- Not tried by hand: the panel, hover preview and an export of an animated
  title.
- Gradient offset animation moves the whole block (not per unit) and rebuilds
  the sprites each frame: heavy with character granularity at 4K.
- The background pill fades with the mean opacity of the units but does not
  move. Underline/strikethrough follow their glyphs.
- Tracking is a per-unit shift around the line centre, not a re-layout.
- Flip 3D is a squash (cos of the angle), not a real perspective.
- Blur is limited to the sprite margin plus its own padding; glow pulse uses
  its own halo in the tint colour (the style's Glow section is not touched).
- Karaoke is not set up by the subtitle templates yet (`build_clips`); the
  per-word times are copied when the user enables it in the panel.
- Keyframe diamonds still cover transform keyframes only.

## Phase 10 — Text template library

Branch `feat/text-templates` (from `feat/text-animations`). Engine
(compositor, decoder, encoder) untouched. The «Text effects» placeholder is
gone.

### Done

- **Format** (`vv-core/src/text_template.rs`, `TextTemplateDef`): versioned
  JSON (`version` 1; a newer file is refused). Fields: `id`, `name`,
  `category` (titles, social, lower_third, karaoke, retro, elegant), `tags`,
  `duration` (s), `layers[]`, `subtitle` (`word_by_word`, `size`). A layer is
  text + font + colour + position/anchor + `fill` gradient + `strokes`,
  `shadows`, `glows`, `extrude`, `background` (pill / rounded) + `anim`
  (`in` / `out` / `loop` = a preset id from `text_anim_presets` with optional
  `duration`, `stagger`, `granularity`, `tint`; `karaoke`) + `start` / `length`
  inside the template. Colours are `#RRGGBB[AA]`. Measures are for a 1080-px
  frame and scale with the timeline height. Several layers = title +
  subtitle + pill and so on.
- **The 16 templates** (`crates/vv-app/assets/text-templates/*.json`,
  embedded by `text_templates.rs`): Neon Pulse, Bubble Pop, Hype Impact (3
  layers), Sticker Pack, Gradient Wave, Retro 80s (2), Handwritten Marker,
  Glitch RGB, Karaoke Pop, Comic Boom, Elegant Serif (2), Pixel Game, Fire &
  Ice, Sunset Script, Outline Hollow, Lower Third Bold (2). **The table with
  fonts/colours/effects was not in the prompt (the placeholder was left in),
  so look and animations are my own design from the names**: check them
  against the real table and edit the JSON. Only bundled fonts are used
  (Permanent Marker is not OFL, so Handwritten Marker uses Caveat).
- **Text tab** (`library_ribbon.rs`): sidebar Favorites, Add text (default +
  all), Titles, Social, Lower thirds, Karaoke, Retro, Elegant, Yours. A search
  box (name, tags, category; searches every template) sits above the grid.
  A card shows the layers at rest; **under the pointer it plays In, hold, Out
  in a loop** (all layers, with their `start` offsets). Star = favorite
  (`text:<id>` key, bundled and user alike), «+» or a click inserts at the
  playhead, dragging onto the timeline inserts there. Right click: «Usa come
  stile dei sottotitoli» (and Remove for the user's).
- **Insertion** (`insert_text_template`, `edit::insert_title_len`): one clip
  per layer, same start, the first on the overlay track found free, each next
  one on a new video track above; one undo step. The clips are ordinary text
  clips: content, colours, font, style and the Animation tab stay editable;
  «Salva come template» (text properties) keeps working (single layer).
- **Subtitles**: `Captions::template` may be `text:<id>` (bundled or a user
  template). `captions::build_clips_with` builds the clips from the saved
  words with that look (first layer, bottom of the frame, size from
  `subtitle.size`); no new transcription. A template with karaoke gets the
  word times of each line. The Subtitles tab lists them under «Stili di
  testo». Karaoke Pop is the one that uses karaoke.
- **Tests**: `vv-core/src/tests/text_template.rs` (colours, scaling,
  presets, bad files, round trip, subtitle look);
  `vv-app/src/tests/text_templates.rs` (16 load, fonts registered, every
  layer has In and Out and renders ink, search; set `VV_TEXT_SHEET_DIR=<dir>`
  to dump a PPM per template);
  `tests/library_ribbon.rs` (categories, multi-layer insert and undo, drop,
  hover timeline, search, subtitle styling).

### Remains / known limits

- Not tried by hand: the grid, the hover animation, dragging, and an export of
  a template.
- **Hollow text is not possible** with this model: a stroke is a dilated
  mask, so a transparent fill still shows the stroke inside. Outline Hollow
  uses a near-black fill instead.
- Positions are in 1080-px units from the centre: Lower Third Bold sits at
  the bottom left of a 16:9 frame; on a portrait timeline its x offset is
  too far to the left.
- «Salva come template» saves the selected clip only: a multi-layer template
  cannot be saved from several clips yet. User templates last 3 s.
- Subtitles from a template use its first layer only (no pill / second line).
  Word-by-word reveal is supported by the format but no bundled template sets
  it.
- Names are not translated (brand-style names); categories are.

## Fixes after the hand test

Branch `feat/text-templates`. Compiled, `cargo test --workspace` passes,
installed with `scripts/install-linux.sh`, and the three fixes were tried in
the real app (XWayland, pointer driven by `ydotool`).
`cargo clippy --workspace --all-targets -D warnings` is clean except for
what was there before: dead code in `export_dialog.rs` / `export_modal.rs`,
and `large_enum_variant` on `frame_provider::OwnedContent`, which the new
toolchain flags.

### 1. Effects and filters as a timeline layer

- **Cause**: free-space drops were only taken over the track rows and landed on
  the video track under the pointer: the empty space above the tracks did
  nothing, and a gap on the Main Track got an element with nothing below it.
- **Model**: `Track::effects` (serde default) marks an effects track.
  `fx::insert_fx_element` puts an element (an adjustment clip carrying
  `assets`) on the effects track under the pointer if it has room, else on the
  topmost one with room, else on a new one on top (`AddTrack::effects`). One
  undo step.
- **Schema**: format 3 → 4. `migrate` calls `fx::mark_effects_tracks`: a
  non-main video track holding only effect elements becomes an effects track.
- **UI**: a drop anywhere in the timeline that no clip takes (also above
  or between the rows) makes an element (3 s); «+» does the same at the
  playhead. The new element is selected. The header shows the fx icon and
  «Effetti»; Eye and Lock work as on any video track. Moving, trimming,
  splitting, deleting and duplicating are the clip gestures. Selected, the
  Properties panel shows only «Effetti e filtri», with ↺ per parameter and per
  section.
- **Rendering**: unchanged. `stack_layers` already applied an element to
  everything below for its length, in the shared preview/export path. New
  tests cover two clips, moving the element, its Eye, and the tracks above it.
- **Limits**: a clip dragged onto an effects track (or an element onto a plain
  track) is not refused. The track flag stays as it is, and the element
  still works there.

### 2. Subtitle template previews

- **Cause**: the hover loop works in the current build (it was added but never
  committed). What was still wrong is that the templates name no font, so the
  bold weights fell back to an unrelated system font (an almost monospaced
  one here). The preview then did not show the template's look.
- `SubtitleTemplate::title` now uses the bundled variable Montserrat
  (`captions::SUBTITLE_FONT`), so preview, real subtitles and export use the
  same face on every machine. Rendering is live (≈0.3 ms per frame), so no
  cached loop is needed.

### 3. Editors inside the Multifunction block

- **Cause**: Curves (the keyframe curve editor), Mixer and Color were
  `egui::Window`s drawn after the Properties block.
- `ribbon_editors.rs`: the three draw into the block in place of the tab
  content, under a «← title» header. The tab underneath is kept (and with it
  the scroll position, held by egui), so «←», or clicking a library tab,
  goes back to it. They follow the block when it is detached. Their commands go
  through the same `apply_effect_changes` as Properties, so one gesture is
  one undo step. The `*_open` window flags were removed from the settings
  (old keys are ignored). The Color toggle in Properties opens the editor in
  the block.
- **Tab strip**: one row that never wraps. When the block is narrow it
  scrolls, with «‹ ›» arrows on the sides (dimmed at the ends).
- **Other editors still in a window (not changed, waiting for a decision)**:
  - the audio effect settings opened from the mixer (EQ, compressor,
    normalize…, `mixer_panel::effect_window`);
  - the VST3 plugin GUIs (native windows, `plugin_editors.rs`);
  - the Speed dialog (`speed_dialog.rs`);
  - the marker editor (`timeline_markers.rs`).

  Dialogs (settings, presets, paste attributes, relink, new timeline…) are not
  editors and stay windows.
