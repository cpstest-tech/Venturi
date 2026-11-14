use super::*;

#[test]
fn builtins_parse_and_cover_every_kind() {
    let lib = AssetLibrary::load(None);
    for kind in AssetKind::ALL {
        assert!(lib.of_kind(kind).count() >= 6, "{kind:?}");
    }
    for asset in lib.of_kind(AssetKind::Transition) {
        assert!(asset.default_secs > 0.0);
    }
    for id in [
        "effects/gaussian_blur",
        "effects/flash",
        "effects/zoom_lens",
    ] {
        assert!(lib.get(id).is_some(), "{id}");
    }
}

#[test]
fn shader_params_fall_back_to_defaults_and_clamp() {
    let lib = AssetLibrary::load(None);
    let asset = lib
        .assets
        .iter()
        .find(|a| !a.params.is_empty())
        .expect("an asset with parameters");
    let p = &asset.params[0];
    assert_eq!(asset.shader_params(&[])[0], p.default);
    assert_eq!(asset.shader_params(&[p.max + 100.0])[0], p.max);
}

#[test]
fn user_folder_adds_and_overrides() {
    let dir = std::env::temp_dir().join(format!("vv-assets-{}", std::process::id()));
    let folder = dir.join("effects/zoom_extra");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("manifest.json"),
        r#"{"kind":"effect","name":{"en":"Extra"},"category":"mine"}"#,
    )
    .unwrap();
    std::fs::write(
        folder.join("shader.wgsl"),
        "fn effect(uv: vec2<f32>) -> vec4<f32> { return src(uv); }",
    )
    .unwrap();
    let lib = AssetLibrary::load(Some(&dir));
    let asset = lib.get("effects/zoom_extra").unwrap();
    assert_eq!(asset.name.get("it"), "Extra");
    assert_eq!(asset.default_secs, 3.0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn curves_interpolate_between_even_samples() {
    let asset = parse(
        "transitions/t",
        r#"{"kind":"transition","name":{"en":"T"},"curves":[[0,1,0],[2]]}"#,
        "fn effect(uv: vec2<f32>) -> vec4<f32> { return src(uv); }",
    )
    .unwrap();
    let at = |p| asset.curve_values(p);
    assert_eq!(at(0.0)[0], 0.0);
    assert_eq!(at(0.25)[0], 0.5);
    assert_eq!(at(0.5)[0], 1.0);
    assert_eq!(at(1.0)[0], 0.0);
    assert_eq!(at(7.0)[0], 0.0);
    assert_eq!(at(0.3)[1], 2.0);
    assert_eq!(at(0.3)[2], 0.0);
}

#[test]
fn bad_curves_are_rejected() {
    let shader = "fn effect(uv: vec2<f32>) -> vec4<f32> { return src(uv); }";
    let empty = r#"{"kind":"transition","name":{"en":"T"},"curves":[[]]}"#;
    assert!(parse("transitions/t", empty, shader).is_err());
    let many = format!(
        r#"{{"kind":"transition","name":{{"en":"T"}},"curves":[{}]}}"#,
        vec!["[0]"; MAX_CURVES + 1].join(",")
    );
    assert!(parse("transitions/t", &many, shader).is_err());
}
