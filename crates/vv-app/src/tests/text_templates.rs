use super::*;
use vv_core::text_template::TemplateCategory;

const NAMES: [&str; 23] = [
    "Neon Pulse",
    "Bubble Pop",
    "Hype Impact",
    "Sticker Pack",
    "Gradient Wave",
    "Retro 80s",
    "Handwritten Marker",
    "Glitch RGB",
    "Karaoke Pop",
    "Comic Boom",
    "Elegant Serif",
    "Pixel Game",
    "Fire & Ice",
    "Sunset Script",
    "Outline Hollow",
    "Lower Third Bold",
    "Pause",
    "On Fire",
    "Modern Halo",
    "Wide",
    "Blue Outline",
    "Cracked Glow",
    "Purple Glow",
];

#[test]
fn all_templates_load() {
    assert_eq!(all().len(), SOURCES.len());
    let names: Vec<&str> = all().iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, NAMES);
}

#[test]
fn ids_are_unique_and_match_the_file_names() {
    let mut ids: Vec<&str> = all().iter().map(|t| t.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), SOURCES.len());
}

#[test]
fn every_category_has_a_template_and_a_duration() {
    for category in TemplateCategory::ALL {
        assert!(in_category(category).next().is_some(), "{category:?}");
    }
    for t in all() {
        assert!(t.duration >= 2.0 && t.duration <= 6.0, "{}", t.id);
        assert!(!t.tags.is_empty(), "{}", t.id);
    }
}

#[test]
fn every_template_uses_a_bundled_font_and_originals_animate_in_and_out() {
    register_fonts();
    let families = vv_render::text::font_families();
    for t in all() {
        for clip in t.clips((1920, 1080)) {
            let a = &clip.title.anim;
            if !t.id.starts_with("capcut_") {
                assert!(a.enter.is_some() && a.exit.is_some(), "{}", t.id);
            }
            let font = &clip.title.font_family;
            assert!(families.iter().any(|f| f == font), "{}: {font}", t.id);
        }
    }
}

#[test]
fn some_templates_have_several_levels_and_one_is_karaoke() {
    assert!(all().iter().filter(|t| t.layers.len() > 1).count() >= 3);
    assert!(find("karaoke_pop").unwrap().is_karaoke());
}

#[test]
fn ported_templates_render_as_composites_in_portrait_and_landscape() {
    register_fonts();
    for template in all().iter().filter(|t| t.id.starts_with("capcut_")) {
        for resolution in [(1080, 1920), (1920, 1080)] {
            assert_eq!(
                template.clips(resolution).len(),
                1,
                "{} must edit as one title",
                template.id
            );
            let titles: Vec<_> = template
                .clips(resolution)
                .into_iter()
                .map(|clip| clip.title.animated(1.8, clip.length_secs))
                .collect();
            let image = crate::library_ribbon::layers_preview_image(&titles, resolution);
            let lit = image
                .pixels
                .iter()
                .filter(|p| p.r() > 60 || p.g() > 60)
                .count();
            assert!(lit > 40, "{} at {resolution:?} draws nothing", template.id);
        }
    }
}

#[test]
fn every_template_renders_ink_at_every_animation_stage() {
    register_fonts();
    for t in all() {
        if t.id.starts_with("capcut_") {
            continue;
        }
        for clip in t.clips((1920, 1080)) {
            let mut title = clip.title.clone();
            title.anim_state = title.anim.state_at(
                &title.display_text(),
                clip.length_secs / 2.0,
                clip.length_secs,
            );
            let image = crate::library_ribbon::title_preview_image(&title, (1920, 1080));
            let lit = image
                .pixels
                .iter()
                .filter(|p| p.r() > 60 || p.g() > 60)
                .count();
            assert!(lit > 40, "{} draws nothing", t.id);
        }
    }
}

#[test]
fn search_looks_at_name_tags_and_category() {
    let neon = find("neon_pulse").unwrap();
    assert!(matches(neon, "Titles", "neon"));
    assert!(matches(neon, "Titles", "GLOW titles"));
    assert!(!matches(neon, "Titles", "serif"));
    assert!(matches(neon, "Titles", ""));
}

fn register_fonts() {
    crate::fonts::register_bundled();
}

/// `VV_TEXT_SHEET_DIR=<dir>` dumps a PPM per template (at rest and at the
/// middle of its In) for a visual check.
#[test]
fn dump_sheets_when_asked() {
    let Ok(dir) = std::env::var("VV_TEXT_SHEET_DIR") else {
        return;
    };
    register_fonts();
    for t in all() {
        for (label, at) in [("rest", None), ("in", Some(0.3f32))] {
            let titles: Vec<_> = t
                .clips((1920, 1080))
                .into_iter()
                .map(|c| {
                    let mut title = c.title;
                    if let Some(at) = at {
                        title.anim_state =
                            title
                                .anim
                                .state_at(&title.display_text(), at, c.length_secs);
                    }
                    title
                })
                .collect();
            let image = crate::library_ribbon::layers_preview_image(&titles, (1920, 1080));
            let mut out = format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
            out.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
            std::fs::write(format!("{dir}/{}-{label}.ppm", t.id), out).unwrap();
        }
    }
}

#[test]
fn fit_width_scale_shrinks_only_what_spills() {
    assert_eq!(fit_width_scale(800.0, HAnchor::Center, 0.0, 1920.0), 1.0);
    let s = fit_width_scale(2400.0, HAnchor::Center, 0.0, 1080.0);
    assert!((2400.0 * s - 1080.0 * 0.9).abs() < 0.01);
    // Anchored left at the center: only the right half is free.
    let s = fit_width_scale(1000.0, HAnchor::Left, 0.0, 1080.0);
    assert!((1000.0 * s - 540.0 * 0.9).abs() < 0.01);
    let s = fit_width_scale(1000.0, HAnchor::Right, 0.0, 1080.0);
    assert!((1000.0 * s - 540.0 * 0.9).abs() < 0.01);
    // An anchor already outside the frame is left alone.
    assert_eq!(fit_width_scale(1000.0, HAnchor::Left, 600.0, 1080.0), 1.0);
}
