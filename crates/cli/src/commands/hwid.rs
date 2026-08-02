use crate::args::HwidAction;
use color_eyre::eyre;
use ironpass_core::traits::HwidProvider;
use ironpass_hwid::SystemHwidProvider;

pub fn handle(action: HwidAction, json: bool) -> eyre::Result<()> {
    let provider = SystemHwidProvider::new();

    match action {
        HwidAction::Show => {
            let hwid = provider.generate()?;
            if json {
                println!("{}", serde_json::json!({ "hwid": hwid }));
            } else {
                println!("HWID: {}", hwid);
            }
        }
        HwidAction::Regenerate => {
            let info = provider.get_device_info()?;
            std::fs::remove_file(provider.hwid_file())?;
            let new_hwid = provider.generate()?;
            if json {
                println!("{}", serde_json::json!({
                    "hwid": new_hwid,
                    "device_model": info.device_model,
                }));
            } else {
                println!("New HWID: {}", new_hwid);
                println!("Device:  {}", info.device_model);
            }
        }
        HwidAction::Info => {
            let info = provider.get_device_info()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!("HWID:         {}", info.hwid);
                println!("Device:       {}", info.device_model);
                println!("OS:           {}", info.os);
                println!("Hostname:     {}", info.hostname);
                println!("Username:     {}", info.username);
                println!("Machine ID:   {}", info.machine_id);
            }
        }
        HwidAction::Set { value } => {
            let info = ironpass_core::models::HwidInfo {
                hwid: value.clone(),
                device_model: "custom".into(),
                os: std::env::consts::OS.into(),
                hostname: hostname::get().map(|h| h.to_string_lossy().to_string()).unwrap_or_default(),
                username: std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .unwrap_or_else(|_| "unknown".to_string()),
                machine_id: "custom".into(),
            };
            let path = provider.hwid_file();
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, serde_json::to_string_pretty(&info)?)?;
            println!("HWID set to: {}", value);
        }
    }

    Ok(())
}
