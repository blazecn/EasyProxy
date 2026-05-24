use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyMode {
    Rule,
    Global,
    Direct,
}

impl ProxyMode {
    pub fn as_mihomo_mode(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Global => "global",
            Self::Direct => "direct",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemProxy {
    host: String,
    port: u16,
}

impl SystemProxy {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn is_enabled(&self) -> bool {
        is_platform_proxy_enabled()
    }

    pub fn enable(&self) -> Result<(), String> {
        enable_platform_proxy(&self.host, self.port)
    }

    pub fn disable(&self) -> Result<(), String> {
        disable_platform_proxy()
    }
}

#[cfg(target_os = "macos")]
fn is_platform_proxy_enabled() -> bool {
    network_services()
        .ok()
        .and_then(|services| services.into_iter().next())
        .map(|service| {
            Command::new("networksetup")
                .args(["-getwebproxy", &service])
                .output()
                .ok()
                .map(|output| {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    stdout.lines().any(|line| line.trim() == "Enabled: Yes")
                })
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn enable_platform_proxy(host: &str, port: u16) -> Result<(), String> {
    for service in network_services()? {
        run_networksetup(&["-setwebproxy", &service, host, &port.to_string()])?;
        run_networksetup(&["-setsecurewebproxy", &service, host, &port.to_string()])?;
        run_networksetup(&["-setsocksfirewallproxy", &service, host, &port.to_string()])?;
        run_networksetup(&["-setwebproxystate", &service, "on"])?;
        run_networksetup(&["-setsecurewebproxystate", &service, "on"])?;
        run_networksetup(&["-setsocksfirewallproxystate", &service, "on"])?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn disable_platform_proxy() -> Result<(), String> {
    for service in network_services()? {
        run_networksetup(&["-setwebproxystate", &service, "off"])?;
        run_networksetup(&["-setsecurewebproxystate", &service, "off"])?;
        run_networksetup(&["-setsocksfirewallproxystate", &service, "off"])?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn network_services() -> Result<Vec<String>, String> {
    let output = Command::new("networksetup")
        .arg("-listnetworkserviceorder")
        .output()
        .map_err(|error| format!("读取 macOS 网络服务失败: {error}"))?;

    if !output.status.success() {
        return Err("读取 macOS 网络服务失败".to_string());
    }

    let services = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            // Lines with service name start with "(N) " pattern
            if trimmed.starts_with('(') {
                trimmed.splitn(2, ')')
                    .nth(1)
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    Ok(services)
}

#[cfg(target_os = "macos")]
fn run_networksetup(args: &[&str]) -> Result<(), String> {
    let status = Command::new("networksetup")
        .args(args)
        .status()
        .map_err(|error| format!("执行 networksetup 失败: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("更新 macOS 系统代理失败".to_string())
    }
}

#[cfg(target_os = "windows")]
fn enable_platform_proxy(host: &str, port: u16) -> Result<(), String> {
    let endpoint = format!("{host}:{port}");
    let status = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyServer",
            "/d",
            &endpoint,
            "/f",
        ])
        .status()
        .map_err(|error| format!("写入 Windows 代理地址失败: {error}"))?;

    if !status.success() {
        return Err("写入 Windows 代理地址失败".to_string());
    }

    set_windows_proxy_enabled(true)
}

#[cfg(target_os = "windows")]
fn disable_platform_proxy() -> Result<(), String> {
    set_windows_proxy_enabled(false)
}

#[cfg(target_os = "windows")]
fn set_windows_proxy_enabled(enabled: bool) -> Result<(), String> {
    let value = if enabled { "1" } else { "0" };
    let status = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            value,
            "/f",
        ])
        .status()
        .map_err(|error| format!("更新 Windows 系统代理失败: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("更新 Windows 系统代理失败".to_string())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn is_platform_proxy_enabled() -> bool {
    false
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn enable_platform_proxy(_host: &str, _port: u16) -> Result<(), String> {
    Err("当前平台暂不支持自动设置系统代理".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn disable_platform_proxy() -> Result<(), String> {
    Err("当前平台暂不支持自动关闭系统代理".to_string())
}
