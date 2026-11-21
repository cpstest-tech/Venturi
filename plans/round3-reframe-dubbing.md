# Round 3 — automatic reframe and dubbing (plan)

Written before the code. What actually shipped is in `handoff.md`
("Short-form round 3").

## What already exists and gets reused

| Need | Existing piece |
|---|---|
| ONNX inference, CPU, no native deps | `tract-onnx` 0.23 (`vv-matting`, `vv-vad`) |
| Frame decoding at reduced size, HW decode | `vv_media::Decoder::open_with` + `set_max_size`, `hw_decode::devices()` |
| YUV → RGB of a decoded frame | `RgbReader` in `vv-matting` → moved to `vv_media::FrameYuv420::rgb_at` and shared |
| Model download with progress/cancel | `vv_matting::ensure_model` pattern; managed binaries: `downloader/ytdlp.rs` |
| Transcript with word times + 10 ms energy | Automatic cut source cache (`auto_cut::start_source`, `clip_local`) |
| Generated keyframes, preview → Applica/Annulla, remove | `auto_zoom::{strip, SetAutoZoom}`, `Clip::auto_keyframes`, `auto_zoom.rs` UI pattern |
| Gemini key, opt-in and free-tier rules | `gemini.rs` (`configured_key`, `generate`, redaction), `ApiAccess` |
| Pitch-preserving time-stretch | clip speed + `Clip::pitch_correction` (mixer, `vv-audio/src/mixer.rs`) |
| Ducking keyframes | `vv_core::style_pack::{speech_regions, ducking_gain}` |
| Loudness normalization | `AudioEffectKind::Normalize { mode: Loudness }` on the track strip |
| Subtitle clips in the current style | `captions::build_clips` / `SubtitleStyle::build` + `carry_look` |
| Placing an audio file as a clip | `place_take` (voiceover), `insert_music` (style packs) |
| Mute/solo in mix and export | `Track::muted`, `Timeline::audible_tracks` |

## Feature 1 — automatic reframe

**Model: YuNet** (`face_detection_yunet_2023mar.onnx`, OpenCV Zoo).
- 232 KB, MIT, ONNX: it runs on the `tract` already in the build, so no new
  dependency and identical on Linux and Windows. Gives a box, a score and 5
  landmarks (eyes, nose, mouth corners): the mouth corners give the active
  speaker without a second model.
- MediaPipe BlazeFace needs TFLite or a conversion (and does not give
  mouth landmarks in the short-range model); an OpenCV/dlib detector would
  bring a C++ toolchain.
- Small enough to embed (`include_bytes!`) instead of downloading: no
  network, no confirmation, nothing to remove. The license is included in
  the crate next to the model.
- Fixed 640×640 input: frames are letterboxed into it.

**New crate `vv-reframe`** (like `vv-matting`): decode at ≤ 640 px on the
long side (HW decoder when available, priority Low), every frame for scene
cuts (64-bin luma histogram distance + an adaptive threshold over a sliding
median), YuNet every N frames (setting, default 5) plus the first frame
after every cut. Per detection: box, score, mouth motion (mean absolute
luma difference of the mouth patch from the previous decoded frame).
Parallel stretches as in matting. Results cached on disk per (content
hash, N), JSON in the library's `reframe/` folder.

**Pure tracking in `vv-core/src/reframe.rs`** (unit tested):
- Per scene (between cuts) the tracker restarts: no gliding across a cut;
  the first key after a cut jumps (Hold interpolation).
- Subject: largest face at the start of the scene; with several faces and
  speech in the audio (energy from the speech cache, when it exists), the
  face with the most mouth motion over ~1 s windows wins, with hysteresis
  (it must lead for 1.5 s). The user can pin a face per scene (click in the
  player or the face buttons in the panel).
- Smoothing: EMA (factor from «Velocità di inseguimento») and a dead zone
  (the target only moves when the face leaves a box of ±X % around the
  current framing), then the path is simplified (Ramer–Douglas–Peucker,
  2 px) so there are few editable keys.
- No face: keep the last point of interest in the scene, or the centre if
  there was none; the panel lists the time ranges without faces.
- Framing: zoom = max(fill zoom × «Zoom minimo», zoom to fit the face with
  its margin), constant within a scene (no breathing), position clamped so
  the frame never shows the letterbox; face at the chosen height
  (centre / upper third).
- Output: ZoomX/ZoomY/PositionX/PositionY keyframes in `effects.transform`,
  tagged in `Clip::auto_keyframes` with `origin: Reframe` (new serde-default
  field, no format bump). Auto zoom and reframe each strip only their own.
  One history step (`SetAutoZoom` reused with the `Reframe` label);
  «Rimuovi reframe» removes the untouched generated keys in one step.

**UI**: «Reframe automatico» in the timeline bar (selected video clip) →
Properties → Video → **Reframe** sub-tab: settings, Analizza (progress +
Annulla), Anteprima → Applica / Annulla (same pattern as Zoom auto), face
boxes drawn over the player while the tab is open (click = follow this face
in this scene), warnings for ranges without faces.

**Risks**: tract running YuNet (checked by a test that loads the model and
runs one frame); decoding long 4K files (reduced size + HW, cancellable);
the zoom/position semantics with rotated or cropped clips (reframe refuses
rotated clips, documented).

## Feature 2 — dubbing

Pipeline: speech cache (Whisper, existing) → segments → translation →
review table → TTS → fit → timeline.

**Segments** (`vv-core/src/dub.rs`, pure): phrases from the words (pause
≥ 0.5 s, sentence end, ≤ 12 s), each with its time window.

**Translation** — trait `Translator` with:
- `Gemini` (existing key and `gemini::generate`; batched JSON schema, one
  request per ~40 segments, instructions to keep length close and to keep
  product names and technical terms; a glossary field, default
  `Apple, iPad, Sidecar, iPhone, Mac`);
- `Manual` (copies the original text; for offline use or a human
  translator).
Sending text to Gemini needs a new explicit consent in Settings
(«Doppiaggio: invia il testo a Gemini per la traduzione»), on top of the
existing API access rules. The key is never logged (errors already go
through `[redacted]`).

**TTS** — trait `Voice` with:
- `Piper` (local, offline): MIT binary from the rhasspy/piper 2023.11.14-2
  release (~22–26 MB) run as a subprocess, and ONNX voices from
  `rhasspy/piper-voices` (~63 MB each). Downloaded only on first use after
  a confirmation dialog that shows sizes and licenses, into the models
  folder (configurable in Settings). Only voices with permissive dataset
  licenses are listed (CC0, CC-BY, public domain).
- Extension point for a cloned voice (XTTS locally or an external service):
  documented in `dub/tts.rs` and the handoff, not implemented.
- Piper runs as a subprocess because its phonemizer (espeak-ng) is a C
  library: linking it would mean a native build on both platforms.

**Fit**: each segment's audio length vs its window → speed factor within
±20 % (setting) applied as clip speed with pitch correction (the existing
mixer path, so the result stays a normal editable clip). Over the limit:
the row turns red, «Accorcia» asks the translator for a shorter version
(Gemini) or lets the user edit it.

**Timeline**: one new audio track per language (`Track::dub = language`),
one clip per segment, Normalize (loudness) on the track strip; ducking or
mute of the original voice and the music as gain keyframes
(`ducking_gain`) — one undo step for the whole dub. Subtitles: words timed
across the segment, built with the current subtitle template and look, on
a new video track tagged with the same language.

**Several languages / export**: export gets «Lingua»: timeline as is /
original / each dub. Choosing one mutes the other dub tracks (and their
subtitle tracks) on the export snapshot and restores the original gains
saved by the dub; the same switch exists in the dub panel as an undoable
step.

**Cache**: translations keyed by SHA-256 of (provider, model, language,
glossary, text); audio by (voice, text, speed). Files under the library's
`dub/` folder; a changed row only re-runs itself. TTS failure on one row
marks only that row (Riprova).

**Errors**: unreachable API / quota (`gemini::api_error`, status 429 shown
as quota), empty translation (row flagged), TTS failure per row, missing
binary/voice (download prompt).

**Voice/music separation**: not done (a Demucs-class model is hundreds of
MB and slow on CPU); noted as a future extension.

## Files

- new: `crates/vv-reframe/` (lib, tests, `assets/yunet.onnx`, `LICENSE-YUNET`)
- new: `vv-core/src/reframe.rs`, `vv-core/src/dub.rs` (+ tests)
- new: `vv-app/src/reframe.rs`, `vv-app/src/dub/{mod,translate,tts,ui}.rs` (+ tests)
- touched: `vv-core` model (`AutoKeyframe::origin`, `Track::dub`,
  labels), `vv-matting` (shared RGB reader), `vv-media` (rgb_at),
  `settings.rs`/`settings_dialog.rs` (models folder, consent, prefs),
  `properties_tabs.rs`, `properties_panel.rs`, `timeline_bar.rs`,
  `main.rs`, `export_*`, `vv-session/src/export.rs`, locales.

## Implementation notes

The exact shipped catalog and settings are in the handoff. The Piper release
is unchanged from the plan. The voice repository is pinned to a revision
with configurations compatible with that release. Newer voices were found
to require a newer phoneme parser. The installed archives, model weights
and configurations have pinned SHA-256 digests.

The app groups the synthesized phrases in one WAV with frame-aligned
source ranges. Assembly is a cancellable worker job. Each phrase still
appears as an individual audio clip. Dubs use a speed-up bound and require
all phrases to fit before preview. The raw audio cache can therefore be
reused when the stretch setting changes.
