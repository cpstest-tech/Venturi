//! Effects, filters and transitions as data: a folder with `manifest.json`
//! and `shader.wgsl`. Built-ins are embedded (`build.rs`); more can be dropped
//! in a user directory. The shader only defines `effect(uv)`, see
//! `custom_shader.rs` for the contract.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

mod builtin {
    include!(concat!(env!("OUT_DIR"), "/builtin_assets.rs"));
}

/// Parameters a shader can read (`param(0)`..`param(7)`).
pub const MAX_PARAMS: usize = 8;

/// Animation curves a shader can read (`curve(0)`..`curve(31)`).
pub const MAX_CURVES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Effect,
    Filter,
    Transition,
}

impl AssetKind {
    pub const ALL: [Self; 3] = [Self::Effect, Self::Filter, Self::Transition];

    /// Folder (under the assets root) holding this kind.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Effect => "effects",
            Self::Filter => "filters",
            Self::Transition => "transitions",
        }
    }
}

/// Text per language code; `get` falls back to English, then to any entry.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct Localized(BTreeMap<String, String>);

impl Localized {
    pub fn get(&self, lang: &str) -> &str {
        self.0
            .get(lang)
            .or_else(|| self.0.get("en"))
            .or_else(|| self.0.values().next())
            .map_or("", String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AssetParam {
    pub id: String,
    pub name: Localized,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

#[derive(Deserialize)]
struct Manifest {
    kind: AssetKind,
    name: Localized,
    #[serde(default)]
    category: String,
    #[serde(default)]
    params: Vec<AssetParam>,
    /// Transitions and effect elements: length when added.
    #[serde(default)]
    default_secs: Option<f64>,
    /// Values sampled at even steps of the progress, evaluated on the CPU
    /// once per frame instead of per pixel.
    #[serde(default)]
    curves: Vec<Vec<f32>>,
}

#[derive(Debug, Clone)]
pub struct Asset {
    /// `<kind folder>/<name>`, e.g. `transitions/wipe`: saved in projects.
    pub id: String,
    pub kind: AssetKind,
    pub name: Localized,
    pub category: String,
    pub params: Vec<AssetParam>,
    pub default_secs: f64,
    pub curves: Vec<Vec<f32>>,
    pub shader: String,
}

impl Asset {
    pub fn default_params(&self) -> Vec<f32> {
        self.params.iter().map(|p| p.default).collect()
    }

    /// `values` as the 8 shader slots: missing ones take the default, the
    /// rest is clamped to the manifest range.
    pub fn shader_params(&self, values: &[f32]) -> [f32; MAX_PARAMS] {
        let mut out = [0.0; MAX_PARAMS];
        for (i, p) in self.params.iter().enumerate().take(MAX_PARAMS) {
            out[i] = values
                .get(i)
                .copied()
                .unwrap_or(p.default)
                .clamp(p.min, p.max);
        }
        out
    }

    /// Every curve at `progress` (0..1), linearly interpolated between
    /// samples; unused slots are 0.
    pub fn curve_values(&self, progress: f32) -> [f32; MAX_CURVES] {
        let mut out = [0.0; MAX_CURVES];
        for (slot, samples) in out.iter_mut().zip(&self.curves) {
            let last = samples.len() - 1;
            let x = progress.clamp(0.0, 1.0) * last as f32;
            let i = (x.floor() as usize).min(last.saturating_sub(1));
            let next = (i + 1).min(last);
            *slot = samples[i] + (samples[next] - samples[i]) * (x - i as f32);
        }
        out
    }
}

fn parse(id: &str, manifest: &str, shader: &str) -> Result<Asset, String> {
    let m: Manifest = serde_json::from_str(manifest).map_err(|e| format!("{id}: {e}"))?;
    if m.params.len() > MAX_PARAMS {
        return Err(format!("{id}: more than {MAX_PARAMS} params"));
    }
    if m.curves.len() > MAX_CURVES {
        return Err(format!("{id}: more than {MAX_CURVES} curves"));
    }
    if m.curves.iter().any(Vec::is_empty) {
        return Err(format!("{id}: empty curve"));
    }
    if !shader.contains("fn effect") {
        return Err(format!("{id}: shader has no `effect` function"));
    }
    Ok(Asset {
        id: id.to_owned(),
        kind: m.kind,
        name: m.name,
        category: m.category,
        params: m.params,
        default_secs: m.default_secs.unwrap_or(match m.kind {
            AssetKind::Transition => 0.5,
            _ => 3.0,
        }),
        curves: m.curves,
        shader: shader.to_owned(),
    })
}

#[derive(Debug, Default)]
pub struct AssetLibrary {
    assets: Vec<Asset>,
}

impl AssetLibrary {
    /// Built-ins, then the folders of `user_dir` (same layout as `assets/`),
    /// which replace a built-in with the same id. Broken ones are reported
    /// on stderr and skipped.
    pub fn load(user_dir: Option<&Path>) -> Self {
        let mut assets: Vec<Asset> = Vec::new();
        for (id, manifest, shader) in builtin::BUILTIN {
            match parse(id, manifest, shader) {
                Ok(asset) => assets.push(asset),
                Err(e) => eprintln!("vv-render: bad built-in asset {e}"),
            }
        }
        if let Some(dir) = user_dir {
            for kind in AssetKind::ALL {
                let Ok(entries) = std::fs::read_dir(dir.join(kind.folder())) else {
                    continue;
                };
                let mut folders: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
                folders.sort();
                for folder in folders {
                    let (Ok(manifest), Ok(shader)) = (
                        std::fs::read_to_string(folder.join("manifest.json")),
                        std::fs::read_to_string(folder.join("shader.wgsl")),
                    ) else {
                        continue;
                    };
                    let name = folder.file_name().unwrap_or_default().to_string_lossy();
                    let id = format!("{}/{name}", kind.folder());
                    match parse(&id, &manifest, &shader) {
                        Ok(asset) => {
                            assets.retain(|a| a.id != asset.id);
                            assets.push(asset);
                        }
                        Err(e) => eprintln!("vv-render: bad asset {e}"),
                    }
                }
            }
        }
        Self { assets }
    }

    pub fn get(&self, id: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn of_kind(&self, kind: AssetKind) -> impl Iterator<Item = &Asset> {
        self.assets.iter().filter(move |a| a.kind == kind)
    }

    /// Categories of `kind`, in order of first appearance.
    pub fn categories(&self, kind: AssetKind) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for asset in self.of_kind(kind) {
            if !out.contains(&asset.category.as_str()) {
                out.push(&asset.category);
            }
        }
        out
    }
}

static LIBRARY: RwLock<Option<Arc<AssetLibrary>>> = RwLock::new(None);
static USER_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// The shared library, loaded on first use: preview, export and the UI all
/// read the same one.
pub fn library() -> Arc<AssetLibrary> {
    if let Some(lib) = LIBRARY.read().unwrap().as_ref() {
        return Arc::clone(lib);
    }
    reload()
}

/// Where extra asset folders are looked up (call `reload` after).
pub fn set_user_dir(dir: Option<PathBuf>) {
    *USER_DIR.write().unwrap() = dir;
}

pub fn reload() -> Arc<AssetLibrary> {
    let lib = Arc::new(AssetLibrary::load(USER_DIR.read().unwrap().as_deref()));
    *LIBRARY.write().unwrap() = Some(Arc::clone(&lib));
    lib
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
