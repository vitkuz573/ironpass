use ironpass_core::{Error, Result, models::HwidInfo};
use ironpass_core::traits::HwidProvider;
use sha2::{Sha256, Digest};
use std::fs;
use std::path::PathBuf;

pub struct SystemHwidProvider {
    storage_path: PathBuf,
}

impl SystemHwidProvider {
    pub fn new() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ironpass");

        Self {
            storage_path: config_dir,
        }
    }

    pub fn hwid_file(&self) -> PathBuf {
        self.storage_path.join("hwid.json")
    }

    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.storage_path)?;
        Ok(())
    }

    fn load_or_generate(&self) -> Result<HwidInfo> {
        if let Ok(data) = fs::read_to_string(self.hwid_file()) {
            if let Ok(info) = serde_json::from_str::<HwidInfo>(&data) {
                return Ok(info);
            }
        }

        let info = self.generate_new()?;
        self.save(&info)?;
        Ok(info)
    }

    fn generate_new(&self) -> Result<HwidInfo> {
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();

        let username = whoami_username();

        let machine_id = machine_uid::get()
            .map_err(|e| Error::Hwid(format!("Failed to get machine UID: {}", e)))?;

        let device_model = detect_device_model();
        let os = detect_os();

        let raw = format!("{}:{}:{}:{}", hostname, username, machine_id, device_model);
        let hwid = format!("{:x}", Sha256::digest(raw.as_bytes()));

        Ok(HwidInfo {
            hwid,
            device_model,
            os,
            hostname,
            username,
            machine_id,
        })
    }

    fn save(&self, info: &HwidInfo) -> Result<()> {
        self.ensure_dir()?;
        let json = serde_json::to_string_pretty(info)?;
        fs::write(self.hwid_file(), json)?;
        Ok(())
    }

    pub fn device_model_string(&self) -> String {
        let info = self.load_or_generate().ok();
        match info {
            Some(i) => format!("{}/{}", i.device_model, i.os),
            None => "Unknown".into(),
        }
    }
}

impl Default for SystemHwidProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HwidProvider for SystemHwidProvider {
    fn generate(&self) -> Result<String> {
        let info = self.load_or_generate()?;
        Ok(info.hwid)
    }

    fn get_device_info(&self) -> Result<HwidInfo> {
        self.load_or_generate()
    }
}

fn detect_os() -> String {
    #[cfg(target_os = "linux")]
    {
        let kernel = std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            let pretty = content.lines()
                .find(|l| l.starts_with("PRETTY_NAME="))
                .and_then(|l| l.splitn(2, '=').nth(1))
                .map(|s| s.trim_matches('"').to_string());
            if let Some(name) = pretty {
                if kernel.is_empty() {
                    return name;
                }
                return format!("{} ({})", name, kernel);
            }
        }
        if kernel.is_empty() {
            "Linux".into()
        } else {
            format!("Linux ({})", kernel)
        }
    }

    #[cfg(target_os = "macos")]
    {
        let version = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown".into());
        format!("macOS {}", version)
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Windows".into())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "Unknown".into()
    }
}

fn detect_device_model() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = fs::read_to_string("/sys/class/dmi/id/product_name") {
            let model = content.trim().to_string();
            if !model.is_empty() && model != "System Product Name" {
                return model;
            }
        }
        if let Ok(content) = fs::read_to_string("/sys/class/dmi/id/sys_vendor") {
            let vendor = content.trim().to_string();
            if !vendor.is_empty() {
                return vendor;
            }
        }
        gethostname::gethostname().to_string_lossy().to_string()
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sysctl")
            .args(["-n", "hw.model"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Mac".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("wmic")
            .args(["csproduct", "get", "name"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .lines()
            .nth(1)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Windows".to_string())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "Unknown".to_string()
    }
}

fn whoami_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
