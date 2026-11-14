use super::*;

const TWO_LAYERS: &str = r##"{
  "version": 1, "id": "t", "name": "T", "category": "titles",
  "tags": ["a"], "duration": 4.0,
  "layers": [
    {"text": "Big", "font": "Anton", "size": 200, "color": "#FF0000",
     "strokes": [{"color": "#000000", "width": 10}],
     "anim": {"in": {"preset": "pop", "duration": 0.5}, "loop": {"preset": "pulse"}}},
    {"text": "Small", "size": 50, "color": "#FFFFFF80", "start": 1.0, "length": 2.0,
     "anchor": {"h": "left", "v": "bottom"}}
  ]
}"##;

#[test]
fn colours_parse_with_and_without_alpha() {
    let c = parse_color("#FF8000").unwrap();
    assert_eq!((c.r, c.a), (1.0, 1.0));
    assert!((parse_color("#00000080").unwrap().a - 0.502).abs() < 0.01);
    assert!(parse_color("red").is_err());
    assert!(parse_color("#FFF").is_err());
    assert!(parse_color("#GG0000").is_err());
}

#[test]
fn layers_become_clips_scaled_to_the_frame() {
    let def = TextTemplateDef::from_json(TWO_LAYERS).unwrap();
    let hd = def.clips((1920, 1080));
    let uhd = def.clips((3840, 2160));
    assert_eq!(hd.len(), 2);
    assert_eq!(hd[0].title.size, 200.0);
    assert_eq!(uhd[0].title.size, 400.0);
    assert_eq!(uhd[0].title.style.strokes[0].width, 20.0);
    assert_eq!(hd[0].title.font_family, "Anton");
    assert_eq!(hd[0].length_secs, 4.0);
    assert_eq!((hd[1].start_secs, hd[1].length_secs), (1.0, 2.0));
    assert_eq!(hd[1].title.anchor.0, HAnchor::Left);
    assert!((hd[1].title.color.a - 0.502).abs() < 0.01);
}

#[test]
fn animations_come_from_the_presets_with_overrides() {
    let def = TextTemplateDef::from_json(TWO_LAYERS).unwrap();
    let anim = &def.clips((1920, 1080))[0].title.anim;
    assert_eq!(anim.enter.as_ref().unwrap().id, "pop");
    assert_eq!(anim.enter.as_ref().unwrap().duration, 0.5);
    assert_eq!(anim.looped.as_ref().unwrap().id, "pulse");
    assert!(anim.exit.is_none() && anim.karaoke.is_none());
}

#[test]
fn bad_templates_are_refused() {
    let unknown = TWO_LAYERS.replace("\"pop\"", "\"nope\"");
    assert!(matches!(
        TextTemplateDef::from_json(&unknown),
        Err(TemplateError::Preset(_))
    ));
    let newer = TWO_LAYERS.replace("\"version\": 1", "\"version\": 99");
    assert!(matches!(
        TextTemplateDef::from_json(&newer),
        Err(TemplateError::Newer(99))
    ));
    let bad_color = TWO_LAYERS.replace("#FF0000", "red");
    assert!(TextTemplateDef::from_json(&bad_color).is_err());
    assert!(TextTemplateDef::from_json("{").is_err());
}

#[test]
fn json_round_trips() {
    let def = TextTemplateDef::from_json(TWO_LAYERS).unwrap();
    let again = TextTemplateDef::from_json(&def.to_json().unwrap()).unwrap();
    assert_eq!(def, again);
}

#[test]
fn a_subtitle_takes_the_first_layer_style_at_the_bottom() {
    let def = TextTemplateDef::from_json(TWO_LAYERS).unwrap();
    let title = def.subtitle_title("hello", (1920, 1080));
    assert_eq!(title.display_text(), "hello");
    assert_eq!(title.size, 65.0);
    assert_eq!(title.anchor, (HAnchor::Center, VAnchor::Bottom));
    assert_eq!(title.font_family, "Anton");
    assert!(title.style.strokes[0].width < 10.0);
    assert!(title.anim.enter.is_some());
}

#[test]
fn a_saved_title_becomes_a_subtitle_keeping_its_look() {
    let mut base = TitleParams::default();
    base.size = 100.0;
    base.style.strokes.push(TextStroke {
        width: 10.0,
        ..TextStroke::default()
    });
    let title = subtitle_from_title(&base, "line", (1920, 1080), 0.05);
    assert_eq!(title.size, 54.0);
    assert!((title.style.strokes[0].width - 5.4).abs() < 1e-3);
    assert_eq!(title.display_text(), "line");
}
