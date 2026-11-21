use super::*;

fn area(w: f32, h: f32) -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h))
}

fn no_overlap(rects: &BlockRects) {
    let all: Vec<_> = rects.iter().collect();
    for (i, (a, ra)) in all.iter().enumerate() {
        for (b, rb) in &all[i + 1..] {
            assert!(
                !ra.intersects(**rb) || ra.intersect(**rb).area() < 1e-3,
                "{a:?} overlaps {b:?}"
            );
        }
    }
}

#[test]
fn closed_drawers_leave_player_on_top_and_timeline_below() {
    let (bar, rects) = rects(area(1080.0, 1920.0), false, false);
    let player = rects[&Block::Player];
    let timeline = rects[&Block::Timeline];
    assert_eq!(rects.len(), 2);
    assert!(bar.bottom() <= player.top());
    assert!(player.bottom() <= timeline.top());
    assert_eq!(player.width(), 1080.0);
    assert!(player.height() > timeline.height() * 1.5);
    assert!(timeline.height() <= TIMELINE_MAX);
}

#[test]
fn drawers_push_the_player_without_overlapping() {
    let (_, rects) = rects(area(1366.0, 768.0), true, true);
    assert_eq!(rects.len(), 4);
    no_overlap(&rects);
    assert!(rects[&Block::Player].width() >= MIN_PLAYER_WIDTH);
    assert!(rects[&Block::Library].left() < rects[&Block::Player].left());
    assert!(rects[&Block::Properties].right() > rects[&Block::Player].right());
}

#[test]
fn a_drawer_on_a_phone_width_becomes_a_full_sheet() {
    let (_, rects) = rects(area(420.0, 800.0), true, false);
    assert!(!rects.contains_key(&Block::Player));
    assert_eq!(rects[&Block::Library].width(), 420.0);
    no_overlap(&rects);
}

#[test]
fn opening_a_drawer_closes_the_other_when_both_do_not_fit() {
    let mut state = State::default();
    state.toggle(Block::Library, 700.0);
    state.toggle(Block::Properties, 700.0);
    assert!(state.properties_open && !state.library_open);
    state.toggle(Block::Library, 1400.0);
    assert!(state.properties_open && state.library_open);
}

#[test]
fn the_window_shape_suggests_the_layout_once_until_it_changes() {
    let mut state = State::default();
    let narrow = egui::vec2(800.0, 1000.0);
    let wide = egui::vec2(1600.0, 900.0);
    assert_eq!(suggested(LayoutMode::Desktop, wide), None);
    assert_eq!(
        state.proposal(LayoutMode::Desktop, narrow),
        Some(LayoutMode::Vertical)
    );
    state.dismiss(LayoutMode::Vertical);
    assert_eq!(state.proposal(LayoutMode::Desktop, narrow), None);
    assert_eq!(state.proposal(LayoutMode::Desktop, wide), None);
    assert_eq!(
        state.proposal(LayoutMode::Desktop, narrow),
        Some(LayoutMode::Vertical)
    );
    assert_eq!(
        state.proposal(LayoutMode::Vertical, wide),
        Some(LayoutMode::Desktop)
    );
    assert_eq!(state.proposal(LayoutMode::Vertical, narrow), None);
}
