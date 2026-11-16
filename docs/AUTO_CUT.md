# Automatic cut

Implemented as source changes only. No compilation, build, test execution or GUI
run was performed for this change.

## Data flow

- The timeline toolbar opens `vv-app/src/auto_cut_ui.rs` for the selected media
  clip. If several clips are selected, the first video source takes precedence.
- `vv-app/src/auto_cut.rs` hashes the source with SHA-256 on a worker, decodes it
  through the existing FFmpeg streaming decoder as mono 16 kHz, and calls the
  existing `vv-transcribe` Whisper engine. A linked audio source covering the
  video clip is preferred; otherwise the selected source's audio is used.
- Transcription uses 60-second ownership windows with two seconds of surrounding
  context. Only words starting in the owned window are retained. Raw audio RAM
  is bounded by the block size. Word times, token confidence and a 10 ms RMS
  envelope are cached; the source is analyzed once even when its timeline clip
  has already been trimmed or sped up.
- The disk cache lives in the user library's `auto-cut/` directory. Keys include
  the full source digest, audio stream, model, language and cache version. A
  bounded session cache avoids hashing and decoding on repeated opens. Source
  size and modification time invalidate session entries. Disk cache write
  failure does not prevent using a successful transcription in the session.
- `vv-core/src/auto_cut.rs` is pure and independently testable. Its cancellation
  callback also allows background analysis to stop without publishing a partial
  segment list. All detection defaults live in `Config`.
- `vv-core/src/auto_cut_edit.rs` builds the timeline result before committing one
  `AutoCut` history command. Its snapshots restore geometry, links, effects,
  transitions, saved caption words and markers on undo.

## Detection and review

Default silence threshold: -35 dBFS; minimum pause: 400 ms; speech padding:
120 ms per side; energy snapping radius: 80 ms. Silence candidates combine word
pauses and energy runs. Transcript words and their padding are protected; edge
snapping only shrinks the proposed deletion, preserving the padding. Loud audio
without recognized words is kept for manual review.

False starts compare normalized neighboring sentences using token Levenshtein
similarity (80%), within three sentences and 15 seconds. Long sentences use
Jaccard similarity over word trigrams to keep comparisons bounded. A repeated unfinished
prefix also qualifies. Low token confidence is an additional signal for a
truncated prefix; confidence alone never deletes unrelated speech. A repeat
needs at least three normalized words. Earlier takes are proposed for removal;
the last take is kept.

Italian fillers are configurable words. Silence and false starts are checked by
default; speech and fillers are unchecked. The segment's Rust field `kind`
serializes as `type`, with `speech`, `silence`, `false_start` and `filler` values.

The modal uses theme tokens, a virtualized segment list and a waveform reduced
on the analysis worker to at most 4096 peak buckets. Threshold/padding changes
are debounced and analyzed in the background. Compatible manual checkbox choices
are retained. Playback uses the existing timeline player and skips checked,
merged frame ranges. It never starts voiceover recording. Clicking a row or the
waveform seeks; Space controls playback when a widget does not own the keyboard.
Esc or Cancel cancels workers and restores the original selection/playhead.

## Timeline semantics

The checked intervals are merged before computing the keep ranges and rounding
cuts inward to timeline frames. The duration summary uses those exact frame
ranges. Audio/video pieces retain their source mappings and get matching link
groups. With global ripple off, the selected and linked tracks close up; other
tracks retain their geometry. With global ripple on, unlocked tracks are cut and
shifted, including crossing text, sticker and music clips. Locked tracks stay
fixed. A locked member of the selected linked group prevents the operation.

In animations remain on the first kept piece, Out on the last. Combo assets
share a duration and cumulative offset, so they continue across pieces. Choosing
a new Combo preset resets that inherited clock. Existing crossing transitions
involving a split/removed clip are removed, matching the regular split command;
undo restores them.

At an internal audio cut the outgoing source continues for 15 ms into the next
piece while complementary sample ramps crossfade the outgoing tail and incoming
head. Timeline geometry and video timing remain frame based. Preview and export
use the same mixer implementation. Ordinary splitting clears the newly created
internal splice ramps and restores them on undo. The new persisted defaults
require no data migration; the project format is 5 to prevent older applications
from silently losing the splice and Combo timing.

No additional network service or dependency is introduced. Automatic cut uses
an already installed speech model. If none is present, the modal directs the
user to the existing Subtitles model installation flow. Freeze frames, disabled
clips and sources without audio are refused. Source changes and history changes
during review prevent applying stale results.

## Manual verification

After compiling separately:

1. Check the toolbar button, full-screen layout, row/checkbox hit areas, hover
   text, keyboard focus, Space, Esc, waveform seeking and the duration summary
   at different window sizes and display scales.
2. Tune -35 dBFS, 400 ms and 120 ms on real recordings: quiet voices, breaths,
   room tone, music beds and noisy microphones. Listen to sibilants and plosives
   near each cut. Inspect false positives as well as missed silences.
3. Review Italian repeats, false starts, deliberate rhetorical repetition,
   short restarts and fillers. Pay particular attention to ownership boundaries
   near 60-second multiples and to language auto-detection.
4. Open the same source twice, then restart the application and open again.
   Check session/disk cache reuse, another audio stream, a changed source and a
   source that is moved or relinked.
5. Try a trimmed clip, a sped-up clip, audio-only media, entirely silent media,
   uninterrupted speech, cancellation during hashing/extraction/Whisper/analysis,
   a missing model, a missing file and a transcription error.
6. Apply global ripple on/off with linked audio/video, music, text, stickers,
   locked tracks, markers, subtitles, existing transitions and animated clips.
   Check the first/last animation and Combo continuity. One undo must restore the
   whole edit; redo must restore the same piece IDs and geometry.
7. Save/reopen and export the result. Listen for clicks or overlap artifacts at
   audio joins and verify video/audio sync, including pitch-corrected speed.
8. Use a long recording to check UI responsiveness, bounded audio buffering,
   cancellation, waveform rendering and scrolling through the virtualized list.
