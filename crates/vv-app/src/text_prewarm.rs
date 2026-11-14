use vv_core::{ClipSource, FrameIdx, Timeline, TitleParams};
use vv_render::OutputFrame;

pub fn prepare(timeline: &Timeline, playhead: FrameIdx, output: OutputFrame) {
    for title in upcoming(timeline, playhead) {
        vv_render::text::prewarm(&title, timeline.resolution, (output.width, output.height));
    }
}

fn upcoming(timeline: &Timeline, playhead: FrameIdx) -> Vec<TitleParams> {
    let fps = timeline.fps.as_f64().max(1.0) as f32;
    let end = playhead.saturating_add((fps * 3.0).ceil() as FrameIdx);
    let mut clips: Vec<_> = timeline
        .visible_video_clips()
        .filter(|(_, clip)| {
            matches!(clip.source, ClipSource::Text)
                && clip.timeline_end() > playhead
                && clip.timeline_start <= end
        })
        .map(|(_, clip)| clip)
        .collect();
    clips.sort_by_key(|clip| clip.timeline_start.max(playhead));
    // Leave cache space for the current frame and library thumbnails.
    clips
        .into_iter()
        .take(8)
        .filter_map(|clip| {
            let frame = playhead.max(clip.timeline_start);
            let title = clip.effects.title.as_ref()?.at(clip.source_frame_at(frame));
            Some(title.animated(
                (frame - clip.timeline_start) as f32 / fps,
                clip.timeline_len as f32 / fps,
            ))
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/text_prewarm.rs"]
mod tests;
