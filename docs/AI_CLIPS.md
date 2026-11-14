# AI Clips

AI Clips is available in the editor's library ribbon. Open AI Clips and choose a source video from its thumbnail grid. Choose a duration, optionally enter instructions, and generate suggestions. Review their scores and previews, select the clips to use, and insert them at the end of the current timeline. The original media file is referenced directly; no rendered clip files are created.

The first run downloads the same local speech model used by the Subtitles tool. Speech recognition runs locally. The Gemini API key is set in **Settings → Integrations**; AI Clips links there when no key is set. The key is stored in the user's configuration directory as `venturi/gemini.key` (`~/.config/venturi/gemini.key` on Linux) with owner-only file permissions on Unix. `GEMINI_API_KEY` in the process environment is also accepted when no key is saved. The key is not saved in a project, in the settings file or in the AI Clips cache.

The transcript is sent to Gemini as timed sentences; the returned times are snapped back to word boundaries locally. Overload and rate-limit responses (429, 5xx) are retried with backoff, then once more on `gemini-flash-latest`; the error shown is Gemini's own message.

Transcriptions, prior selections, and candidate frames are saved in the application library's `ai-clips` directory under a SHA-256-derived fingerprint of the source bytes. Changing the prompt or duration reuses the transcript. Repeating the same prompt and duration uses the saved Gemini result. When the source content changes, the old cache is ignored.

Generated media clips and their audio are linked. Optional subtitle text clips are editable on the subtitle track and linked to the matching media clip. The selected subtitle template is used when it is a built-in template; otherwise the default template is used. Insertion and replacement are each a single Undo step. Replacement applies only to the latest AI Clips batch inserted during the current editor session and requires explicit confirmation.

The visual pass checks a midpoint frame from each of the eight strongest transcript candidates. Frames and text are sent to Gemini; only candidate frames are uploaded. A failed visual pass keeps the transcript-based clips. A network or API error leaves the project timeline untouched. The cached transcript and any completed transcript analysis remain available for a retry.
