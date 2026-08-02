use crate::api_client::ApiClient;
use crate::args::HwidAction;
use color_eyre::eyre;

pub async fn handle(api_url: &str, action: HwidAction, json: bool) -> eyre::Result<()> {
    let client = ApiClient::with_url(api_url.into());

    match action {
        HwidAction::Show => {
            let resp = client.get_hwid().await?;
            if json {
                println!("{}", serde_json::json!({ "hwid": resp.hwid }));
            } else {
                println!("HWID: {}", resp.hwid);
            }
        }
        HwidAction::Regenerate => {
            let resp = client.regenerate_hwid().await?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "hwid": resp.hwid,
                        "device_model": resp.info.device_model,
                    })
                );
            } else {
                println!("New HWID: {}", resp.hwid);
                println!("Device:  {}", resp.info.device_model);
            }
        }
        HwidAction::Info => {
            let resp = client.get_hwid().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&resp.info)?);
            } else {
                println!("HWID:         {}", resp.info.hwid);
                println!("Device:       {}", resp.info.device_model);
                println!("OS:           {}", resp.info.os);
                println!("Hostname:     {}", resp.info.hostname);
                println!("Username:     {}", resp.info.username);
                println!("Machine ID:   {}", resp.info.machine_id);
            }
        }
        HwidAction::Set { value } => {
            let info = ironpass_core::models::HwidInfo {
                hwid: value.clone(),
                device_model: "custom".into(),
                os: std::env::consts::OS.into(),
                hostname: hostname::get()
                    .map(|h| h.to_string_lossy().to_string())
                    .unwrap_or_default(),
                username: std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .unwrap_or_else(|_| "unknown".to_string()),
                machine_id: "custom".into(),
            };
            let provider = ironpass_hwid::SystemHwidProvider::new();
            let path = provider.hwid_file();
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, serde_json::to_string_pretty(&info)?)?;
            println!("HWID set to: {}", value);
        }
    }

    Ok(())
}
