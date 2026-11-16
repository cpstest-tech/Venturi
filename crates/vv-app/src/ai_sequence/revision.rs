use vv_core::{
    ClipId, ClipSource, Command, CommandLabel, MediaId, Project, SetClipValue, TimelineId,
};

pub fn replacement(
    project: &Project,
    timeline: TimelineId,
    clip_id: ClipId,
    expected: MediaId,
    replacement: MediaId,
) -> Result<Box<dyn Command>, String> {
    let timeline_ref = project
        .timelines
        .get(timeline)
        .ok_or("The original timeline no longer exists")?;
    let (track_index, clip) = timeline_ref
        .tracks
        .iter()
        .enumerate()
        .find_map(|(index, track)| {
            track
                .clips
                .iter()
                .find(|clip| clip.id == clip_id)
                .map(|clip| (index, clip))
        })
        .ok_or("The original clip no longer exists")?;
    if timeline_ref.tracks[track_index].locked {
        return Err("Unlock the clip's track before applying a revision".into());
    }
    if !matches!(clip.source, ClipSource::Media(id) if id == expected) {
        return Err("This clip already uses a different revision; select its current version before applying another".into());
    }
    let old = project
        .media_pool
        .get(expected)
        .ok_or("Previous media is missing")?;
    let new = project
        .media_pool
        .get(replacement)
        .ok_or("Import the completed revision first")?;
    if old.meta.fps != new.meta.fps {
        return Err("The revision frame rate differs from the current source".into());
    }
    let last = clip
        .freeze
        .map(|f| f + 1)
        .unwrap_or_else(|| clip.source_out());
    if clip.source_in() < 0 || last > new.meta.duration_frames {
        return Err("The revision is too short for the current trim; adjust the clip or generate a longer revision".into());
    }
    Ok(Box::new(SetClipValue::new(
        timeline,
        track_index,
        clip_id,
        CommandLabel::RelinkMedia,
        ClipSource::Media(replacement),
        |clip| &mut clip.source,
    )))
}

pub fn family<'a>(jobs: &'a [super::Job], id: &'a str) -> &'a str {
    let mut current = id;
    for _ in 0..jobs.len() {
        match jobs
            .iter()
            .find(|job| job.id == current)
            .and_then(|job| job.request.parent.as_deref())
        {
            Some(parent) => current = parent,
            None => return current,
        }
    }
    id
}
