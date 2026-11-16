use super::Request;
use vv_core::{ClipSource, Id, Rational, Timeline};

pub fn position(timeline: &Timeline, request: &Request) -> Result<i64, String> {
    let original_fps = Rational::from_fps(request.composition.fps);
    let offset = vv_core::convert_frames(request.anchor_offset, original_fps, timeline.fps);
    let Some(anchor) = request.anchor_clip else {
        return Ok(vv_core::convert_frames(request.start, original_fps, timeline.fps).max(0));
    };
    let clip = timeline
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .find(|clip| clip.id.0 == anchor)
        .ok_or("The original timeline clip was removed")?;
    let base = if let Some(reference) = &request.anchor_details {
        if reference
            .media
            .is_some_and(|media| !matches!(clip.source,ClipSource::Media(id) if id.raw() == media))
        {
            return Err("The original timeline clip now uses a different source".into());
        }
        let expected_rate = reference.rate.as_f64() * timeline.fps.as_f64() / original_fps.as_f64();
        if (clip.rate().as_f64() - expected_rate).abs() > 1e-6 || clip.freeze.is_some() {
            return Err("The original timeline clip's speed or freeze state changed".into());
        }
        if reference.source_frame < clip.source_in() || reference.source_frame >= clip.source_out()
        {
            return Err("The original reference frame was trimmed out of the timeline clip".into());
        }
        clip.timeline_frame_at(reference.source_frame)
    } else {
        clip.timeline_start
    };
    base.checked_add(offset)
        .filter(|position| *position >= 0)
        .ok_or("The original insertion reference is no longer valid".into())
}
