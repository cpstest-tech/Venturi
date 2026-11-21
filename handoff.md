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

## Automatic cut

Implemented in `vv-core/src/auto_cut{,_edit}.rs` and
`vv-app/src/auto_cut{,_ui}.rs`, using the existing Whisper/FFmpeg pipeline.
Includes local source caching, cancellable background analysis, full-screen
review, virtualized segments, preview skipping and a single undo command with
linked ripple, animation edge handling and 15 ms sample crossfades. Project
format is now 5; new fields default for older files. No new dependency or
network service. Speech models must already be installed through Subtitles.

**Not compiled, built, run or tested**, as requested. Unit tests were written
in the existing separate test files. Architecture, decisions, limits and manual
verification steps: `docs/AUTO_CUT.md`.

## Short-form round 1 — auto zoom, beat sync, safe zones

Compiled, `cargo clippy --workspace --all-targets -D warnings` clean, `cargo test
--workspace` passes (new: 11 auto-zoom, 8 beat-sync, 4 beat-detection, 5 safe-zone
tests). Not tried by hand: all GUI behaviour (checklist below). No LLM, no
network, **no new dependency**: beat detection is our own Rust (see below);
transcription is the existing Whisper engine.

### 1. Auto zoom

- **Where**: «Zoom automatici» button in the timeline bar (next to Taglio
  automatico) opens Properties → Video → **Zoom auto** (new sub-tab).
- **Data**: the Automatic cut source cache (`auto_cut::load_source`: SHA-256 of
  the file, Whisper words, 10 ms energy, disk cache in the library's
  `auto-cut/`). Opening the tab only *looks the cache up* (`transcribe: false`);
  without a transcript it shows «Genera trascrizione», which runs the existing
  engine with progress and Cancel. Source selection (linked audio first) and the
  clip-local mapping were moved into `speech_source` / `clip_local`, now shared
  with Automatic cut.
- **Heuristic** (`vv-core/src/auto_zoom.rs`, pure): per word, a score from its
  peak dB above the median of the words within ±4 s (+6 dB = full), sentence
  start (first word, pause ≥ 0.6 s or `.!?` before) and the pause before it.
  Frequency sets the score threshold; the greedy pick (best score first) keeps
  the minimum gap. A zoom holds until the phrase ends (pause ≥ 0.35 s), between
  0.6 and 2.5 s.
- **Output**: ZoomX/ZoomY keyframes (and PositionX/Y with «Sposta il centro»):
  rest → peak with Cubic Out (fast in), hold, then Cubic In-Out back to rest
  (smooth out). Zooms too close to return to rest go straight from one level to
  the next. Values multiply the clip's own zoom, so a rotated or scaled clip
  keeps it. The generated keys are listed in `Clip::auto_keyframes`
  (serde default, skipped when empty, **no format bump**: an older app reads
  the file and just treats them as hand-made).
- **Parameters** (saved in `settings.json` → `shorts.auto_zoom`): intensity
  105–125 %, easing Lento/Normale/Rapido (in 0.25/0.15/0.08 s, out
  0.7/0.45/0.3 s), frequency Bassa/Media/Alta (default gap 4/2.5/1.5 s),
  minimum gap 0.5–10 s, alternate levels (every other zoom at 55 % of the
  intensity), centre shift (±40 % of the margin the zoom frees, fixed
  4-step pattern).
- **Preview**: «Anteprima» applies the keyframes as a real history step; while
  it is pending, changing a parameter undoes it and applies the new one (on
  slider release). «Applica» keeps it, «Annulla» undoes it. Any other edit
  makes it final.
- **Remove**: «Rimuovi zoom automatici» removes only the keys still equal to
  what was generated: hand-made keys and generated keys the user moved or
  changed stay. One undo step.

### 2. Beat sync

- **Library choice**: librosa is Python and aubio is a C library (bindgen + C
  toolchain on every platform). `vv-audio/src/beat.rs` implements the same
  method as `librosa.beat_track` in ~300 lines of Rust, reusing the radix-2 FFT
  of `spectrum.rs` (made `pub(crate)`): 40-band log spectral flux (22.05 kHz,
  2048/512), tempo by autocorrelation weighted by a log-normal prior at
  120 BPM (50–220 BPM), beats by dynamic programming (Ellis 2007, tightness
  100), weak edge beats trimmed. Streamed: memory does not grow with the song.
- **Errors**: shorter than 5 s, silent, no clear rhythm (autocorrelation peak
  < 0.12 of lag 0, or < 4 beats), missing file (offline), no audio stream,
  freeze/disabled/compound clips. Unstable BPM (12 s windows deviating > 4 %
  on average) and a weak rhythm (clarity < 0.25) are warnings: the beats are
  placed anyway.
- **UI**: «Beat» button in the timeline bar toggles a small floating window
  (simplest choice: the Properties panel has no Audio tab for photos, and the
  same controls serve a music clip and a video selection). Analysis of the
  selected clip (an audio-track clip first) runs on a worker with progress
  and Cancel; results are cached per source for the session.
- **Markers**: ordinary timeline markers with `Marker::beat = true` (serde
  default, cyan), one undo step (`beat_sync::SetMarkers`). They move, get
  edited and get deleted like any marker. Re-analysing replaces the beat
  markers over that clip only; user markers stay. «Rimuovi i marker dei beat».
- **Snap**: «Snap ai beat» (saved, `shorts.beat_snap`) adds or removes beat
  markers from the existing snap targets (`snap_extra` in `timeline_ui.rs`):
  moves, trims, Blade cuts and drops snap to them when Snapping is on. The
  ruler playhead still snaps to clip edges only.
- **Tagli sui beat** (`beat_sync::cuts_on_beats`): the selected video-track
  clips/photos, in timeline order, go one after the other on the track of the
  first, from the first beat at or after it, each lasting 1/2/4 beats
  (`shorts.beat_every`). A video shorter than its slot keeps its length (gap
  left). Linked audio follows. Past the last beat marker the median beat
  period is extrapolated. What is underneath is overwritten
  (`make_room_for_ranges`). Same preview → Applica/Annulla pattern, one undo
  step. Refused with a message: no clips, < 2 beat markers, no beat after the
  first clip, locked track.

### 3. Safe zones

- **Config**: `crates/vv-app/assets/safe_zones.json` (embedded, version 1):
  per platform `id`, `name`, `zones[]` with `label` {it, en} and `x, y, w, h`
  in percent from the top-left of a 9:16 frame. A `safe_zones.json` next to
  `settings.json` (`~/.config/tandem/`) overrides it; it is re-read every time
  the overlay is switched on. An unreadable override falls back to the
  bundled file and says why at the bottom of the player.
- **Player**: the safe-zones icon is now a toggle (cyan when on) with a
  platform menu next to it (TikTok, Instagram Reels, YouTube Shorts). Both are
  saved (`shorts.safe_zones`, `shorts.safe_zone_platform`).
- Painted with the UI painter over the viewer rect (`paint_safe_zones` in
  `main.rs`), so it cannot reach the export or the decoder. Not shown in the
  full-screen viewer or while browsing a pool item.
- **Warning**: while paused, each visible text clip under the playhead
  (subtitles included) whose drawn box enters a zone gets a yellow outline and
  «Testo in una zona coperta».
- **The zone values are estimates** from the commonly published 1080×1920
  guides, not measured on the apps: check them on a device and edit the JSON.

### Known limits

- Auto zoom works on one clip at a time (the first selected); on a clip with
  speed ≠ 1 the times follow the speed; a zoom whose span already holds a
  hand-made zoom/position key is skipped (so manual work is never
  overwritten). A generated key added between two hand-made keys changes the
  shape of that segment if it was not linear.
- The beat analysis decodes the whole media file even if the clip uses part
  of it; beats are placed only inside the clip. No downbeat/bar detection.
- Tagli sui beat does not run the Magnet afterwards and does not reorder:
  clips keep their timeline order.
- Safe zones are drawn for any ratio, but the values only make sense on 9:16.
- The beat window and the auto-zoom tab look up the source file every frame
  while open (two `stat` calls, no read).

### Manual test checklist

1. Auto zoom: select a talking-head clip with no transcript → Zoom auto →
   «Genera trascrizione» → cancel halfway, retry, let it finish. Anteprima,
   move each parameter, Applica; play it back; Ctrl+Z / Ctrl+Y.
2. Add a hand-made zoom key, regenerate, then «Rimuovi zoom automatici»: the
   hand key stays. Retouch one generated key: it stays too.
3. Missing model (rename `models/`), offline file, photo, disabled clip: each
   shows its message.
4. Beat: select a music clip → Rileva i beat; check BPM on 3–4 known songs
   (electronic, acoustic, rubato), a speech-only file (no rhythm), a 3 s file.
5. Drag and delete a beat marker; Snap ai beat on/off while moving a clip and
   cutting with Blade; undo the analysis in one step.
6. Select 6 photos + 2 videos → Tagli sui beat every 1/2/4 → Anteprima,
   Annulla, Anteprima, Applica, Ctrl+Z.
7. Safe zones: toggle, switch platform, restart the app (state kept), put a
   subtitle at the bottom (warning), export and check the file has no overlay.
   Copy the JSON next to `settings.json`, change a value, toggle off/on.
8. Playback performance with the auto-zoom tab and the beat window open.

## Short-form round 2 — vertical layout, text editing, style packs

`cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test
--workspace` passes (vv-app 978, vv-core 430, the other crates unchanged).
New tests: 5 layout, 9 + 7 transcript (one cuts a word on a real app state
and undoes it), 7 + 6 style packs.

Looked at in the real app (isolated `XDG_CONFIG_HOME`, 1270×690 window,
driven with `ydotool`): vertical layout with both drawers, the layout
proposal strip, the Trascrizione and Stili tabs, the pack card and its parts,
switching back to desktop (arrangement and density restored), crash
recovery. Two fixes came out of it: the player's centred Play button covered
the right-hand controls in a narrow player (it now moves left of them,
`transport.rs`), and the pack cards wrapped their part list letter by
letter. Not tried by hand: anything needing media (transcription, cuts,
applying a pack, touch screen) — see the checklist. No LLM, no network, **no new dependency** (`rfd`,
`serde_json`, `egui_tiles` were already there).

### 1. Vertical layout and small screens

- **Where**: Layout menu → «Layout desktop» / «Layout verticale» (radio), and
  «Proponi il layout adatto alla finestra». The choice is saved
  (`settings.json` → `shorts.layout`, `shorts.propose_layout`).
- **How** (`vertical_layout.rs`): only the block rectangles change. The
  player gets the top at full height (a 9:16 timeline fills it), the timeline
  sits below (34 % of the height, 150–340 px), a 44 px bar holds the
  «Libreria» and «Proprietà» drawers and «Layout desktop». The drawers sit
  beside the player and push it (360 px each; opening one closes the other
  when both would leave the player under 200 px). On a phone-width window a
  drawer takes the whole top as a sheet. The desktop tile tree is never
  touched, so switching back is instant and restores the arrangement;
  detached windows are hidden while vertical and come back with desktop.
- **Responsive** (simplest choice, noted): the app *proposes* instead of
  switching by itself. Under 900 px wide, or taller than wide, a strip
  offers the vertical layout («Passa al verticale» / «Non ora» / «Non
  chiedere più»); from 1200 px and wider than tall it offers desktop back.
  «Non ora» holds until the window shape changes again.
- **Touch**: the vertical layout uses a touch density (`theme::set_touch_density`:
  36 px controls, larger padding, sliders, checkboxes, splitter grab).
  Two-finger pan scrolls the timeline (`timeline_ui.rs`); pinch zoom was
  already there. Mouse and keyboard are unchanged.
- **Small screens, both layouts**: the timeline bar commands scroll
  sideways instead of being clipped (the zoom and toggles stay on the
  right); the export modal shrinks to the window and stacks the cover above
  the form below ~650 px; the project title of the top bar hides under
  760 px so it cannot cover the menus.
- `block_shown` / `reveal_block` replace `workspace.is_shown/reopen` for the
  Library (downloads, editors, «Mostra nel pool», «Zoom automatici» opens the
  Properties drawer).

### 2. Text-based editing

- **Where**: Library → **Trascrizione** tab (`transcript.rs` model and
  actions, `transcript_ui.rs` panel). It follows the Main Track media clips
  with audio (falls back to the first audio track with media when the Main
  Track has none), in timeline order, one paragraph per clip.
- **Same engine as Automatic cut**: the words come from the Automatic cut
  source cache (`auto_cut::start_source`, Whisper, SHA-256 + disk cache), so
  a source transcribed once serves Automatic cut, Auto zoom and this panel.
  Opening the tab looks the cache up in the background; «Genera trascrizione
  (N)» transcribes the missing sources one after the other with progress and
  Cancel. Pauses, false starts and fillers come from `auto_cut::analyze`
  (the modal's colours), and the panel has a shortcut to the Automatic cut
  modal on the clip under the playhead. The session cache now holds 64
  sources (was 4).
- **Player sync**: the word under the playhead is highlighted and followed
  during playback; a click seeks to the word (with the scrub sound when
  stopped). Search (phrase, case and punctuation ignored, the last word as a
  prefix) with ‹ › and Enter selects the match and seeks to it.
- **Selection**: click, Shift+click (range), Ctrl+click (add/remove), drag,
  double click (sentence), menu Seleziona → all pauses / false starts /
  fillers. Touch: tap selects and seeks, a long press opens the menu and
  extends the selection from the last tap; dragging scrolls.
- **Cut**: Del/Backspace (after a click in the panel), «Elimina (n)» or the
  menu. `vv_core::transcript_edit::word_cuts` turns each run of selected
  words into a clip-local range: each side keeps up to the margin of the
  pause next to it, never enters a kept word (also with overlapping
  Whisper times) and moves to the quietest 10 ms within ±80 ms. Selected
  pauses go as analysed. Each clip gets one `AutoCut` command (ripple of the
  other tracks optional, linked audio, captions, markers, the audio
  crossfade at every join), the last clip first; all of them are one undo
  step («Taglio dal testo»), all or nothing.
- **Corrections**: F2, Enter or right click → «Correggi la parola». Stored
  in `Project::transcript_edits` (source path, stream, word start ms,
  heard text → text; serde default, skipped when empty, **no format bump**:
  an older app drops the corrections but keeps the corrected subtitles).
  `CorrectWord` also replaces the word in the saved caption words and in
  the subtitle clip showing it, without touching any timing. Corrected words
  are cyan; words under 45 % confidence get a dashed yellow underline and a
  tooltip.
- **Keys**: while the panel owns the keyboard (last click inside it, tab
  shown), Del/Backspace and Ctrl+A act on the text instead of the media pool
  or the timeline; Esc clears the selection.
- **Settings** (Opzioni, saved in `shorts.transcript`): pause kept (margin)
  0–400 ms, default 120 ms; audio crossfade 0–20 ms, default 15 ms; «Sposta
  anche le altre tracce» (default on); show pauses.

### 3. Style packs

- **Format** (`vv-core/src/style_pack.rs`): JSON with `format:
  "venturi-style-pack"` and `version: 1`. Every component is optional:
  `subtitles` (template id or `text:<id>`), `auto_zoom` (the auto zoom
  `Config`), `animations` (clip In/Out asset ids `animations/<name>` with
  seconds, text In/Out preset ids), `music` (optional file, level dB,
  ducking dB), `transition` (`transitions/<name>`, seconds), `intro` /
  `outro` (text template id, optional text; the template sets the length),
  `safe_zone` (platform id). Unknown fields are ignored; a newer version is
  refused with its number; another JSON file is «not a pack»; a broken one
  is reported with the parser's reason. `migrate` is where a version 2
  will rewrite version 1 files.
- **Built-in packs** (`style_packs.rs::builtin_packs`, one commented block
  each, copy one to add more): **Pulito**, **Dinamico**, **Tech**, **Vlog**.
  A test checks that every part they name exists.
- **Library** (`style-packs/*.json` next to `settings.json`): Libreria →
  **Stili** tab. Cards with a still of the subtitle look, name, description
  and the list of parts. Save the current edit as a pack, duplicate, rename,
  edit (form with an «include» checkbox per part), export to a file, import
  a file (it gets a new id), delete (with confirmation). Built-in packs can
  only be duplicated and exported. Unreadable files are listed by name with
  the reason.
- **Applying**: tick the parts (parts that cannot be applied here are
  disabled with the reason), «Anteprima» applies them as one real history
  step; play to check; ticking a part again re-applies; «Conferma» keeps it,
  «Annulla» undoes it. Any other edit makes it final. Ctrl+Z undoes a
  confirmed pack in one step («Applica pacchetto stile»).
- **What never gets overwritten**: clips that already have an In/Out
  animation, pairs that already have a transition, music clips whose volume
  you animated by hand, and auto zoom spans holding hand-made keyframes.
- **Per part**: subtitles are rebuilt from the saved words with the new
  template (no new transcription; without subtitles the template becomes
  the default of «Genera sottotitoli»). Auto zoom runs on Main Track videos
  that already have a transcript (the others are counted in the report).
  Transitions go on every free cut of the Main Track. Animations go on
  every video, photo and text clip (not subtitles, effects or locked
  tracks) without one. Music: with a file that exists, it is imported and
  placed on a new audio track for the length of the edit; without a file
  (built-in packs), the level and ducking go to the music already there
  (audio clips not linked to a picture and not a transcribed voice).
  **Ducking** is new: plain gain keyframes on the music clip, down by the
  ducking amount over the transcript's words (0.15 s lead, 0.35 s release,
  close phrases merged); a Main Track clip with no transcript counts as
  speech for its whole length. Intro/outro: the text template on a new
  track at the start / ending with the video. Safe zones: switched on with
  the platform; this is a player setting, so «Annulla» restores it but
  Ctrl+Z does not.

### Known limits

- Vertical: the player, timeline and properties contents are the same
  widgets, only placed and spaced for touch; the track header stays 120 px
  and the clip trim handles keep their size. The top bar menus can still
  overflow under ~560 px.
- Text editing: subtitles generated by a different transcription than the
  Automatic cut cache may not contain the corrected word (then only the
  transcript changes); a correction with several words keeps one caption
  word; the panel re-measures every word after each edit (a few ms for
  thousands of words); dragging a selection does not auto-scroll.
- Style packs: built-in names follow the language at the first load of the
  tab; a pack's music file is an absolute path (on another machine it falls
  back to the existing music, with a note); auto zoom needs transcripts made
  beforehand; restyling the subtitles drops per-clip edits of their look (as
  switching template always did).

### Manual test checklist

1. Layout menu → Verticale, open/close both drawers at 1080×1920, 1366×768
   and 420×800 (sheet); back to desktop (arrangement and detached windows
   come back); restart (layout kept); narrow the window under 900 px
   (proposal), «Non ora», widen and narrow again, «Non chiedere più».
2. On a touchscreen: pinch and two-finger pan on the timeline, tap/long
   press in the transcript, sliders and buttons in the drawers.
3. Export modal and Automatic cut modal at 1366×768 and in a narrow window.
4. Trascrizione on a talking-head project: Genera trascrizione, cancel
   halfway, retry; play (highlight and follow); click a word; search a
   phrase; delete a word, a sentence (double click) and a pause; listen to
   each join (no click, no swallowed syllable); Ctrl+Z / Ctrl+Y; margin 0 and
   300 ms; ripple off with music.
5. Correct a misheard word with subtitles on: the subtitle changes, its
   timing does not; reopen the project (the correction stays); undo.
6. Del in the transcript after clicking the media pool, and vice versa: each
   deletes only its own selection.
7. Stili: apply each built-in pack to a 9:16 talking-head edit with music:
   Anteprima, untick parts, Annulla, Anteprima, Conferma, play, export,
   Ctrl+Z (one step). Check that hand-made animations, transitions and a
   hand-made volume fade survive.
8. Save the current style, edit it (music file, intro text), export it,
   delete it, import it back; import a JSON that is not a pack, a broken one
   and one with `"version": 99`.
9. Playback performance with the Trascrizione tab open on a 30-minute
   recording.

## Short-form round 3 — automatic reframe and dubbing

The implementation plan and dependency trade-offs are in
[plans/round3-reframe-dubbing.md](plans/round3-reframe-dubbing.md).

### Automatic reframe

- **Entry point** — select a video clip and click «Reframe automatico» in the timeline toolbar to open Properties → Video → Reframe.
- **Analysis** — `vv-reframe` decodes the source at up to 640 px on its longest side with an available hardware decoder. It samples faces every N frames and checks every frame for scene cuts. YuNet runs locally on tract. The cache is in `library/reframe/` and is keyed by source fingerprint, source range and detection interval. Remove that folder to rebuild it.
- **Tracking** — `vv-core/src/reframe.rs` follows the largest face first and resets the EMA at scene cuts. Mouth motion can pick an active speaker. If energy from the speech cache is available, it gates that decision. Frames without faces use the last point of interest in the shot or the center, with a warning. In the paused player, click a visible face to pin it through the end of the shot. Face buttons also work for detections outside the current crop.
- **Editing** — «Anteprima» generates regular position and scale keyframes. «Applica» keeps them in one undo step. «Rimuovi reframe» removes generated keys that still match their original values. Auto zoom and reframe track their ownership separately. The panel warns before replacing a pre-existing position or scale curve. Undo restores the previous curve.
- **Settings and model** — `shorts.reframe` remembers tracking speed, dead zone, face margin and placement, minimum zoom, speaker following and the detection interval. [YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet) is about 232 KB, under MIT, embedded with its license in `crates/vv-reframe/assets/`. There is no separately installed face model. Reframe currently requires a clip without crop, rotation, an offset anchor or a freeze.

### Dubbing

- **Entry point** — choose «Doppiaggio» in the timeline toolbar, a target language, a preset voice and Manual or Gemini translation. The modal reuses the transcript panel’s source selection, Whisper worker and word cache. It displays original and editable translated phrases side by side before audio generation.
- **Translation and consent** — Manual uses the reviewed text directly. Gemini uses the saved API key, saved Gemini model and existing API access settings. Enable the separate dubbing checkbox under Settings → AI providers to send phrase text and the glossary to Gemini. Audio stays local. Quota, network and empty responses have distinct errors. The default glossary includes Apple, iPad, iPhone, Mac and Sidecar.
- **Synthesis and fit** — `crates/vv-app/src/dub/{ui,translate,tts,mod}.rs` handles review, workers and caches. `vv-core/src/dub.rs` handles phrases, fit and language switching. A phrase that exceeds the maximum speed increase (default 20%) is shown in red. «Accorcia» invokes Gemini when enabled or focuses the row for editing. «Riprova» acts on the failed row. Audio is reused by model and reviewed text. Translation cache files live under `library/dub/`. Generated media used by the project is written under `Dub/` beside its project file, or under `library/dub/tracks/` for an untitled project. Keep the WAV media referenced by projects that use it.
- **Mix and export** — «Anteprima» prepares one audio file in a worker and places separate editable, pitch-corrected clips in a new track. «Applica» commits audio, configurable ducking or muting, optional loudness normalization and translated subtitles as one undo step. Locked clips and clips with animated gains retain those gains. There is one audio track and an optional subtitle track per target language. Switch languages in the same modal or choose Original, As in timeline or a dub in the export modal. The export snapshot uses the selected audio and subtitles, including SRT/TXT sidecars.
- **Settings and limits** — `shorts.dub` stores the target language, voice, translation provider, glossary, voice and music ducking, stretch limit, subtitles, normalization and Gemini consent. Subtitles estimate word timing within each synthesized phrase. Translation cache holds Gemini results. Manual drafts are session-local. When a source clip mixes voice and music, lowering or muting it affects the entire clip. **Voice/music separation** remains a future extension. For voice cloning, implement the `Voice` trait in `dub/tts.rs` and register a provider in `voice_for`. An external provider would require explicit consent describing the data it receives.

### Downloaded models

- **Shared storage** — Settings → AI providers → Models folder. The default is `models/` next to the Tandem library. Changing the path leaves existing files in their prior directory.
- **Whisper** — existing [whisper.cpp models](https://huggingface.co/ggerganov/whisper.cpp), MIT, about 190 MB for Small or 874 MB for Turbo. The dubbing modal asks before downloading a missing model. Remove `ggml-small-q5_1.bin` or `ggml-large-v3-turbo-q8_0.bin` to reclaim space. Matting continues to use the same configurable directory.
- **Piper** — [rhasspy 2023.11.14-2](https://github.com/rhasspy/piper/releases/tag/2023.11.14-2), MIT engine, about 22–27 MB across Linux x86-64/arm64 and Windows x86-64. Extracted with system `tar` into `models/piper/`. Remove that directory to uninstall the engine. Piper is launched as a local subprocess.
- **Voices** — each ONNX model and its JSON configuration total about 63–64 MB. The catalog uses [a pinned compatible revision](https://huggingface.co/rhasspy/piper-voices/tree/b4e358ffcb232591bf73a540e70f7de528e4174a). Each binary archive, model and configuration is checked against SHA-256 at install time. Each voice links to its credits in the app. The catalog lists Paola, Joe, Davefx, Thorsten, Faber and Cadu (CC0), Kristin and John (public-domain datasets), Ald (Unlicense), and [Siwis](https://huggingface.co/rhasspy/piper-voices/blob/b4e358ffcb232591bf73a540e70f7de528e4174a/fr/fr_FR/siwis/medium/MODEL_CARD) (CC BY 4.0 dataset). Remove the selected pair from `models/piper-voices/` to free its space. A download begins only after its size and license have been shown and confirmed.

### Manual test checklist

1. Set a 9:16 timeline with a 16:9 talking-head clip. Open Reframe from the toolbar. Start analysis, cancel it, retry and confirm that the result is reused. Try a clip with two speakers, a hard cut and a shot without a face.
2. Preview, pause, pin a face in the player and check the framing after the next cut. Change the tracking speed, margin and dead zone. Apply, edit a generated key, then remove the reframe. Verify one undo per operation.
3. Open Doppiaggio with speech and music on separate tracks. Use an empty models folder to check both confirmation prompts and cancellation. Generate or reuse the transcript. Choose Manual and edit a few phrases before generating audio.
4. Make one phrase deliberately too long. Check the red row. Use Accorcia or edit the row, regenerate and confirm that the unchanged audio is reused. Listen to the preview, discard, preview again and apply. Check the track, ducking, text style and Ctrl+Z / Ctrl+Y.
5. Generate a second language. Switch between Original and each dub in the editor. Export once in each language with a subtitle sidecar and verify the selected speech, mix and subtitles. Check that exporting leaves the editor’s language as it was.
6. In Settings, toggle Gemini consent off and check that translation is disabled. For a live test, enable consent and use a non-sensitive sample. Verify the glossary, empty responses and retry behavior. The test suite does not call Gemini.

### Round 3 verification

The workspace suite passed on Linux with isolated XDG directories, 2,315
passed and 23 skipped by default. The app accounted for 984 passing tests. The opt-in Piper smoke test ran separately with the
pinned English Joe and Italian Paola voices and produced valid WAV files.
Clippy and rustfmt passed. This machine has FFmpeg 9.1, while the locked
`ffmpeg-next` 9.0 bindings expect FFmpeg 9.0 headers. The release was built
with FFmpeg 9.0.2 headers and checked against the local 9.1 runtime. The
Linux installer placed it in `~/.local/bin/tandem` and the installed binary
passed a headless startup check. The GUI checklist, Windows runtime and live
Gemini request remain manual checks.

## Audio cleanup — noise reduction and auto level

Two new mixer effects (`AudioEffectKind::NoiseReduction`, `AudioEffectKind::AutoLevel`),
non-destructive, saved in the project, with undo/redo through the existing
effect commands. Preview and export run the same code (`vv-audio` mixer
inserts), so they match. No new dependency: the FFT is the one already in
`vv-audio/src/spectrum.rs`, the K weighting the one of `loudness.rs`.

- **Reused**: the track insert chain, `AddAudioEffect` / `SetAudioEffect` /
  `MoveAudioEffect`, the mixer effect window. The existing Normalize (peak or
  integrated LUFS, one static gain) is unchanged; Auto level is the dynamic
  counterpart. The dub/style-pack ducking (`style_pack::ducking_gain`) is not
  touched.
- **Noise reduction** (`vv-audio/src/denoise.rs`): STFT spectral gate, ~21 ms
  frames (1024 at 48 kHz), quarter hop, sqrt-Hann. Noise floor = minimum of
  the smoothed power of each bin over the last ~1.5 s (8 × 0.19 s windows) ×
  3; gain = √(1 − over·noise/power), never below a floor, smoothed across 3
  bins and with a release. Strength Low / Medium / High = over 1.5 / 2 / 3,
  floor −9 / −15 / −22 dB. One gain for all channels (stable stereo image).
- **Latency**: the gate delays by one frame. The mixer reads a track with a
  latency that much ahead (`chain_latency`, `mix_into`), and after every
  seek/reset `prime_latency` feeds the chain the first `latency` frames, so
  nothing shifts against the picture and the first frames are not silent.
  Inserts after the gate see the time moved back (`apply_inserts`). For this
  reason noise reduction is **track only**: it is hidden from the master's
  "+" menu and ignored on the master (`AudioEffectKind::track_only`).
- **Auto level** (`vv-audio/src/autolevel.rs`): feed-forward AGC, zero
  latency. K-weighted short-term loudness over 0.4 / 0.6 / 1 s (Fast /
  Medium / Slow), gain every 10 ms ramped per sample, time constant 0.6 /
  1.5 / 3 s (cuts 4× faster), gate at max(−50, target − max − 8) LUFS so
  silence is not raised, peak safety at −0.3 dBFS. Defaults: −16 LUFS,
  ±12 dB, Medium. Ranges: target −30…−8 LUFS, max 3…24 dB.
- **UI**: mixer "+" menu → «Riduzione rumore» / «Volume costante», with their
  window (`audio_cleanup::{noise_reduction_settings, auto_level_settings}`).
  Properties → Audio of a clip has the two sections (the old noise
  reduction placeholder is gone): «Attiva», the settings and «Si applica a
  tutta la traccia: A1». A clip has no effect chain of its own, so they act
  on the audio track of the clip, or of its linked audio for a video clip
  (simplest choice). A new noise reduction goes first in the chain, a new
  auto level right after it. Several selected clips → all their tracks, one
  undo step.
- **Known limits**: the noise floor is learned again after a seek, so the
  first ~0.2–1.5 s after a jump can be reduced slightly differently from a
  continuous export; same for Auto level (its first window settles in
  0.15 s). Made for voice: a held note of music counts as noise. Adding or
  removing a noise reduction while playing skips/repeats 21 ms once.
- **Build note**: FFmpeg 9.1 on this machine broke the `ffmpeg-sys-next` 9.0
  bindgen (CUDA types). `Cargo.lock` now has `ffmpeg-sys-next` 9.1.1 and
  `ffmpeg-next` 9.1.0 (Cargo.toml unchanged, `9.0.0` requirement).
- **Tests**: `vv-audio/src/tests/{denoise,autolevel}.rs` (unity gate = pure
  delay, reduction per strength, voice level kept, target reached, gate,
  ceiling, block-size independence), two mixer tests (no time shift, split
  independence with a latency), `vv-app/src/tests/audio_cleanup.rs`.

### Manual test checklist

1. Voice recorded with a fan/hiss on an audio track: Properties → Audio →
   Riduzione rumore → Attiva. Play: the noise drops in the pauses within a
   second; try Bassa/Media/Alta, listen for metallic artefacts on Alta.
2. Same clip on a video track with linked audio: the section names the
   audio track; check lip sync with the gate on and off (no shift).
3. Seek around while playing: no click/silence at the jump beyond the brief
   re-learning; export and compare a passage with the preview.
4. Volume costante on a voice with quiet and loud sentences: levels even
   out; silences stay silent; change target/max/speed while playing.
5. Mixer: both effects in the track "+" menu, not in the master's; bypass,
   reorder, remove; Ctrl+Z / Ctrl+Y; save, reopen the project.
6. Several clips on two tracks selected: one toggle turns it on for both,
   one undo step.
