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
