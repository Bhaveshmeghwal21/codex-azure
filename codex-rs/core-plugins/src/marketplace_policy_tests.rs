use super::*;
use codex_config::ConfigLayerEntry;
use codex_config::ConfigLayerSource;
use codex_config::ConfigLayerStack;
use codex_config::ConfigRequirements;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::collections::HashSet;

fn stack_with_user_config(contents: &str, file: AbsolutePathBuf) -> ConfigLayerStack {
    ConfigLayerStack::new(
        vec![ConfigLayerEntry::new(
            ConfigLayerSource::User {
                file,
                profile: None,
            },
            toml::from_str(contents).expect("parse user config"),
        )],
        ConfigRequirements::default(),
        String::new(),
    )
    .expect("build config layer stack")
}

fn empty_stack() -> ConfigLayerStack {
    ConfigLayerStack::new(Vec::new(), ConfigRequirements::default(), String::new())
        .expect("build empty config layer stack")
}

#[test]
fn marketplace_policy_allows_installs_without_marketplace_requirements() {
    let policy = MarketplacePolicy::from_requirements(&ConfigRequirements::default());
    let temp = tempfile::tempdir().expect("tempdir");
    let path = AbsolutePathBuf::try_from(temp.path().join("marketplace.json")).expect("abs path");

    assert_eq!(
        policy.validate_install(
            &empty_stack(),
            temp.path(),
            &path,
            "custom-marketplace",
        ),
        Ok(())
    );
}

#[test]
fn allowed_configured_marketplace_names_returns_user_configured_names() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_file = AbsolutePathBuf::try_from(temp.path().join("config.toml")).expect("abs path");
    let stack = stack_with_user_config(
        r#"
[marketplaces.alpha]
source_type = "local"
source = "/tmp/alpha"

[marketplaces.beta]
source_type = "git"
source = "https://github.com/example/plugins.git"
"#,
        config_file,
    );

    assert_eq!(
        allowed_configured_marketplace_names(&stack, temp.path()),
        HashSet::from(["alpha".to_string(), "beta".to_string()])
    );
}
