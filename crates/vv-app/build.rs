use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

// ggml-metal uses `@available`, whose helper lives in clang's runtime
// library and is not linked by default.
fn link_clang_runtime() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Some(dir) = Command::new("clang")
        .arg("--print-resource-dir")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    else {
        return;
    };
    println!("cargo:rustc-link-search={dir}/lib/darwin");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}

fn main() {
    link_clang_runtime();
    let version = git(&["describe", "--tags", "--exact-match", "HEAD"])
        .or_else(|| git(&["rev-parse", "--short", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=VV_VERSION={version}");

    // Without these the build script doesn't rerun on every commit and the version goes stale.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        let git_dir = Path::new(&git_dir);
        println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join("packed-refs").display()
        );
        // A missing path would make cargo rerun this on every build.
        let tags_dir = git_dir.join("refs/tags");
        if tags_dir.exists() {
            println!("cargo:rerun-if-changed={}", tags_dir.display());
        }
        if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
            println!(
                "cargo:rerun-if-changed={}",
                git_dir.join(head_ref).display()
            );
        }
    }
}
