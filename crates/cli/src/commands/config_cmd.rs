use crate::args::ConfigAction;
use color_eyre::eyre;
use ironpass_config::ConfigManager;

pub async fn handle(action: ConfigAction) -> eyre::Result<()> {
    let manager = ConfigManager::new();

    match action {
        ConfigAction::Show => {
            let config = manager.load_config()?;
            println!("{}", toml::to_string_pretty(&config)?);
        }
        ConfigAction::Reset => {
            let config = ironpass_config::AppConfig::default();
            manager.save_config(&config)?;
            println!("Config reset to defaults at: {}", manager.config_path().display());
        }
        ConfigAction::Set { key, value } => {
            let mut config = manager.load_config()?;
            set_config_value(&mut config, &key, &value)?;
            manager.save_config(&config)?;
            println!("Set {} = {}", key, value);
        }
        ConfigAction::Paths => {
            println!("Config dir:   {}", manager.config_dir_display());
            println!("Data dir:     {}", manager.data_dir_display());
            println!("Config file:  {}", manager.config_path().display());
            println!("Subs file:    {}", manager.subscriptions_path().display());
        }
    }

    Ok(())
}

fn set_config_value(config: &mut ironpass_config::AppConfig, key: &str, value: &str) -> eyre::Result<()> {
    match key {
        "general.user_agent" => config.general.user_agent = value.to_string(),
        "general.timeout_secs" => config.general.timeout_secs = value.parse()?,
        "general.max_retries" => config.general.max_retries = value.parse()?,
        "subscription.default_url" => config.subscription.default_url = Some(value.to_string()),
        "subscription.auto_update" => config.subscription.auto_update = value.parse()?,
        "subscription.update_interval_hours" => config.subscription.update_interval_hours = value.parse()?,
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
