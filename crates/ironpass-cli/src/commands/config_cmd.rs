use crate::args::ConfigAction;
use color_eyre::eyre;
use ironpass_api_client::ApiClient;
use ironpass_config::AppConfig;

pub async fn handle(api_url: &str, action: ConfigAction) -> eyre::Result<()> {
    let client = ApiClient::with_url(api_url.into());

    match action {
        ConfigAction::Show => {
            let config = client.get_config().await?;
            println!("{}", toml::to_string_pretty(&config)?);
        }
        ConfigAction::Reset => {
            let config = AppConfig::default();
            client.put_config(&config).await?;
            println!("Config reset to defaults");
        }
        ConfigAction::Set { key, value } => {
            let mut config = client.get_config().await?;
            set_config_value(&mut config, &key, &value)?;
            client.put_config(&config).await?;
            println!("Set {} = {}", key, value);
        }
        ConfigAction::Paths => {
            println!("API URL:  {}", api_url);
            println!("Daemon:   ironpassd");
        }
    }

    Ok(())
}

fn set_config_value(config: &mut AppConfig, key: &str, value: &str) -> eyre::Result<()> {
    match key {
        "general.user_agent" => config.general.user_agent = value.to_string(),
        "general.timeout_secs" => config.general.timeout_secs = value.parse()?,
        "general.max_retries" => config.general.max_retries = value.parse()?,
        "subscription.default_url" => config.subscription.default_url = Some(value.to_string()),
        "subscription.auto_update" => config.subscription.auto_update = value.parse()?,
        "subscription.update_interval_hours" => {
            config.subscription.update_interval_hours = value.parse()?
        }
        "subscription.proxy" => config.subscription.proxy = Some(value.to_string()),
        "hwid.enabled" => config.hwid.enabled = value.parse()?,
        "hwid.custom_id" => config.hwid.custom_id = Some(value.to_string()),
        "hwid.device_model_override" => config.hwid.device_model_override = Some(value.to_string()),
        "output.format" => config.output.format = value.to_string(),
        "output.pretty" => config.output.pretty = value.parse()?,
        "logging.level" => config.logging.level = value.to_string(),
        "logging.file" => config.logging.file = value.parse()?,
        _ => return Err(eyre::eyre!("Unknown config key: {}", key)),
    }
    Ok(())
}
