use super::{Job, Runtime};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

const DRIVER: &str = r#"
const {bundle} = require('/runtime/node_modules/@remotion/bundler');
const {selectComposition, renderMedia} = require('/runtime/node_modules/@remotion/renderer');
(async () => {
  const serveUrl = await bundle({entryPoint:'/work/index.tsx', publicDir: null});
  const browserExecutable = process.argv[2];
  const composition = await selectComposition({serveUrl, id:'Sequence', browserExecutable, logLevel:'verbose'});
  if (process.argv[3] === 'validate') return;
  const transparent = process.argv[3] === 'transparent';
  await renderMedia({serveUrl, composition, browserExecutable,
    codec: transparent ? 'prores' : 'h264',
    ...(transparent ? {proResProfile:'4444', pixelFormat:'yuva444p10le', imageFormat:'png'} : {}),
    outputLocation:transparent ? '/work/result.mov' : '/work/result.mp4', concurrency:1});
})().catch(e => {
  const message=e.stack || String(e);
  const environment=/ENOSPC|EROFS|EACCES|ENOMEM|Failed to launch the browser|browser executable|spawn.*ENOENT/i.test(message);
  console.error((environment ? 'TANDEM_RENDER_ENVIRONMENT_ERROR: ' : 'TANDEM_COMPOSITION_ERROR: ')+message);
  process.exit(1);
});
"#;

pub fn prepare(job: &Job, directory: &Path) -> Result<(), String> {
    discard_partial(directory);
    let config = &job.request.composition;
    config.validate()?;
    std::fs::write(
        directory.join("Scene.tsx"),
        job.source.as_deref().ok_or("Missing source")?,
    )
    .map_err(|e| e.to_string())?;
    let entry = format!(
        "import React from 'react'; import {{registerRoot, Composition}} from 'remotion'; import Scene from './Scene'; registerRoot(() => <Composition id=\"Sequence\" component={{Scene}} width={{{}}} height={{{}}} fps={{{}}} durationInFrames={{{}}}/>);",
        config.width, config.height, config.fps, config.frames
    );
    std::fs::write(directory.join("index.tsx"), entry).map_err(|e| e.to_string())?;
    std::fs::write(directory.join("driver.cjs"), DRIVER).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(directory.join("node_modules")).map_err(|e| e.to_string())
}

pub fn run(
    runtime: &Runtime,
    job: &Job,
    directory: &Path,
    validate: bool,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if !cfg!(target_os = "linux") {
        return Err("Isolated Remotion rendering currently requires Linux and bubblewrap".into());
    }
    let root = runtime
        .directory
        .canonicalize()
        .map_err(|e| format!("Configure a local Remotion runtime: {e}"))?;
    let node = runtime
        .node
        .canonicalize()
        .map_err(|e| format!("Configure the Node executable: {e}"))?;
    let browser = runtime
        .browser
        .canonicalize()
        .map_err(|e| format!("Configure the Chromium executable: {e}"))?;
    let relative_browser = browser
        .strip_prefix(&root)
        .map_err(|_| "Chromium must be installed inside the Remotion runtime directory")?;
    if !root.join("node_modules/@remotion/renderer").is_dir()
        || !root.join("node_modules/@remotion/bundler").is_dir()
    {
        return Err(
            "Install @remotion/renderer and @remotion/bundler in the configured runtime".into(),
        );
    }
    let mut command = Command::new("bwrap");
    command.env_clear().args([
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
    ]);
    for path in ["/usr", "/lib", "/lib64", "/etc/fonts", "/etc/ld.so.cache"] {
        if Path::new(path).exists() {
            command.args(["--ro-bind", path, path]);
        }
    }
    command
        .arg("--ro-bind")
        .arg(&root)
        .arg("/runtime")
        .arg("--ro-bind")
        .arg(node)
        .arg("/node")
        .arg("--bind")
        .arg(directory.canonicalize().map_err(|e| e.to_string())?)
        .arg("/work")
        .arg("--ro-bind")
        .arg(root.join("node_modules"))
        .arg("/work/node_modules")
        .args([
            "--chdir",
            "/work",
            "--setenv",
            "HOME",
            "/tmp",
            "--setenv",
            "PATH",
            "/usr/bin:/bin",
            "--",
            "/node",
            "--max-old-space-size=1024",
            "/work/driver.cjs",
        ])
        .arg(Path::new("/runtime").join(relative_browser))
        .arg(if validate {
            "validate"
        } else if job.request.composition.transparent {
            "transparent"
        } else {
            "opaque"
        });
    super::process::run(
        &mut command,
        cancel,
        Duration::from_secs(if validate {
            job.request.limits.render_timeout_secs.min(120)
        } else {
            job.request.limits.render_timeout_secs
        }),
    )?;
    let output = directory.join(if job.request.composition.transparent {
        "result.mov"
    } else {
        "result.mp4"
    });
    if !validate && !output.is_file() {
        return Err("Renderer did not produce a media file".into());
    }
    Ok(output)
}

pub fn preflight(runtime: &Runtime, cancel: &AtomicBool) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("Isolated rendering currently requires Linux and bubblewrap".into());
    }
    let root = runtime
        .directory
        .canonicalize()
        .map_err(|_| "Configure the local Remotion runtime before generating")?;
    if !runtime.node.is_file()
        || !runtime.browser.is_file()
        || !runtime
            .browser
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&root)
        || !root.join("node_modules/@remotion/renderer").is_dir()
        || !root.join("node_modules/@remotion/bundler").is_dir()
    {
        return Err("Configure Node, Chromium inside the runtime, and the installed Remotion packages before generating".into());
    }
    super::process::run(
        Command::new("bwrap").args([
            "--unshare-all",
            "--die-with-parent",
            "--ro-bind",
            "/",
            "/",
            "--",
            "/usr/bin/true",
        ]),
        cancel,
        Duration::from_secs(10),
    )
    .map_err(|e| format!("Local rendering isolation is unavailable: {e}"))?;
    Ok(())
}

pub fn repairable(error: &str) -> bool {
    error.contains("TANDEM_COMPOSITION_ERROR:")
}

pub fn discard_partial(directory: &Path) {
    for name in ["result.mp4", "result.mov"] {
        let path = directory.join(name);
        if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
            let _ = std::fs::remove_file(path);
        }
    }
}
