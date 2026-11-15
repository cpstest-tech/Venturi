use super::*;
use vv_core::{
    ClipId, CrossTransition, Ease, PushDirection, Rational, Track, TrackKind, Transform,
    TransformTracks, Transition, TransitionKind,
};

struct NoMediaProvider;

#[test]
fn animation_asset_passes_follow_timeline_time_after_trim_and_speed() {
    let mut clip = Clip::from_source_range(
        ClipId(1),
        ClipSource::SolidColor,
        500,
        700,
        100,
        Rational::one(),
    );
    clip.set_speed(Rational::new(2, 1), Rational::one());
    clip.effects.animations.in_asset = Some("animations/zoom_2".into());
    clip.effects.animations.out_asset = Some("animations/fade_out".into());
    clip.effects.animations.in_seconds = 1.0;
    clip.effects.animations.out_seconds = 1.0;
    let passes = asset_passes(&clip, 110, 20.0, 0.2);
    assert_eq!(passes.len(), 1);
    assert_eq!(passes[0].id, "animations/zoom_2");
    assert_eq!(passes[0].progress, 0.5);
    assert_eq!(passes[0].amount, 1.0);
    assert!(asset_passes(&clip, 140, 20.0, 1.0).is_empty());
    assert_eq!(asset_passes(&clip, 190, 20.0, 1.0)[0].progress, 0.5);
}
impl FrameProvider for NoMediaProvider {
    fn frame_for(
        &mut self,
        _: &Project,
        _: &Clip,
        _: FrameIdx,
    ) -> Result<Option<Arc<FrameYuv420>>, crate::export::ExportError> {
        Ok(None)
    }
}

fn solid_clip(id: u64, start: FrameIdx, len: FrameIdx, zoom: [f32; 2]) -> Clip {
    let mut clip = Clip::from_source_range(
        ClipId(id),
        ClipSource::SolidColor,
        0,
        len,
        start,
        Rational::one(),
    );
    clip.effects.transform = TransformTracks::constant(Transform {
        zoom,
        ..Default::default()
    });
    clip
}

fn position_of(layer: &OwnedLayer) -> [f32; 2] {
    assert!(
        matches!(layer.content, OwnedContent::Solid(_)),
        "expected a Solid layer"
    );
    layer.transform.position
}

/// Bug reported by the user: a compound clip is a timeline like any
/// other, so the areas where its nested timeline has nothing to show
/// must stay transparent — not black, or they would cover what is
/// below in the timeline containing it.
#[test]
fn an_empty_area_of_a_compound_clip_shows_the_layer_below_it() {
    let mut project = Project::default();
    // Nested timeline: a red covering only the left half.
    let mut red = solid_clip(1, 0, 10, [1.0, 1.0]);
    red.effects.color = Some(vv_core::Keyframed::constant(Rgba {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }));
    red.effects.transform = TransformTracks::constant(Transform {
        crop: [0.0, 0.0, 2.0, 0.0],
        ..Default::default()
    });
    let nested_id = project.timelines.insert(Timeline {
        name: "Nested".into(),
        fps: Rational::new(25, 1),
        resolution: (4, 4),
        tracks: vec![video_track(vec![red])],
        markers: Vec::new(),
        master: Default::default(),
    });
    let compound_media = project
        .media_pool
        .insert(compound_media_item(nested_id, (4, 4), 10));

    // Outer timeline: a blue below, the compound clip above.
    let mut blue = solid_clip(2, 0, 10, [1.0, 1.0]);
    blue.effects.color = Some(vv_core::Keyframed::constant(Rgba {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    }));
    let compound_clip = Clip::from_source_range(
        ClipId(3),
        ClipSource::Media(compound_media),
        0,
        10,
        0,
        Rational::one(),
    );
    let outer = Timeline {
        name: "Outer".into(),
        fps: Rational::new(25, 1),
        resolution: (4, 4),
        tracks: vec![video_track(vec![blue]), video_track(vec![compound_clip])],
        markers: Vec::new(),
        master: Default::default(),
    };

    let compositor = vv_render::Compositor::new_headless();
    let mut inner = NoMediaProvider;
    let mut provider = GpuCompounds::new(&mut inner, &compositor);
    let mut layers = Vec::new();
    for (track_index, clip) in outer.active_video_clips_at(0) {
        layers.extend(
            track_layers_at(
                &project,
                &outer,
                track_index,
                clip,
                0,
                outer.resolution,
                &mut provider,
            )
            .unwrap(),
        );
    }
    assert_eq!(layers.len(), 2, "the blue and the compound clip");

    let render_layers: Vec<vv_render::Layer> = layers.iter().map(OwnedLayer::as_render).collect();
    let out = compositor
        .render_layers_rgba_transparent(&render_layers, vv_render::OutputFrame::exact(4, 4));
    let px = |x: usize, y: usize| &out[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
    assert_eq!(
        px(0, 1),
        &[255, 0, 0, 255],
        "left: the red of the nested timeline"
    );
    assert_eq!(
        px(3, 1),
        &[0, 0, 255, 255],
        "right: empty in the nested one, the blue below shows"
    );
}

/// A reduced preview composes a compound clip at a fraction of its timeline
/// and the outer frame at the same fraction: the picture stays the same.
#[test]
fn a_reduced_preview_composes_a_compound_clip_smaller_with_the_same_picture() {
    let mut project = Project::default();
    let mut red = solid_clip(1, 0, 10, [1.0, 1.0]);
    red.effects.color = Some(vv_core::Keyframed::constant(Rgba {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }));
    red.effects.transform = TransformTracks::constant(Transform {
        crop: [0.0, 0.0, 4.0, 0.0],
        ..Default::default()
    });
    let nested_id = project.timelines.insert(Timeline {
        name: "Nested".into(),
        fps: Rational::new(25, 1),
        resolution: (8, 8),
        tracks: vec![video_track(vec![red])],
        markers: Vec::new(),
        master: Default::default(),
    });
    let compound_media = project
        .media_pool
        .insert(compound_media_item(nested_id, (8, 8), 10));
    let mut blue = solid_clip(2, 0, 10, [1.0, 1.0]);
    blue.effects.color = Some(vv_core::Keyframed::constant(Rgba {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    }));
    let compound_clip = Clip::from_source_range(
        ClipId(3),
        ClipSource::Media(compound_media),
        0,
        10,
        0,
        Rational::one(),
    );
    let outer = Timeline {
        name: "Outer".into(),
        fps: Rational::new(25, 1),
        resolution: (8, 8),
        tracks: vec![video_track(vec![blue]), video_track(vec![compound_clip])],
        markers: Vec::new(),
        master: Default::default(),
    };

    let compositor = vv_render::Compositor::new_headless();
    let mut inner = NoMediaProvider;
    let mut provider = GpuCompounds::new(&mut inner, &compositor).with_preview_divisor(2);
    let mut layers = Vec::new();
    for (track_index, clip) in outer.active_video_clips_at(0) {
        layers.extend(
            track_layers_at(
                &project,
                &outer,
                track_index,
                clip,
                0,
                outer.resolution,
                &mut provider,
            )
            .unwrap(),
        );
    }
    let compound_width = layers.iter().find_map(|l| match &l.content {
        OwnedContent::Texture { texture, .. } => Some(texture.width()),
        _ => None,
    });
    assert_eq!(compound_width, Some(4), "the compound at half its 8 px");

    let render_layers: Vec<vv_render::Layer> = layers.iter().map(OwnedLayer::as_render).collect();
    let out = compositor.render_layers_rgba_transparent(
        &render_layers,
        vv_render::OutputFrame::exact(8, 8).reduced(2),
    );
    let px = |x: usize, y: usize| &out[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
    assert_eq!(px(0, 1), &[255, 0, 0, 255], "left: the nested red");
    assert_eq!(px(3, 1), &[0, 0, 255, 255], "right: the blue below");
}

/// An adjustment inside a compound clip only reaches the nested tracks below
/// it: the empty part of the nested timeline stays transparent, so the outer
/// layer below shows through unfiltered.
#[test]
fn an_adjustment_inside_a_compound_clip_leaves_the_outer_timeline_alone() {
    let mut project = Project::default();
    let mut red = solid_clip(1, 0, 10, [1.0, 1.0]);
    red.effects.color = Some(vv_core::Keyframed::constant(Rgba {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }));
    red.effects.transform = TransformTracks::constant(Transform {
        crop: [0.0, 0.0, 2.0, 0.0],
        ..Default::default()
    });
    let mut adjustment =
        Clip::from_source_range(ClipId(4), ClipSource::Adjustment, 0, 10, 0, Rational::one());
    adjustment.effects.filters = vec![vv_core::ClipFilter {
        grade: vv_core::GradeTracks::constant(vv_core::GradePreset::BlackAndWhite.value()),
        ..vv_core::ClipFilter::new(vv_core::FilterKind::ColorCorrection)
    }];
    let nested_id = project.timelines.insert(Timeline {
        name: "Nested".into(),
        fps: Rational::new(25, 1),
        resolution: (4, 4),
        tracks: vec![video_track(vec![red]), video_track(vec![adjustment])],
        markers: Vec::new(),
        master: Default::default(),
    });
    let compound_media = project
        .media_pool
        .insert(compound_media_item(nested_id, (4, 4), 10));

    let mut blue = solid_clip(2, 0, 10, [1.0, 1.0]);
    blue.effects.color = Some(vv_core::Keyframed::constant(Rgba {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    }));
    let compound_clip = Clip::from_source_range(
        ClipId(3),
        ClipSource::Media(compound_media),
        0,
        10,
        0,
        Rational::one(),
    );
    let outer = Timeline {
        name: "Outer".into(),
        fps: Rational::new(25, 1),
        resolution: (4, 4),
        tracks: vec![video_track(vec![blue]), video_track(vec![compound_clip])],
        markers: Vec::new(),
        master: Default::default(),
    };

    let compositor = vv_render::Compositor::new_headless();
    let mut inner = NoMediaProvider;
    let mut provider = GpuCompounds::new(&mut inner, &compositor);
    let mut layers = Vec::new();
    for (track_index, clip) in outer.active_video_clips_at(0) {
        layers.extend(
            track_layers_at(
                &project,
                &outer,
                track_index,
                clip,
                0,
                outer.resolution,
                &mut provider,
            )
            .unwrap(),
        );
    }
    let render_layers: Vec<vv_render::Layer> = layers.iter().map(OwnedLayer::as_render).collect();
    let out = compositor
        .render_layers_rgba_transparent(&render_layers, vv_render::OutputFrame::exact(4, 4));
    let px = |x: usize, y: usize| &out[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
    let left = px(0, 1);
    assert!(
        left[0] == left[1] && left[1] == left[2] && left[0] > 0,
        "the nested red, filtered: {left:?}"
    );
    assert_eq!(px(3, 1), &[0, 0, 255, 255], "the outer blue, untouched");
}

/// Until a media inside the nested timeline is ready, the compound clip
/// produces no layer: better no frame than a half-composed
/// frame.
#[test]
fn a_compound_clip_has_no_layer_until_its_nested_media_is_ready() {
    let mut project = Project::default();
    let missing_media = project.media_pool.insert(vv_core::MediaItem {
        path: "a.mp4".into(),
        meta: vv_core::MediaMeta {
            duration_frames: 10,
            fps: Rational::new(25, 1),
            width: 4,
            height: 4,
            has_video: true,
            has_audio: false,
            sample_rate: 0,
            channels: 0,
            audio_streams: 0,
            file: Default::default(),
        },
        content_hash: 1,
        compound: None,
        folder: None,
    });
    let nested_id = project.timelines.insert(Timeline {
        name: "Nested".into(),
        fps: Rational::new(25, 1),
        resolution: (4, 4),
        tracks: vec![video_track(vec![Clip::from_source_range(
            ClipId(1),
            ClipSource::Media(missing_media),
            0,
            10,
            0,
            Rational::one(),
        )])],
        markers: Vec::new(),
        master: Default::default(),
    });
    let compound_media = project
        .media_pool
        .insert(compound_media_item(nested_id, (4, 4), 10));
    let clip = Clip::from_source_range(
        ClipId(2),
        ClipSource::Media(compound_media),
        0,
        10,
        0,
        Rational::one(),
    );
    let outer = Timeline {
        name: "Outer".into(),
        fps: Rational::new(25, 1),
        resolution: (4, 4),
        tracks: vec![video_track(vec![clip.clone()])],
        markers: Vec::new(),
        master: Default::default(),
    };

    let compositor = vv_render::Compositor::new_headless();
    let mut inner = NoMediaProvider;
    let mut provider = GpuCompounds::new(&mut inner, &compositor);
    let layers = track_layers_at(
        &project,
        &outer,
        0,
        &clip,
        0,
        outer.resolution,
        &mut provider,
    )
    .unwrap();

    assert!(
        layers.is_empty(),
        "the nested media is not cached: no layer"
    );
}

fn video_track(clips: Vec<Clip>) -> Track {
    Track {
        kind: TrackKind::Video,
        clips,
        muted: false,
        solo: false,
        locked: false,
        crossings: Vec::new(),
        mix: Default::default(),
        armed: Default::default(),
        main: false,
        subtitles: false,
        effects: false,
    }
}

fn compound_media_item(
    nested: vv_core::TimelineId,
    size: (u32, u32),
    duration: FrameIdx,
) -> vv_core::MediaItem {
    vv_core::MediaItem {
        path: "Compound Clip 1".into(),
        meta: vv_core::MediaMeta {
            duration_frames: duration,
            fps: Rational::new(25, 1),
            width: size.0,
            height: size.1,
            has_video: true,
            has_audio: false,
            sample_rate: 0,
            channels: 0,
            audio_streams: 0,
            file: Default::default(),
        },
        content_hash: 1,
        compound: Some(nested),
        folder: None,
    }
}

/// Reproduces the bug reported by the user: the right clip of the crossing,
/// zoomed 2x, must stay off screen for the whole first half of the
/// window, not pop in and then merely "pan" — see
/// `push_clearance` in vv-core for the derivation of the 1.5 factor.
#[test]
fn crossing_offsets_clear_a_zoomed_clip_fully_off_screen() {
    let left = solid_clip(1, 0, 100, [1.0, 1.0]);
    let right = solid_clip(2, 100, 100, [2.0, 2.0]);
    let track = Track {
        kind: TrackKind::Video,
        clips: vec![left.clone(), right.clone()],
        muted: false,
        solo: false,
        locked: false,
        crossings: vec![CrossTransition {
            left_clip: ClipId(1),
            right_clip: ClipId(2),
            transition: Transition {
                kind: TransitionKind::Push,
                duration: 20,
                direction: PushDirection::Right,
                ease: Ease::None,
                curve: 0.0,
                asset: None,
            },
        }],
        mix: Default::default(),
        armed: Default::default(),
        main: false,
        subtitles: false,
        effects: false,
    };
    let timeline = Timeline {
        name: "t".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks: vec![track],
        markers: Vec::new(),
        master: Default::default(),
    };
    let project = Project::default();
    let mut provider = NoMediaProvider;
    let frame_size = (1920.0, 1080.0);

    // Start of the window: the left one is still entirely in place, the zoomed
    // right one must vanish past `frame_size.0`, not stop at `frame_size.0`.
    let layers = track_layers_at(
        &project,
        &timeline,
        0,
        &left,
        90,
        timeline.resolution,
        &mut provider,
    )
    .unwrap();
    assert_eq!(position_of(&layers[0]), [0.0, 0.0]);
    assert_eq!(position_of(&layers[1]), [-1.5 * frame_size.0, 0.0]);

    // Near the end of the window (109, the last frame before the window
    // [90, 110) closes): the left one (zoom 1x) is almost entirely out
    // with the "full" clearance, the right one (zoom 2x) is almost entirely
    // in place.
    let layers = track_layers_at(
        &project,
        &timeline,
        0,
        &left,
        109,
        timeline.resolution,
        &mut provider,
    )
    .unwrap();
    let progress = 19.0 / 20.0;
    assert_eq!(position_of(&layers[0]), [progress * frame_size.0, 0.0]);
    assert_eq!(
        position_of(&layers[1]),
        [-(1.0 - progress) * 1.5 * frame_size.0, 0.0]
    );
}

#[test]
fn crossing_dissolve_fades_the_right_clip_in_over_the_left_one() {
    let left = solid_clip(1, 0, 100, [1.0, 1.0]);
    let right = solid_clip(2, 100, 100, [1.0, 1.0]);
    let mut track = Track::new(TrackKind::Video);
    track.clips = vec![left.clone(), right];
    track.crossings = vec![CrossTransition {
        left_clip: ClipId(1),
        right_clip: ClipId(2),
        transition: Transition {
            kind: TransitionKind::CrossDissolve,
            duration: 20,
            direction: PushDirection::Right,
            ease: Ease::None,
            curve: 0.0,
            asset: None,
        },
    }];
    let timeline = Timeline {
        name: "t".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks: vec![track],
        markers: Vec::new(),
        master: Default::default(),
    };
    let layers = track_layers_at(
        &Project::default(),
        &timeline,
        0,
        &left,
        100,
        timeline.resolution,
        &mut NoMediaProvider,
    )
    .unwrap();

    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].opacity, 1.0);
    assert_eq!(layers[1].opacity, 0.5);
    assert_eq!(position_of(&layers[0]), [0.0, 0.0]);
    assert_eq!(position_of(&layers[1]), [0.0, 0.0]);
}

/// Without a GPU to morph on, a Smooth Cut dissolves between the frame
/// before its window and the one after, both held still.
#[test]
fn a_smooth_cut_without_a_gpu_dissolves_between_the_held_sides() {
    let left = solid_clip(1, 0, 100, [1.0, 1.0]);
    let right = solid_clip(2, 100, 100, [1.0, 1.0]);
    let mut track = Track::new(TrackKind::Video);
    track.clips = vec![left.clone(), right];
    track.crossings = vec![CrossTransition {
        left_clip: ClipId(1),
        right_clip: ClipId(2),
        transition: Transition {
            kind: TransitionKind::SmoothCut,
            duration: 4,
            direction: PushDirection::Right,
            ease: Ease::None,
            curve: 0.0,
            asset: None,
        },
    }];
    let timeline = Timeline {
        name: "t".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks: vec![track],
        markers: Vec::new(),
        master: Default::default(),
    };
    let layers = track_layers_at(
        &Project::default(),
        &timeline,
        0,
        &left,
        99,
        timeline.resolution,
        &mut NoMediaProvider,
    )
    .unwrap();

    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].opacity, 1.0);
    assert_eq!(layers[1].opacity, 0.25);
}

fn gray_frame() -> Arc<FrameYuv420> {
    Arc::new(FrameYuv420 {
        width: 2,
        height: 2,
        y: vec![128; 4],
        chroma: vv_media::Chroma::Planar {
            u: vec![128],
            v: vec![128],
        },
        chroma_width: 1,
        chroma_height: 1,
        matrix: vv_core::ColorMatrix::Bt709,
        full_range: false,
        alpha: None,
    })
}

fn video_layer(frame: Arc<FrameYuv420>, opacity: f32) -> OwnedLayer {
    OwnedLayer {
        content: OwnedContent::Video {
            frame,
            source_size: (2, 2),
            matte: None,
        },
        transform: Transform::default(),
        opacity,
        filters: Vec::new(),
        blend: vv_core::BlendMode::Normal,
        masks: Vec::new(),
        assets: Vec::new(),
    }
}

#[test]
fn the_same_frame_with_the_same_parameters_renders_the_same() {
    let frame = gray_frame();
    assert!(renders_same(
        &[video_layer(frame.clone(), 1.0)],
        &[video_layer(frame, 1.0)]
    ));
    assert!(renders_same(&[], &[]));
}

/// Identity, not content: a new decode of identical pixels still recomposes.
#[test]
fn a_different_frame_or_parameter_renders_differently() {
    let frame = gray_frame();
    let shown = [video_layer(frame.clone(), 1.0)];
    assert!(!renders_same(&shown, &[video_layer(gray_frame(), 1.0)]));
    assert!(!renders_same(&shown, &[video_layer(frame.clone(), 0.5)]));
    assert!(!renders_same(
        &shown,
        &[video_layer(frame.clone(), 1.0), video_layer(frame, 1.0)]
    ));
}

#[test]
fn a_nan_parameter_never_renders_the_same() {
    let frame = gray_frame();
    let layer = || video_layer(frame.clone(), f32::NAN);
    assert!(!renders_same(&[layer()], &[layer()]));
}

fn colored_solid(id: u64, color: Rgba) -> Clip {
    let mut clip = solid_clip(id, 0, 10, [1.0, 1.0]);
    clip.effects.color = Some(vv_core::Keyframed::constant(color));
    clip
}

fn stack_at_0(timeline: &Timeline, provider: &mut dyn FrameProvider) -> Vec<OwnedLayer> {
    let project = Project::default();
    let mut groups = Vec::new();
    for (track_index, clip) in timeline.active_video_clips_at(0) {
        let layers = track_layers_at(
            &project,
            timeline,
            track_index,
            clip,
            0,
            timeline.resolution,
            provider,
        )
        .unwrap();
        groups.push((clip.effects.matte, layers));
    }
    stack_layers(groups, provider, timeline.resolution)
}

const RED: Rgba = Rgba {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const BLUE: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

/// Blue, red, and above them a black matte covering the left half.
fn matted_timeline(matte_mode: vv_core::MatteMode) -> Timeline {
    let mut matte = colored_solid(3, Rgba::BLACK);
    matte.effects.transform = TransformTracks::constant(Transform {
        crop: [0.0, 0.0, 2.0, 0.0],
        ..Default::default()
    });
    matte.effects.matte = Some(matte_mode);
    Timeline {
        name: "Matte".into(),
        fps: Rational::new(25, 1),
        resolution: (4, 4),
        tracks: vec![
            video_track(vec![colored_solid(1, BLUE)]),
            video_track(vec![colored_solid(2, RED)]),
            video_track(vec![matte]),
        ],
        markers: Vec::new(),
        master: Default::default(),
    }
}

#[test]
fn a_matte_cuts_out_the_track_below_and_is_not_shown() {
    let compositor = vv_render::Compositor::new_headless();
    let mut inner = NoMediaProvider;
    let mut provider = GpuCompounds::new(&mut inner, &compositor);
    let layers = stack_at_0(&matted_timeline(vv_core::MatteMode::Alpha), &mut provider);
    assert_eq!(layers.len(), 2, "the blue and the cut out red");

    let render_layers: Vec<vv_render::Layer> = layers.iter().map(OwnedLayer::as_render).collect();
    let out = compositor
        .render_layers_rgba_transparent(&render_layers, vv_render::OutputFrame::exact(4, 4));
    let px = |x: usize, y: usize| &out[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
    assert_eq!(px(0, 1), &[255, 0, 0, 255], "left: inside the matte, red");
    assert_eq!(
        px(3, 1),
        &[0, 0, 255, 255],
        "right: outside, the blue below"
    );
}

#[test]
fn a_matte_with_nothing_below_shows_nothing() {
    let mut timeline = matted_timeline(vv_core::MatteMode::Alpha);
    timeline.tracks.drain(..2);
    let compositor = vv_render::Compositor::new_headless();
    let mut inner = NoMediaProvider;
    let mut provider = GpuCompounds::new(&mut inner, &compositor);
    assert!(stack_at_0(&timeline, &mut provider).is_empty());
}

#[test]
fn without_the_gpu_a_matte_leaves_the_track_below_uncut() {
    let mut timeline = matted_timeline(vv_core::MatteMode::Alpha);
    timeline.tracks[1].clips[0].effects.blend_mode = vv_core::BlendMode::Screen;
    let layers = stack_at_0(&timeline, &mut NoMediaProvider);
    assert_eq!(layers.len(), 2);
    assert!(matches!(layers[1].content, OwnedContent::Solid(c) if c == RED));
    assert_eq!(layers[1].blend, vv_core::BlendMode::Screen);
}

#[test]
fn a_title_layer_shows_the_text_of_its_frame() {
    let mut clip = solid_clip(1, 0, 20, [1.0, 1.0]);
    clip.source = ClipSource::Text;
    let mut title = TitleParams::default();
    title
        .content
        .upsert(10, "Later".into(), vv_core::Interpolation::Hold);
    clip.effects.title = Some(title);
    let text_at = |frame| {
        let layer = clip_layer(
            &Project::default(),
            &clip,
            frame,
            (4, 4),
            25.0,
            &mut NoMediaProvider,
        )
        .unwrap()
        .unwrap();
        match layer.content {
            OwnedContent::Text(title) => title.display_text(),
            _ => panic!("not a title"),
        }
    };
    assert_eq!(text_at(9), "Later", "before the first keyframe, its text");
    assert_eq!(text_at(15), "Later");
    let mut two = clip.clone();
    two.effects.title.as_mut().unwrap().content.upsert(
        0,
        "First".into(),
        vv_core::Interpolation::Hold,
    );
    let first = clip_layer(
        &Project::default(),
        &two,
        5,
        (4, 4),
        25.0,
        &mut NoMediaProvider,
    )
    .unwrap()
    .unwrap();
    assert!(matches!(first.content, OwnedContent::Text(t) if t.display_text() == "First"));
}

#[test]
fn a_matte_over_an_adjustment_cuts_out_the_adjusted_stack() {
    let mut timeline = matted_timeline(vv_core::MatteMode::Alpha);
    let mut adjustment = solid_clip(2, 0, 10, [1.0, 1.0]);
    adjustment.source = ClipSource::Adjustment;
    let mut darken = vv_core::ClipFilter::new(vv_core::FilterKind::Exposure);
    darken.amount = vv_core::Keyframed::constant(-10.0);
    adjustment.effects.filters.push(darken);
    timeline.tracks[1].clips = vec![adjustment];

    let compositor = vv_render::Compositor::new_headless();
    let mut inner = NoMediaProvider;
    let mut provider = GpuCompounds::new(&mut inner, &compositor);
    let layers = stack_at_0(&timeline, &mut provider);
    let render_layers: Vec<vv_render::Layer> = layers.iter().map(OwnedLayer::as_render).collect();
    let out = compositor
        .render_layers_rgba_transparent(&render_layers, vv_render::OutputFrame::exact(4, 4));
    let px = |x: usize, y: usize| &out[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
    let inside = px(0, 1);
    assert!(
        inside[2] < 40 && inside[3] == 255,
        "left: inside the matte, the darkened blue: {inside:?}"
    );
    assert_eq!(px(3, 1), &[0, 0, 0, 0], "right: outside, nothing");
}

mod assets {
    use super::*;
    use vv_core::{AssetFx, Keyframed, Rgba};

    const RED: Rgba = Rgba {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    const BLUE: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    fn colored(id: u64, start: FrameIdx, len: FrameIdx, color: Rgba) -> Clip {
        let mut clip = solid_clip(id, start, len, [1.0, 1.0]);
        clip.effects.color = Some(Keyframed::constant(color));
        clip
    }

    fn timeline(tracks: Vec<Track>) -> Timeline {
        Timeline {
            name: "t".into(),
            fps: Rational::new(25, 1),
            resolution: (16, 16),
            tracks,
            markers: Vec::new(),
            master: Default::default(),
        }
    }

    fn center_pixel(compositor: &vv_render::Compositor, layers: &[OwnedLayer]) -> [u8; 4] {
        let layers: Vec<_> = layers.iter().map(OwnedLayer::as_render).collect();
        let out = compositor.render_layers(&layers, vv_render::OutputFrame::exact(16, 16));
        let i = (8 * 16 + 8) * 4;
        [out[i], out[i + 1], out[i + 2], out[i + 3]]
    }

    fn compose_frame(
        compositor: &vv_render::Compositor,
        timeline: &Timeline,
        frame: FrameIdx,
    ) -> Vec<OwnedLayer> {
        let project = Project::default();
        let mut inner = NoMediaProvider;
        let mut gpu = GpuCompounds::new(&mut inner, compositor);
        let mut groups = Vec::new();
        for (track_index, clip) in timeline.active_video_clips_at(frame) {
            let layers = track_layers_at(
                &project,
                timeline,
                track_index,
                clip,
                frame,
                timeline.resolution,
                &mut gpu,
            )
            .unwrap();
            groups.push((clip.effects.matte, layers));
        }
        stack_layers(groups, &mut gpu, timeline.resolution)
    }

    #[test]
    fn an_asset_effect_on_a_clip_processes_that_clip() {
        let compositor = vv_render::Compositor::new_headless();
        let mut clip = colored(1, 0, 50, RED);
        clip.effects.assets = vec![AssetFx::new("filters/noir", vec![])];
        let mut track = Track::new(TrackKind::Video);
        track.clips = vec![clip];
        let layers = compose_frame(&compositor, &timeline(vec![track]), 10);
        let [r, g, b, _] = center_pixel(&compositor, &layers);
        assert!(
            r.abs_diff(g) < 4 && g.abs_diff(b) < 4,
            "grey, got {r} {g} {b}"
        );
    }

    #[test]
    fn an_asset_effect_element_processes_the_tracks_below_for_its_length_only() {
        let compositor = vv_render::Compositor::new_headless();
        let mut below = Track::new(TrackKind::Video);
        below.clips = vec![colored(1, 0, 100, RED)];
        let mut element =
            Clip::from_source_range(ClipId(2), ClipSource::Adjustment, 0, 50, 0, Rational::one());
        element.effects.assets = vec![AssetFx::new("filters/noir", vec![])];
        let mut above = Track::new(TrackKind::Video);
        above.clips = vec![element];
        let timeline = timeline(vec![below, above]);

        let inside = compose_frame(&compositor, &timeline, 10);
        let [r, g, b, _] = center_pixel(&compositor, &inside);
        assert!(
            r.abs_diff(g) < 4 && g.abs_diff(b) < 4,
            "grey, got {r} {g} {b}"
        );

        let outside = compose_frame(&compositor, &timeline, 70);
        let [r, g, b, _] = center_pixel(&compositor, &outside);
        assert!(
            r > 240 && g < 15 && b < 15,
            "untouched red, got {r} {g} {b}"
        );
    }

    fn is_grey([r, g, b, _]: [u8; 4]) -> bool {
        r.abs_diff(g) < 4 && g.abs_diff(b) < 4
    }

    #[test]
    fn an_element_over_two_clips_acts_on_both_only_in_its_interval() {
        let compositor = vv_render::Compositor::new_headless();
        let mut main = Track::new_main();
        main.clips = vec![colored(1, 0, 50, RED), colored(2, 50, 50, BLUE)];
        let mut element = Clip::from_source_range(
            ClipId(3),
            ClipSource::Adjustment,
            0,
            50,
            25,
            Rational::one(),
        );
        element.effects.assets = vec![AssetFx::new("filters/noir", vec![])];
        let mut effects = Track::new_effects();
        effects.clips = vec![element];
        let mut timeline = timeline(vec![main, effects]);

        let pixel = |timeline: &Timeline, frame| {
            center_pixel(&compositor, &compose_frame(&compositor, timeline, frame))
        };
        assert!(is_grey(pixel(&timeline, 30)), "red clip inside");
        assert!(is_grey(pixel(&timeline, 60)), "blue clip inside");
        assert!(!is_grey(pixel(&timeline, 10)), "before the element");
        assert!(!is_grey(pixel(&timeline, 90)), "after the element");

        timeline.tracks[1].clips[0].timeline_start = 60;
        assert!(!is_grey(pixel(&timeline, 30)), "moved away");

        timeline.tracks[1].muted = true;
        assert!(!is_grey(pixel(&timeline, 70)), "hidden by its eye");
    }

    #[test]
    fn an_element_leaves_the_tracks_above_it_alone() {
        let compositor = vv_render::Compositor::new_headless();
        let mut main = Track::new_main();
        main.clips = vec![colored(1, 0, 100, RED)];
        let mut element = Clip::from_source_range(
            ClipId(2),
            ClipSource::Adjustment,
            0,
            100,
            0,
            Rational::one(),
        );
        element.effects.assets = vec![AssetFx::new("filters/noir", vec![])];
        let mut effects = Track::new_effects();
        effects.clips = vec![element];
        let mut top = Track::new(TrackKind::Video);
        top.clips = vec![colored(3, 0, 100, BLUE)];
        let timeline = timeline(vec![main, effects, top]);
        let [r, g, b, _] = center_pixel(&compositor, &compose_frame(&compositor, &timeline, 10));
        assert!(
            b > 240 && r < 15 && g < 15,
            "untouched blue, got {r} {g} {b}"
        );
    }

    #[test]
    fn an_asset_transition_merges_both_sides_into_one_layer() {
        let compositor = vv_render::Compositor::new_headless();
        let mut track = Track::new(TrackKind::Video);
        track.clips = vec![colored(1, 0, 100, RED), colored(2, 100, 100, BLUE)];
        track.crossings = vec![CrossTransition {
            left_clip: ClipId(1),
            right_clip: ClipId(2),
            transition: Transition {
                kind: TransitionKind::CrossDissolve,
                duration: 20,
                direction: PushDirection::Right,
                ease: Ease::None,
                curve: 0.0,
                asset: Some(AssetFx::new("transitions/dissolve", vec![])),
            },
        }];
        let timeline = timeline(vec![track]);

        let halfway = compose_frame(&compositor, &timeline, 100);
        assert_eq!(halfway.len(), 1);
        let [r, _, b, _] = center_pixel(&compositor, &halfway);
        assert!(
            (120..=135).contains(&r) && (120..=135).contains(&b),
            "{r} {b}"
        );

        let before = compose_frame(&compositor, &timeline, 50);
        let [r, _, b, _] = center_pixel(&compositor, &before);
        assert!(r > 240 && b < 15);
    }

    #[test]
    fn a_transition_tolerates_one_frame_of_gap_but_not_two() {
        let make = |gap: FrameIdx| {
            let mut track = Track::new(TrackKind::Video);
            track.clips = vec![colored(1, 0, 100, RED), colored(2, 100 + gap, 100, BLUE)];
            track.crossings = vec![CrossTransition {
                left_clip: ClipId(1),
                right_clip: ClipId(2),
                transition: Transition {
                    kind: TransitionKind::CrossDissolve,
                    duration: 20,
                    direction: PushDirection::Right,
                    ease: Ease::None,
                    curve: 0.0,
                    asset: None,
                },
            }];
            track
        };
        assert!(make(1).crossing_at(95).is_some());
        assert!(make(2).crossing_at(95).is_none());
    }
}

#[test]
fn generated_pop_subtitles_pop_in_on_the_rendered_frames() {
    let mut project = Project::default();
    let timeline = project.timelines.insert(Timeline {
        name: "T".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks: Vec::new(),
        markers: Vec::new(),
        master: Default::default(),
    });
    let words: Vec<vv_core::CaptionWord> =
        [("Ciao", 0, 400), ("a", 400, 700), ("tutti", 700, 1200)]
            .into_iter()
            .map(|(text, start_ms, end_ms)| vv_core::CaptionWord {
                text: text.into(),
                start_ms,
                end_ms,
            })
            .collect();
    let template = vv_core::SubtitleTemplate::find("word_pop").unwrap();
    let clips = vv_core::captions::build_clips(&mut project, timeline, &words, template);
    assert_eq!(clips.len(), 3);
    let ink_at = |clip: &Clip, frame: FrameIdx| -> u64 {
        let layer = clip_layer(
            &project,
            clip,
            frame,
            (1920, 1080),
            25.0,
            &mut NoMediaProvider,
        )
        .unwrap()
        .unwrap();
        let OwnedContent::Text(title) = layer.content else {
            panic!("not a title");
        };
        vv_render::text::render_title(&title, (1920, 1080), (480, 270))
            .masks()
            .iter()
            .map(|(placed, color)| {
                placed.mask.data.iter().map(|&v| u64::from(v)).sum::<u64>()
                    * (color.a * 100.0) as u64
            })
            .sum()
    };
    for clip in &clips {
        let first = ink_at(clip, clip.timeline_start);
        let settled = ink_at(clip, clip.timeline_end() - 1);
        assert!(
            first * 2 < settled,
            "clip at {} does not pop: {first} vs {settled}",
            clip.timeline_start
        );
    }
}

#[test]
fn clip_animation_reaches_the_shared_preview_export_layer() {
    let project = Project::default();
    let mut clip = solid_clip(1, 100, 50, [2.0, 2.0]);
    clip.effects.animations.entrance = Some(vv_core::clip_anim::Entrance::FromLeft);
    clip.effects.animations.pulse = true;
    let mut provider = NoMediaProvider;
    let first = clip_layer(&project, &clip, 100, (1920, 1080), 25.0, &mut provider)
        .unwrap()
        .unwrap();
    assert_eq!(first.transform.position[0], -1920.0);
    let middle = clip_layer(&project, &clip, 125, (1920, 1080), 25.0, &mut provider)
        .unwrap()
        .unwrap();
    assert_eq!(middle.transform.position[0], 0.0);
    assert_eq!(middle.transform.zoom, [2.0; 2]);
}
