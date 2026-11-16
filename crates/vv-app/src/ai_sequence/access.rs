#[derive(Clone, Debug, Default)]
pub enum ApiAccess {
    #[default]
    Disabled,
    FreeProject {
        key_identity: String,
    },
    ConfiguredBilling {
        key_identity: String,
    },
}

impl ApiAccess {
    pub fn check(&self, key: &str, model: &str) -> Result<(), String> {
        let expected = match self {
            Self::Disabled => return Err(t!("ai_sequence.access_disabled").into()),
            Self::FreeProject { key_identity } => {
                if crate::gemini::free_tier_info(model) != crate::gemini::FreeTierInfo::ListedFree {
                    return Err(t!("ai_sequence.free_refused").into());
                }
                key_identity
            }
            Self::ConfiguredBilling { key_identity } => key_identity,
        };
        if *expected != crate::gemini::key_identity(key) {
            return Err(t!("ai_sequence.key_changed").into());
        }
        Ok(())
    }
}
