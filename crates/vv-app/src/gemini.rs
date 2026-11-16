use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) fn key_path() -> Result<PathBuf, String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or("Cannot locate the user configuration directory")?;
    Ok(config.join("tandem").join("gemini.key"))
}

pub(crate) fn load_key() -> Result<String, String> {
    load_key_at(&key_path()?)
}

pub(crate) fn load_key_at(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(key) => Ok(key.trim().to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn save_key(key: &str) -> Result<(), String> {
    save_key_at(&key_path()?, key)
}

pub(crate) fn save_key_at(path: &Path, key: &str) -> Result<(), String> {
    static NEXT_KEY_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|error| error.to_string())?;
    let temp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT_KEY_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|error| error.to_string())?;
    std::io::Write::write_all(&mut file, key.trim().as_bytes())
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    std::fs::rename(temp, path).map_err(|error| error.to_string())
}

pub(crate) fn delete_key() -> Result<(), String> {
    match std::fs::remove_file(key_path()?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn cancellable_sleep(cancel: &AtomicBool, duration: std::time::Duration) -> Result<(), String> {
    let until = std::time::Instant::now() + duration;
    while std::time::Instant::now() < until {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

pub(crate) fn is_transient(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// Gemini's own message, e.g. "The model is overloaded", instead of the bare status.
pub(crate) fn api_error(status: u16, body: &[u8]) -> String {
    let message = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message")?.as_str().map(str::to_owned));
    match message {
        Some(message) => format!("Gemini error {status}: {message}"),
        None => format!("Gemini error {status}"),
    }
}

/// Callers choose the exact model order and whether quota failures may be retried.
pub(crate) fn generate_with_timeout(
    request: &serde_json::Value,
    key: &str,
    cancel: &AtomicBool,
    models: &[&str],
    retry_quota: bool,
    timeout: std::time::Duration,
) -> Result<serde_json::Value, String> {
    if key.trim().is_empty() {
        return Err("Gemini API key is missing".into());
    }
    if models.is_empty() {
        return Err("No Gemini model selected".into());
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .build()
        .into();
    let payload = request.to_string();
    let mut last_error = String::new();
    for model in models {
        validate_model(model)?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
        );
        for attempt in 0..4 {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let mut response = match agent
                .post(&url)
                .header("x-goog-api-key", key)
                .header("content-type", "application/json")
                .send(payload.as_bytes())
            {
                Ok(response) => response,
                Err(error) => {
                    last_error = error.to_string();
                    cancellable_sleep(cancel, std::time::Duration::from_secs(2 << attempt))?;
                    continue;
                }
            };
            let status = response.status().as_u16();
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
            let body = response
                .body_mut()
                .with_config()
                .limit(64 * 1024 * 1024)
                .read_to_vec()
                .map_err(|e| e.to_string())?;
            if status == 200 {
                return serde_json::from_slice(&body).map_err(|e| e.to_string());
            }
            last_error = api_error(status, &body).replace(key, "[redacted]");
            if !is_transient(status) || (status == 429 && !retry_quota) {
                return Err(last_error);
            }
            let wait = retry_after.unwrap_or(2 << attempt).min(30);
            cancellable_sleep(cancel, std::time::Duration::from_secs(wait))?;
        }
    }
    Err(last_error)
}

pub(crate) fn generate(
    request: &serde_json::Value,
    key: &str,
    cancel: &AtomicBool,
    models: &[&str],
    retry_quota: bool,
) -> Result<serde_json::Value, String> {
    generate_with_timeout(
        request,
        key,
        cancel,
        models,
        retry_quota,
        std::time::Duration::from_secs(300),
    )
}

pub(crate) fn validate_model(model: &str) -> Result<(), String> {
    if model.is_empty()
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return Err("Invalid Gemini model identifier".into());
    }
    Ok(())
}

pub(crate) fn configured_key() -> Result<String, String> {
    let key = load_key()?;
    if !key.trim().is_empty() {
        return Ok(key);
    }
    std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .ok_or("Configure the shared Gemini key in Settings, or set GEMINI_API_KEY".into())
}

pub(crate) fn key_identity(key: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, key.trim().as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn list_models() -> Result<Vec<String>, String> {
    let key = configured_key()?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .into();
    let mut token = String::new();
    let mut names = Vec::new();
    for _ in 0..20 {
        let mut response = agent
            .get("https://generativelanguage.googleapis.com/v1beta/models")
            .query("pageSize", "100")
            .query("pageToken", &token)
            .header("x-goog-api-key", &key)
            .call()
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let bytes = response
            .body_mut()
            .with_config()
            .limit(4 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| e.to_string())?;
        if status != 200 {
            return Err(api_error(status, &bytes).replace(&key, "[redacted]"));
        }
        let body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        names.extend(model_names(&body));
        token = body["nextPageToken"].as_str().unwrap_or("").to_owned();
        if token.is_empty() {
            names.sort();
            names.dedup();
            return Ok(names);
        }
    }
    Err("Gemini model listing exceeded 20 pages".into())
}

fn model_names(body: &serde_json::Value) -> Vec<String> {
    body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|model| {
            model["supportedGenerationMethods"]
                .as_array()
                .is_some_and(|methods| methods.iter().any(|method| method == "generateContent"))
        })
        .filter_map(|model| {
            model["name"]
                .as_str()?
                .strip_prefix("models/")
                .filter(|name| validate_model(name).is_ok())
                .map(str::to_owned)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FreeTierInfo {
    ListedFree,
    PaidOnly,
    Unknown,
}
pub(crate) const PRICING_CHECKED_ON: &str = "2026-11-16";

pub(crate) fn free_tier_info(model: &str) -> FreeTierInfo {
    match model {
        "gemini-4-hydrogen" => FreeTierInfo::ListedFree,
        "gemini-4-carbon" | "gemini-4-argon" => FreeTierInfo::PaidOnly,
        _ => FreeTierInfo::Unknown,
    }
}

pub(crate) fn free_tier_label(model: &str) -> std::borrow::Cow<'static, str> {
    match free_tier_info(model) {
        FreeTierInfo::ListedFree => t!("ai_models.free_tier", date = PRICING_CHECKED_ON),
        FreeTierInfo::PaidOnly => t!("ai_models.paid_only", date = PRICING_CHECKED_ON),
        FreeTierInfo::Unknown => t!("ai_models.unknown"),
    }
}

#[cfg(test)]
#[path = "tests/gemini.rs"]
mod tests;
