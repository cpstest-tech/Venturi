use super::*;

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("tandem-setup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn an_unconfigured_runtime_is_not_ready() {
    assert!(!ready(&Runtime::default()));
}

#[test]
fn a_complete_runtime_is_ready_only_with_chromium_inside_it() {
    let root = scratch("ready");
    for package in ["renderer", "bundler"] {
        std::fs::create_dir_all(root.join("node_modules/@remotion").join(package)).unwrap();
    }
    std::fs::create_dir_all(root.join("node/bin")).unwrap();
    std::fs::write(root.join("node/bin/node"), "").unwrap();
    std::fs::create_dir_all(root.join("node_modules/.remotion")).unwrap();
    std::fs::write(root.join("node_modules/.remotion/chrome"), "").unwrap();
    let mut runtime = Runtime {
        directory: root.clone(),
        node: root.join("node/bin/node"),
        browser: root.join("node_modules/.remotion/chrome"),
    };
    assert!(ready(&runtime));
    let outside = scratch("outside");
    std::fs::write(outside.join("chrome"), "").unwrap();
    runtime.browser = outside.join("chrome");
    assert!(!ready(&runtime));
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn the_embedded_runtime_pins_the_remotion_packages() {
    for package in ["@remotion/renderer", "@remotion/bundler", "remotion"] {
        assert!(PACKAGE_JSON.contains(package), "{package}");
        assert!(
            PACKAGE_LOCK.contains(&format!("node_modules/{package}")),
            "{package}"
        );
    }
    assert!(SETUP_BROWSER.contains("Chromium executable:"));
}
