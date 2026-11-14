use super::*;
use crate::stickers::{rasterize, render_rgba};
use std::collections::HashSet;

fn coverage(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 16).count()
}

#[test]
fn slugs_are_unique_and_loops_have_a_length() {
    let slugs: HashSet<_> = DEFS.iter().map(|d| d.slug).collect();
    assert_eq!(slugs.len(), DEFS.len());
    for def in DEFS {
        assert!((0.5..=4.0).contains(&def.secs), "{}", def.slug);
        assert!((0.0..1.0).contains(&def.still), "{}", def.slug);
    }
}

#[test]
fn every_icon_is_a_phosphor_icon() {
    let known: HashSet<&str> = include_str!("../../assets/stickers/phosphor.txt")
        .lines()
        .collect();
    for def in DEFS {
        if let Body::Icon(name, _) = def.body {
            assert!(
                known.contains(name),
                "{} uses unknown icon {name}",
                def.slug
            );
        }
    }
}

const ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="currentColor"><circle cx="128" cy="128" r="90"/></svg>"#;

#[test]
fn every_frame_of_every_sticker_draws_something_valid() {
    for def in DEFS {
        for step in 0..8 {
            let t = step as f32 / 8.0;
            let svg = frame(def, Some(ICON), t);
            let rgba =
                render_rgba(&svg, 64, true).unwrap_or_else(|e| panic!("{} at {t}: {e}", def.slug));
            // Drawings and fades may be empty where they restart.
            if step == 5 {
                assert!(coverage(&rgba) > 0, "{} is empty at {t}", def.slug);
            }
        }
    }
}

#[test]
fn the_still_of_a_tile_shows_the_sticker() {
    for def in DEFS {
        let svg = frame(def, Some(ICON), def.still);
        let png = rasterize(&svg, 64, true).unwrap();
        assert!(png.len() > 200, "{}", def.slug);
        let rgba = render_rgba(&svg, 64, true).unwrap();
        assert!(coverage(&rgba) > 40, "{} still is empty", def.slug);
    }
}

#[test]
fn loops_close_on_themselves() {
    for def in DEFS {
        let n = (def.secs * FRAME_RATE as f32).round();
        let first = render_rgba(&frame(def, Some(ICON), 0.0), 64, false).unwrap();
        let last = render_rgba(&frame(def, Some(ICON), (n - 1.0) / n), 64, false).unwrap();
        let moved = first
            .as_chunks::<4>()
            .0
            .iter()
            .zip(last.as_chunks::<4>().0)
            .filter(|(a, b)| a[3].abs_diff(b[3]) > 100)
            .count();
        assert!(
            moved < 64 * 64 / 6,
            "{} jumps at the loop point ({moved})",
            def.slug
        );
    }
}

#[test]
fn pastel_stickers_keep_their_own_colors() {
    let pastel: Vec<_> = DEFS.iter().filter(|d| d.colored()).collect();
    assert_eq!(pastel.len(), 6);
    for def in pastel {
        assert!(!crate::stickers::is_tintable_path(std::path::Path::new(
            &format!("anim-{}.webm", def.slug)
        )));
    }
}
