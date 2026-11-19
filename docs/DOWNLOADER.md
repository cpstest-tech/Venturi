# Web downloader

Paste a YouTube, Instagram, TikTok or Spotify link into the bar on top of
**Audio → Music**, **Audio → SFX** or **Media → Project** (or press Ctrl+V
with the link in the clipboard anywhere in the editor). A dialog reads the
link, then offers:

- **Save to**: Music, SFX (audio only, copied into the user library) or the
  project's media (audio or video).
- **Quality**: audio as served (Opus/AAC, no conversion), WAV, FLAC or MP3;
  video up to 480p–2160p, H.264 preferred because it edits smoothly
  (unticked, VP9 up to 4K is allowed; AV1 is never picked, FFmpeg here only
  decodes it in hardware).
- **Only a section**: two draggable edges over YouTube's "most replayed"
  curve and the chapter marks, or typed times, or a chapter from the list.

Downloads run in the background, two at a time, with progress in the bar
and in the toolbar. When one finishes it is imported where it was asked
for, with a toast and, if the window is in the background, a system
notification.

## How it works

- [yt-dlp](https://github.com/yt-dlp/yt-dlp) does the downloading. The
  standalone release for the platform is fetched on first use into
  `$XDG_DATA_HOME/tandem/tools/`, checked against the release's
  `SHA2-256SUMS`, and fetched again when older than five days, since sites
  break old versions quickly.
- yt-dlp needs ffmpeg and ffprobe to merge and convert: the AppImage, the
  macOS app and the Windows zip ship both next to the `tandem` binary; a
  development build uses the ones on `PATH`.
- YouTube needs a JavaScript runtime: Node from the AI runtime setup or on
  `PATH` is passed to yt-dlp; Deno is found by yt-dlp itself.
- A section is fetched by yt-dlp only when ffmpeg can read `https` (the
  bundled one is built without TLS). Otherwise the whole file is downloaded
  and cut locally: audio by stream copy, video re-encoded to H.264 so the
  cut lands on the exact frame.
- Spotify audio is DRM-protected. Track, album and playlist links are read
  from Spotify's public embed page (title, artists, length), each track is
  searched on YouTube and the closest recording is kept: length within 15 s,
  no live/cover/remix the title does not ask for, YouTube Music "- Topic"
  channels preferred.
- Instagram posts that need a login can use the cookies of a browser chosen
  in the dialog (`--cookies-from-browser`).

Files for the library are downloaded into the cache
(`$XDG_CACHE_HOME/venturi/downloads`), since the library keeps its own
copy; files for a project stay in `$XDG_DATA_HOME/tandem/downloads`.

Only download what you have the rights to use.
