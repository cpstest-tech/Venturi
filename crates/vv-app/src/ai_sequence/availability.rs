use super::{Provider, process};
use std::sync::{atomic::AtomicBool, mpsc};
use std::time::Duration;

pub struct Probe {
    rx: mpsc::Receiver<Vec<(Provider, String)>>,
}

impl Probe {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut statuses = Vec::new();
            for provider in Provider::ALL {
                let status = if let Some(executable) = provider.executable() {
                    match process::run(
                        process::program(executable).arg("--version"),
                        &AtomicBool::new(false),
                        Duration::from_secs(5),
                    ) {
                        Ok(version) => format!(
                            "Installed: {}. Authentication is checked when a request starts.",
                            version.trim().chars().take(150).collect::<String>()
                        ),
                        Err(error) => format!(
                            "Unavailable: {}",
                            error.chars().take(300).collect::<String>()
                        ),
                    }
                } else if crate::gemini::load_key().is_ok_and(|key| !key.is_empty())
                    || std::env::var("GEMINI_API_KEY").is_ok_and(|key| !key.trim().is_empty())
                {
                    "Shared Gemini key configured. Validity and quota are checked on request."
                        .into()
                } else {
                    "Gemini key missing. Configure the shared Viral Clips key below.".into()
                };
                statuses.push((provider, status));
            }
            let _ = tx.send(statuses);
        });
        Self { rx }
    }

    pub fn poll(&self) -> Option<Vec<(Provider, String)>> {
        self.rx.try_recv().ok()
    }
}
