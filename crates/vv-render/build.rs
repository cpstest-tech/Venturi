//! Embeds every asset folder of `assets/<kind>/<name>/` (manifest.json +
//! shader.wgsl): adding a folder needs no Rust change.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new("assets");
    println!("cargo:rerun-if-changed=assets");
    let mut out = String::from("pub const BUILTIN: &[(&str, &str, &str)] = &[\n");
    let mut kinds: Vec<_> = std::fs::read_dir(root)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    kinds.sort();
    for kind in kinds.iter().filter(|p| p.is_dir()) {
        let mut folders: Vec<_> = std::fs::read_dir(kind)
            .map(|d| d.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        folders.sort();
        for folder in folders.iter().filter(|p| p.join("manifest.json").is_file()) {
            let abs = |name: &str| {
                std::fs::canonicalize(folder.join(name))
                    .unwrap_or_else(|_| folder.join(name))
                    .display()
                    .to_string()
            };
            let rel = format!(
                "{}/{}",
                kind.file_name().unwrap().to_string_lossy(),
                folder.file_name().unwrap().to_string_lossy()
            );
            writeln!(
                out,
                "    ({rel:?}, include_str!({:?}), include_str!({:?})),",
                abs("manifest.json"),
                abs("shader.wgsl")
            )
            .unwrap();
            println!("cargo:rerun-if-changed={}", folder.display());
        }
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("builtin_assets.rs");
    std::fs::write(dest, out).unwrap();
}
