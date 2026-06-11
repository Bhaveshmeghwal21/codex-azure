use codex_app_server_protocol::ConfigEdit;
use serde_json::json;

use crate::config_update::clear_config_value;
use crate::config_update::replace_config_value;
use crate::legacy_core::config::Config;

pub(crate) const AZURE_USAGE: &str = "Usage: /azure list | /azure add <id> --base-url <url> --api-version <version> --key <key> [--model <deployment>] [--context-window <tokens>] [--use] | /azure use <id> [--model <deployment>] | /azure remove <id>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AzureCommand {
    List,
    Add(AzureAddArgs),
    Use(AzureUseArgs),
    Remove { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AzureAddArgs {
    pub(crate) id: String,
    pub(crate) base_url: String,
    pub(crate) api_version: String,
    pub(crate) key: String,
    pub(crate) model: Option<String>,
    pub(crate) context_window: Option<i64>,
    pub(crate) use_provider: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AzureUseArgs {
    pub(crate) id: String,
    pub(crate) model: Option<String>,
}

pub(crate) struct AzureWriteRequest {
    pub(crate) edits: Vec<ConfigEdit>,
    pub(crate) success_message: String,
}

pub(crate) fn parse_azure_command(input: &str) -> Result<AzureCommand, String> {
    let tokens = split_args(input)?;
    let Some((verb, rest)) = tokens.split_first() else {
        return Err(AZURE_USAGE.to_string());
    };
    match verb.as_str() {
        "list" => Ok(AzureCommand::List),
        "add" => parse_add(rest),
        "use" => parse_use(rest),
        "remove" | "rm" => parse_remove(rest),
        _ => Err(AZURE_USAGE.to_string()),
    }
}

pub(crate) fn build_write_request(
    command: AzureCommand,
    config: &Config,
) -> Result<AzureWriteRequest, String> {
    match command {
        AzureCommand::List => Err(AZURE_USAGE.to_string()),
        AzureCommand::Add(args) => Ok(build_add_request(args)),
        AzureCommand::Use(args) => Ok(build_use_request(args, config)),
        AzureCommand::Remove { id } => build_remove_request(id, config),
    }
}

pub(crate) fn list_providers(config: &Config) -> String {
    let mut rows = config
        .model_providers
        .iter()
        .filter(|(_, provider)| {
            provider
                .base_url
                .as_deref()
                .is_some_and(|url| url.contains(".openai.azure.com") || url.contains("/openai"))
        })
        .map(|(id, provider)| {
            let active = if id == &config.model_provider_id {
                " active"
            } else {
                ""
            };
            let base_url = provider.base_url.as_deref().unwrap_or("-");
            format!("{id}{active}: {base_url}")
        })
        .collect::<Vec<_>>();
    rows.sort();
    if rows.is_empty() {
        "No Azure providers configured.".to_string()
    } else {
        rows.join("\n")
    }
}

fn parse_add(tokens: &[String]) -> Result<AzureCommand, String> {
    let Some((id, rest)) = tokens.split_first() else {
        return Err(AZURE_USAGE.to_string());
    };
    validate_provider_id(id)?;
    let mut base_url = None;
    let mut api_version = None;
    let mut key = None;
    let mut model = None;
    let mut context_window = Some(1_050_000);
    let mut use_provider = false;
    let mut idx = 0;
    while idx < rest.len() {
        match rest[idx].as_str() {
            "--base-url" => {
                base_url = Some(require_value(rest, &mut idx, "--base-url")?);
            }
            "--api-version" => {
                api_version = Some(require_value(rest, &mut idx, "--api-version")?);
            }
            "--key" => {
                key = Some(require_value(rest, &mut idx, "--key")?);
            }
            "--model" => {
                model = Some(require_value(rest, &mut idx, "--model")?);
            }
            "--context-window" => {
                let value = require_value(rest, &mut idx, "--context-window")?;
                context_window = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| "--context-window must be an integer".to_string())?,
                );
            }
            "--no-context-window" => {
                context_window = None;
                idx += 1;
            }
            "--use" => {
                use_provider = true;
                idx += 1;
            }
            _ => return Err(AZURE_USAGE.to_string()),
        }
    }
    Ok(AzureCommand::Add(AzureAddArgs {
        id: id.to_string(),
        base_url: base_url.ok_or_else(|| "Missing --base-url".to_string())?,
        api_version: api_version.ok_or_else(|| "Missing --api-version".to_string())?,
        key: key.ok_or_else(|| "Missing --key".to_string())?,
        model,
        context_window,
        use_provider,
    }))
}

fn parse_use(tokens: &[String]) -> Result<AzureCommand, String> {
    let Some((id, rest)) = tokens.split_first() else {
        return Err(AZURE_USAGE.to_string());
    };
    validate_provider_id(id)?;
    let mut model = None;
    let mut idx = 0;
    while idx < rest.len() {
        match rest[idx].as_str() {
            "--model" => {
                model = Some(require_value(rest, &mut idx, "--model")?);
            }
            _ => return Err(AZURE_USAGE.to_string()),
        }
    }
    Ok(AzureCommand::Use(AzureUseArgs {
        id: id.to_string(),
        model,
    }))
}

fn parse_remove(tokens: &[String]) -> Result<AzureCommand, String> {
    match tokens {
        [id] => {
            validate_provider_id(id)?;
            Ok(AzureCommand::Remove { id: id.to_string() })
        }
        _ => Err(AZURE_USAGE.to_string()),
    }
}

fn build_add_request(args: AzureAddArgs) -> AzureWriteRequest {
    let mut edits = vec![
        replace_config_value(
            format!("model_providers.{}.name", args.id),
            json!(args.id.clone()),
        ),
        replace_config_value(
            format!("model_providers.{}.base_url", args.id),
            json!(args.base_url),
        ),
        replace_config_value(
            format!("model_providers.{}.experimental_bearer_token", args.id),
            json!(args.key),
        ),
        replace_config_value(
            format!("model_providers.{}.query_params.\"api-version\"", args.id),
            json!(args.api_version),
        ),
    ];
    if let Some(context_window) = args.context_window {
        edits.push(replace_config_value(
            format!("model_providers.{}.model_context_window", args.id),
            json!(context_window),
        ));
    }
    if args.use_provider {
        edits.push(replace_config_value("model_provider", json!(args.id)));
    }
    if let Some(model) = args.model {
        edits.push(replace_config_value("model", json!(model)));
    }
    AzureWriteRequest {
        edits,
        success_message: if args.use_provider {
            format!("Azure provider `{}` added and selected.", args.id)
        } else {
            format!("Azure provider `{}` added.", args.id)
        },
    }
}

fn build_use_request(args: AzureUseArgs, config: &Config) -> AzureWriteRequest {
    let mut edits = vec![replace_config_value("model_provider", json!(args.id))];
    if let Some(model) = args.model {
        edits.push(replace_config_value("model", json!(model)));
    }
    let current_model = config
        .model
        .clone()
        .unwrap_or_else(|| "current model".to_string());
    AzureWriteRequest {
        edits,
        success_message: format!("Azure provider `{}` selected for {current_model}.", args.id),
    }
}

fn build_remove_request(id: String, config: &Config) -> Result<AzureWriteRequest, String> {
    if id == config.model_provider_id {
        return Err(format!(
            "Provider `{id}` is active. Run `/azure use <other-id>` before removing it."
        ));
    }
    Ok(AzureWriteRequest {
        edits: vec![clear_config_value(format!("model_providers.{id}"))],
        success_message: format!("Azure provider `{id}` removed."),
    })
}

fn require_value(tokens: &[String], idx: &mut usize, flag: &str) -> Result<String, String> {
    let value_index = *idx + 1;
    let Some(value) = tokens.get(value_index) else {
        return Err(format!("Missing value for {flag}"));
    };
    if value.starts_with("--") {
        return Err(format!("Missing value for {flag}"));
    }
    *idx += 2;
    Ok(value.clone())
}

fn validate_provider_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err("Provider id may contain only letters, numbers, `_`, and `-`.".to_string());
    }
    Ok(())
}

fn split_args(input: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars();
    let mut quote = None;
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
            ch if ch.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if quote.is_some() {
        return Err("Unterminated quote in /azure command.".to_string());
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parse_add_accepts_required_azure_fields() {
        let command = parse_azure_command(
            "add azure2 --base-url https://example.openai.azure.com/openai --api-version 2025-04-01-preview --key secret --model gpt-5.5 --use",
        )
        .expect("parse add");

        assert_eq!(
            command,
            AzureCommand::Add(AzureAddArgs {
                id: "azure2".to_string(),
                base_url: "https://example.openai.azure.com/openai".to_string(),
                api_version: "2025-04-01-preview".to_string(),
                key: "secret".to_string(),
                model: Some("gpt-5.5".to_string()),
                context_window: Some(1_050_000),
                use_provider: true,
            })
        );
    }

    #[test]
    fn build_add_request_writes_provider_and_api_version() {
        let request = build_add_request(AzureAddArgs {
            id: "azure2".to_string(),
            base_url: "https://example.openai.azure.com/openai".to_string(),
            api_version: "2025-04-01-preview".to_string(),
            key: "secret".to_string(),
            model: Some("gpt-5.5".to_string()),
            context_window: Some(1_050_000),
            use_provider: true,
        });

        let key_paths = request
            .edits
            .iter()
            .map(|edit| edit.key_path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            key_paths,
            vec![
                "model_providers.azure2.name",
                "model_providers.azure2.base_url",
                "model_providers.azure2.experimental_bearer_token",
                "model_providers.azure2.query_params.\"api-version\"",
                "model_providers.azure2.model_context_window",
                "model_provider",
                "model",
            ]
        );
    }
}
