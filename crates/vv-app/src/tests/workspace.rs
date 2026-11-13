use super::*;

#[test]
fn the_default_layout_docks_every_block() {
    let workspace = Workspace::default();
    for block in Block::ALL {
        assert!(workspace.is_docked(block), "{block:?}");
    }
    assert!(workspace.closed_blocks().is_empty());
    assert!(workspace.detached_blocks().is_empty());
}

#[test]
fn a_closed_block_is_listed_and_comes_back_on_reopen() {
    let mut workspace = Workspace::default();
    assert!(workspace.undock(Block::Properties));
    assert_eq!(workspace.closed_blocks(), vec![Block::Properties]);

    workspace.reopen(Block::Properties);
    assert!(workspace.is_docked(Block::Properties));
    assert!(workspace.closed_blocks().is_empty());
}

#[test]
fn reopening_a_shown_block_does_not_duplicate_it() {
    let mut workspace = Workspace::default();
    workspace.reopen(Block::Player);
    assert_eq!(
        workspace
            .docked_blocks()
            .iter()
            .filter(|&&b| b == Block::Player)
            .count(),
        1
    );
}

#[test]
fn a_detached_block_leaves_the_tree_and_redocks() {
    let mut workspace = Workspace::default();
    workspace.detach(Block::Player, egui::vec2(640.0, 360.0));
    assert!(!workspace.is_docked(Block::Player));
    assert!(workspace.is_detached(Block::Player));
    assert!(workspace.closed_blocks().is_empty());
    let builder = workspace.viewport_builder(Block::Player).unwrap();
    assert_eq!(builder.inner_size, Some(egui::vec2(640.0, 360.0)));

    workspace.redock(Block::Player);
    assert!(workspace.is_docked(Block::Player));
    assert!(!workspace.is_detached(Block::Player));
}

#[test]
fn every_block_can_be_redocked_into_an_emptied_tree() {
    let mut workspace = Workspace::default();
    for block in Block::ALL {
        workspace.detach(block, egui::Vec2::ZERO);
    }
    assert!(!workspace.has_docked_blocks());
    for block in Block::ALL {
        workspace.redock(block);
    }
    for block in Block::ALL {
        assert!(workspace.is_docked(block), "{block:?}");
    }
}

#[test]
fn the_layout_round_trips_through_the_settings_value() {
    let mut workspace = Workspace::default();
    workspace.detach(Block::Timeline, egui::vec2(1200.0, 300.0));
    workspace.undock(Block::Library);
    let value = workspace.to_value();

    let loaded = Workspace::from_value(value.as_ref());
    assert!(loaded.is_detached(Block::Timeline));
    assert_eq!(loaded.closed_blocks(), vec![Block::Library]);
    assert!(loaded.is_docked(Block::Player));
}

#[test]
fn an_unreadable_or_inconsistent_layout_falls_back_to_the_default() {
    let garbage = serde_json::json!({ "tree": 3 });
    assert_eq!(Workspace::from_value(Some(&garbage)), Workspace::default());

    let mut twice = Workspace::default();
    twice.detached.push(Detached {
        block: Block::Player,
        window: WindowGeometry {
            pos: None,
            size: DEFAULT_WINDOW_SIZE,
        },
        opened_with: None,
    });
    assert_eq!(
        Workspace::from_value(twice.to_value().as_ref()),
        Workspace::default()
    );
}
