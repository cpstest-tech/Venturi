use super::*;

#[test]
fn a_resolution_preset_keeps_the_aspect_with_even_sides() {
    assert_eq!(with_short_side((1920, 1080), 2160), (3840, 2160));
    assert_eq!(with_short_side((1080, 1920), 720), (720, 1280));
    assert_eq!(with_short_side((1440, 1080), 720), (960, 720));
    assert_eq!(with_short_side((2560, 1080), 1440), (3412, 1440));
}
