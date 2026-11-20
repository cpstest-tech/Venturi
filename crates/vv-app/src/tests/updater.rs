use super::*;

#[test]
fn versions_parse_with_or_without_the_v_and_a_suffix() {
    assert_eq!(parse_version("v0.10.2"), Some((0, 10, 2)));
    assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
    assert_eq!(parse_version("v1.2.3-rc1"), Some((1, 2, 3)));
    assert_eq!(parse_version("v1.2"), None);
    assert_eq!(parse_version("v1.2.3.4"), None);
    assert_eq!(parse_version("latest"), None);
}

#[test]
fn only_a_strictly_higher_version_is_newer() {
    assert!(is_newer("v0.10.3", "0.10.2"));
    assert!(is_newer("v0.11.0", "0.10.9"));
    assert!(is_newer("v1.0.0", "0.99.99"));
    assert!(!is_newer("v0.10.2", "0.10.2"));
    assert!(!is_newer("v0.10.1", "0.10.2"));
    assert!(!is_newer("nightly", "0.10.2"));
}

fn release() -> Release {
    serde_json::from_str(
        r#"{
            "tag_name": "v9.0.0",
            "html_url": "https://github.com/cpstest-tech/tandem/releases/tag/v9.0.0",
            "assets": [
                {"name": "Tandem-x86_64.AppImage", "size": 3,
                 "browser_download_url": "https://example.invalid/a",
                 "digest": "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},
                {"name": "Tandem-windows-x64-setup.exe", "size": 3,
                 "browser_download_url": "https://example.invalid/b"}
            ],
            "unknown_field": true
        }"#,
    )
    .unwrap()
}

#[test]
fn each_method_picks_its_platform_file() {
    let release = release();
    let appimage = Method::AppImage("/tmp/Tandem.AppImage".into());
    if std::env::consts::ARCH == "x86_64" {
        assert_eq!(
            appimage.asset(&release).unwrap().name,
            "Tandem-x86_64.AppImage"
        );
    }
    assert!(Method::WindowsInstaller.asset(&release).is_some());
    assert!(Method::MacDmg.asset(&release).is_none());
    assert!(Method::Manual.asset(&release).is_none());
}

#[test]
fn a_download_must_match_the_size_and_the_digest() {
    let asset = &release().assets[0];
    let abc = ring::digest::digest(&ring::digest::SHA256, b"abc");
    assert!(verify(asset, 3, abc.as_ref()).is_ok());
    assert!(verify(asset, 2, abc.as_ref()).is_err());
    let other = ring::digest::digest(&ring::digest::SHA256, b"abd");
    assert!(verify(asset, 3, other.as_ref()).is_err());
}

#[test]
fn a_file_without_digest_is_checked_by_size_only() {
    let asset = &release().assets[1];
    assert!(verify(asset, 3, &[0; 32]).is_ok());
    assert!(verify(asset, 4, &[0; 32]).is_err());
}
