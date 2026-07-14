use codex_config::ConfigLayerStack;
use codex_config::ConfigRequirements;
use std::collections::HashSet;
use std::path::Path;

/// Compatibility policy for marketplace installation.
///
/// This fork's `ConfigRequirements` does not model marketplace source
/// restrictions, so marketplace installs retain the existing unrestricted
/// behavior. Keep this type narrow so callers can stay policy-aware without
/// pretending unsupported requirements exist.
pub(crate) struct MarketplacePolicy;

impl MarketplacePolicy {
    pub(crate) fn from_requirements(_requirements: &ConfigRequirements) -> Self {
        Self
    }

    pub(crate) fn validate_install(
        &self,
        _config_layer_stack: &ConfigLayerStack,
        _codex_home: &Path,
        _marketplace_path: &codex_utils_absolute_path::AbsolutePathBuf,
        _marketplace_name: &str,
    ) -> Result<(), String> {
        Ok(())
    }
}

pub fn allowed_configured_marketplace_names(
    config_layer_stack: &ConfigLayerStack,
    _codex_home: &Path,
) -> HashSet<String> {
    config_layer_stack
        .effective_user_config()
        .and_then(|config| {
            config
                .get("marketplaces")
                .and_then(toml::Value::as_table)
                .map(|marketplaces| marketplaces.keys().cloned().collect())
        })
        .unwrap_or_default()
}
