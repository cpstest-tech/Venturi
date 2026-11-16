use super::*;

#[test]
fn model_identifiers_cannot_change_the_request_endpoint() {
    for model in ["gemini-4-hydrogen", "gemini-3.5-flash", "custom_model.1"] {
        assert!(validate_model(model).is_ok());
    }
    for model in [
        "",
        "../keys",
        "model?key=secret",
        "model:generateContent",
        "model/path",
    ] {
        assert!(validate_model(model).is_err());
    }
}

#[test]
fn cancelled_generation_never_sends_a_request() {
    let result = generate(
        &serde_json::json!({}),
        "unused",
        &AtomicBool::new(true),
        &["gemini-4-hydrogen"],
        false,
    );
    assert_eq!(result.unwrap_err(), "Cancelled");
}

#[test]
fn empty_model_list_does_not_report_an_empty_error() {
    assert!(
        generate(
            &serde_json::json!({}),
            "unused",
            &AtomicBool::new(false),
            &[],
            false
        )
        .is_err()
    );
}

#[test]
fn model_listing_filters_unsupported_methods_and_invalid_identifiers() {
    let body = serde_json::json!({"models":[
        {"name":"models/gemini-4-hydrogen","supportedGenerationMethods":["generateContent","countTokens"]},
        {"name":"models/embedding-only","supportedGenerationMethods":["embedContent"]},
        {"name":"models/../escape","supportedGenerationMethods":["generateContent"]}
    ]});
    assert_eq!(model_names(&body), vec!["gemini-4-hydrogen"]);
}

#[test]
fn free_tier_labels_distinguish_verified_models_from_unknown_or_paid_models() {
    assert_eq!(
        free_tier_info("gemini-4-hydrogen"),
        FreeTierInfo::ListedFree
    );
    assert_eq!(free_tier_info("gemini-4-carbon"), FreeTierInfo::PaidOnly);
    assert_eq!(free_tier_info("gemini-custom"), FreeTierInfo::Unknown);
    for key in [
        "ai_models.free_tier",
        "ai_models.paid_only",
        "ai_models.unknown",
    ] {
        for locale in rust_i18n::available_locales!() {
            assert!(
                crate::_rust_i18n_try_translate(&locale, key).is_some(),
                "{locale}: {key}"
            );
        }
    }
}
