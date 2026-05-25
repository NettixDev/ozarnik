use std::collections::HashSet;

const FALLBACK_MODEL_METADATA_WARNING_PREFIX: &str = "Model metadata for `";
const FALLBACK_MODEL_METADATA_WARNING_SUFFIX: &str =
    "` not found. Defaulting to fallback metadata; this can degrade performance and cause issues.";

#[derive(Default)]
pub(super) struct WarningDisplayState {
    fallback_model_metadata_slugs: HashSet<String>,
}

impl WarningDisplayState {
    pub(super) fn should_display(&mut self, message: &str) -> bool {
        // SQAgent/ОЗАРНИК: suppress the fallback-model-metadata warning entirely.
        // OnlySQ models are fetched live and never appear in the (now-empty) bundled
        // catalog, so this warning would fire on every turn and is just noise.
        if let Some(slug) = fallback_model_metadata_warning_slug(message) {
            let _ = self.fallback_model_metadata_slugs.insert(slug.to_string());
            return false;
        }
        true
    }
}

fn fallback_model_metadata_warning_slug(message: &str) -> Option<&str> {
    message
        .strip_prefix(FALLBACK_MODEL_METADATA_WARNING_PREFIX)?
        .strip_suffix(FALLBACK_MODEL_METADATA_WARNING_SUFFIX)
}
