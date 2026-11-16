# AI Sequence Generator

The **AI Infographic** tab of the library, next to AI Clips, describes and
generates animations. It is based on the clips selected on the timeline (or the
In/Out range); with no selection the animation is free. The length is *Like the
clip*, *AI decides* (default; the model returns `duration_seconds`, at most 30 s)
or *Custom*. While a job runs, a striped "AI generating…" block reserves its spot
on the timeline. Providers, models, billing and the runtime live in Settings → AI
Providers. The feature has a persistent sequential background queue, Gemini and CLI adapters, source
revisions, a bounded compile/render repair loop, and local rendering through
bubblewrap on Linux. Completed output is imported through the existing background
media importer. Optional timeline insertion resolves the original clip anchor
against the current timeline and creates an overlay track.

The local rendering implementation currently targets Linux with bubblewrap.
Codex, Claude Code and OpenCode have passed live image/context, session revision
and transparent-render tests. Gemini authentication, model identity and
`generateContent` support were verified against the configured API key, but live
Hydrogen generation returned HTTP 503 (high demand) on both final attempts.
No alternate model was selected automatically. Its successful live generation
path therefore remains unverified while the upstream service is unavailable.

## Local runtime setup

Run these commands in `tools/remotion-runtime`:

```sh
npm ci --ignore-scripts --no-audit --no-fund
npm run setup-browser
```

Copy the printed runtime, Node and Chromium paths into the AI panel's local
settings (Settings → AI Providers). Install bubblewrap using the OS package manager. The application checks
that namespace isolation works before making a provider request. Rendering fails
closed on other operating systems or when bubblewrap cannot create namespaces.
No generated npm scripts are run and no generated dependency installation is
performed. The runtime packages are pinned separately from generated source.

Choose a provider and its model. Each provider keeps its own model selection.
An empty model field uses the CLI default; Codex ignores user automation config
for tool safety, so pass its model explicitly when needed. OpenCode retains its
configured providers and expects its own provider/model syntax. Authenticate each CLI
using its official login command before opening the editor. Gemini shares Viral
Clips' API key. Free-only mode requires confirming that billing is disabled for
the key's Google API project; only models with a verified Free Tier listing are
accepted. The API key cannot report billing status, so this is a user confirmation,
not an automatic billing check. Changing the key invalidates that confirmation.
Otherwise, explicitly enable the API project's configured billing policy.
Authorization is kept in memory and is never serialized into project files.

Choose timeline context to use selected clips or the existing In/Out marks.
Project captions are cropped to that range. If they are absent, background
extraction can reuse the existing Viral Clips transcript cache without invoking
speech recognition. Prompt-only mode sends no project captions or media.
Optional context includes up to three representative raw source frames and up
to three raw source audio excerpts (10 seconds each, at most 30 seconds total).
Frames use the same decoder/downscale helper as Viral Clips. Audio uses the
existing seekable decoder, downsampled to 16 kHz mono WAV. These are source
excerpts, not a baked rendering/mix of the editor's effects. Gemini receives
inline image/audio parts; CLI adapters receive images using their documented
attachment protocols. CLI audio input is not enabled. Missing optional media
produces context warnings; a timeline request without usable context fails
before consuming provider tokens. Sources,
render output, composition settings, prompts and parent revision references live
in `<project-file>.ai-sequences/<job-id>` beside the project file. Each project
file owns a distinct archive, including loose project files in the same directory.
Project import, duplicate, rename and Save As preserve sources and rebase rendered
media references. Save As copies sources in the background and keeps edits made
during the copy. Existing generation jobs remain with the original project; a
project switch during the copy cancels applying Save As to the new document. Interrupted jobs become failed jobs on recovery and can
be retried as new jobs. Closing the panel leaves queued work running.

The render sandbox has no host home directory or network access. It mounts the
runtime and system libraries read-only, and only the job directory writable.
Node heap, subprocess output and runtime are bounded. These are not hard total
memory/CPU quotas for the browser; OS-level cgroup limits remain future work.
CLI generation uses provider restrictions (Codex read-only with shell/unified
execution disabled and user automation config ignored; Claude no tools/MCP or
slash commands; OpenCode denied permissions); it is not the render OS sandbox. CLI configuration
and authentication remain controlled by the user and installed provider version.

## Existing architecture

The desktop UI is Rust/eframe/egui (`vv-app`), with timeline and project models,
commands and persistence in `vv-core`. Playback/compositing uses `vv-render` and
media decoding uses `vv-media`. Remotion and React are optional pinned Node dependencies in
`tools/remotion-runtime`; the Rust playback renderer is unchanged.

`ai_clips.rs` owns the Long to Short workflow. It caches sentence transcripts,
word timestamps and sampled visual analysis, runs blocking work on background
threads, and exposes cancellation and stage information. Its transcript request
uses JSON response schemas. Its visual analysis sends representative images.
Viral Clips retains its configured models `gemini-3.5-flash` and
`gemini-flash-latest`; sequence jobs use the explicitly selected model.

`gemini.rs` now owns the HTTP transport and key persistence. Settings and Viral
Clips use the same existing key path, `$XDG_CONFIG_HOME/tandem/gemini.key`
(or `~/.config/tandem/gemini.key`). Writes are atomic and use mode 0600 on Unix.
Viral Clips also supports `GEMINI_API_KEY`. Keys must not enter project metadata.
No new SDK or dependency was introduced.

The shared transport accepts an explicit ordered model list and a quota retry
policy. Viral Clips retains its previous fallback/backoff behavior. Future
sequence requests should pass exactly the selected model and disable quota
retries: an exhausted quota must not silently cause a model switch. Cancellation
is checked between requests and during backoff; an in-flight synchronous HTTP
request can still take up to the existing 300-second timeout to return.

`proxy_worker.rs` provides an existing sequential worker pattern, but its state
is not a persistent generic job queue. Project captions are already persisted
in `Project::captions`; use them before requesting speech recognition again.
Timeline changes must go through existing commands/history, rather than mutate
clips from a worker thread.

## Revisions and visual suggestions

Selecting a generated clip adds an AI section to Properties. It can request a
background revision, preview completed media in the existing viewer, and apply
or restore a version. Revisions inherit the original composition configuration
and source. Applying changes only the selected clip's media source through an
undoable command; it preserves position, trim, speed, effects and fades. The
application rejects locked tracks, missing clips, stale expected sources,
frame-rate changes and media too short for the current trim.

AI Visual Suggestions is a separate section in the generation panel. Its
Analyze button explicitly starts a cancellable background request over the
selected captions. The analysis provider/model can differ from the generation
provider/model. Returned ranges are checked against the analyzed duration.
Suggestions begin unselected, can be edited or discarded, and can navigate the
timeline. Only Generate selected suggestions enqueues generation requests.
Each request receives captions cropped and rebased to the suggestion's range,
with the original stable clip anchor and relative insertion offset.

## Remaining integration work

- Complete the successful Gemini live-account generation test when Hydrogen
  stops returning HTTP 503; authentication and model metadata requests succeed.
- Add platform sandbox implementations outside Linux and hard browser resource
  limits; until then unsupported rendering fails closed.
- Additional interactive desktop coverage and OS sandbox implementations can
  extend the existing automated tests. Primary UI controls and job states have
  English/Italian translations; provider/compiler diagnostics retain their text.

## Model availability and cost

Google documents `gemini-4-hydrogen`, but model availability alone does not
establish that a particular request is free. Consult the current pricing and
the account's active quota before enabling a free-only workflow. Do not label
arbitrary configured models as free or assume API model listing reports billing
eligibility. A paid API project can incur charges on the selected model without
any model fallback; free-only mode needs enforceable billing eligibility, or
must refuse the request when it cannot establish that eligibility.

Sources checked on 2026-11-16:

- https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/4-hydrogen
- https://aistudio.google.com/docs/pricing
- https://aistudio.google.com/docs/rate-limits
- https://www.remotion.dev/docs/licensing

Remotion licensing also needs review before distribution/integration into the
GPL editor; do not assume every deployment is covered by a free license.

## Verification during development

The final verification passed `cargo check`, the production debug build,
887 app tests and 391 core tests (zero failures; 10 app tests ignored). A separate ignored integration test renders a real short composition with
Remotion inside bubblewrap. Run it after installing the runtime and browser:

```sh
cargo test -p vv-app remotion_render_smoke --offline -- --ignored
```

The integration test needs actual namespace permissions; the development
sandbox may block it even if a normal desktop launch supports bubblewrap.
An interrupted browser extraction can leave a truncated executable: setup checks
`--version` before reporting success. Repair the local browser installation before
retrying in that case.

## AI provider settings

Settings → AI Providers detects installed CLI versions on a background thread.
The Gemini key editor is shared with Integrations/Viral Clips. Model listing is
an explicit action using Gemini `models.list`, filtered for `generateContent`.
A listed model is not automatically labeled free: listing does not establish the
billing tier or guarantee quota. Custom model identifiers remain configurable.
The editor never switches to a different model after a Gemini quota failure.

CLI version checks were exercised with Codex 0.166.0, Claude Code 2.1.323 and
OpenCode 2.2.5 during development. Live integration tests passed for all three:
a synthetic PNG was attached, a React composition was generated, its session was
forked for a revision, and the revised source produced transparent local Remotion
output. These tests are ignored by default because they use authenticated
accounts. A shared Gemini key was subsequently configured. Its model metadata
request succeeded (HTTP 200, exact Hydrogen ID and `generateContent` support),
but generation/revision testing was stopped by Google's HTTP 503 high-demand
responses. The application kept the selected model and reported the failure.

The final complete suite ran outside the restrictive development sandbox:
887 app tests and 391 core tests passed with zero failures. Ten app tests are
ignored by default, including live-provider tests. The production binary was
built with `cargo build -p vv-app --offline` at `target/debug/tandem`.
A sandboxed suite cannot access the user's audio devices and cache directories;
its failures must not be treated as application regressions without comparison.

AI storage refuses symbolic-link roots/manifests and uses an exclusive queue lock
so two editor instances cannot recover or overwrite the same active jobs. Corrupt
manifests produce visible recovery warnings while other valid jobs remain usable.
Renaming/deleting a project with active AI jobs is deferred until they finish or
are cancelled. Automatic timeline insertion waits until the user's undo group is
closed, validates the original source reference and follows moved clips; replaced,
trimmed-out or retimed references leave the completed media available for placement.

## Session reuse and analysis caching

CLI responses preserve provider session identifiers, structured event names and
reported usage. Matching-provider/model revisions fork the previous session,
which keeps concurrent revisions from sharing one mutable conversation. A fresh
session checkbox is available when previous CLI state is absent or incompatible.
Gemini revisions reuse the saved source and cropped context through the shared
HTTP service. Repair calls and ordinary revisions omit unchanged media attachments.

Visual analyses are persisted under `.analyses/<input-digest>` in the project's
AI archive, including suggestions, cropped context and reported usage. Pressing
Analyze with identical inputs reuses that report without invoking a provider;
Ignore saved analysis explicitly requests a new call. The digest includes the
request and source file size/modification metadata. Suggestions are always loaded
unselected. Accepting them crops and rebases captions and media to each accepted
range before enqueuing generation.

## Final integration details

AI provenance is stored by media ID in the native project format, so relinking a
rendered file does not remove its source association. Import/removal metadata is
undoable, and automatic import completion preserves the viewer and selection.
Results completed while another project is open are deferred to their owning
project. Completed, not-yet-imported jobs are recovered when the archive opens.
Failed imports show an error and leave the rendered asset available for retry.

Natural-language transparency revisions update a structured render flag as well
as the component's background. Transparent output uses ProRes 4444 with alpha;
the integration test verifies that the native editor decoder retains transparent
pixels, not merely that a `.mov` file exists.

AI Providers settings expose bounded code-generation attempts and provider/render
timeouts. Only compilation/composition errors enter the code repair loop;
environment failures, exhausted quotas and cancellations do not cause new code
requests. Reported usage is retained per successful code request. Partial rendered
files are removed after failures, and cancellation intent survives restart.

Gemini model pricing information was checked on 2026-11-16 against
https://aistudio.google.com/docs/pricing: Hydrogen has a listed standard Free Tier;
Carbon and Argon do not. Unknown models are not labeled free. No request uses
search grounding or automatically enables paid tools/models.
