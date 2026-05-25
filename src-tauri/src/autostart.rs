use std::fs;
use std::path::PathBuf;

const SERVICE_LABEL: &str = "com.easyproxy.desktop";

fn plist_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/unknown".to_string());
    PathBuf::from(home).join(format!("Library/LaunchAgents/{SERVICE_LABEL}.plist"))
}

pub fn is_enabled() -> bool {
    plist_path().exists()
}

pub fn enable() -> Result<(), String> {
    let current_exe =
        std::env::current_exe().map_err(|e| format!("获取可执行文件路径失败: {e}"))?;
    let exe_path = current_exe
        .to_str()
        .ok_or("可执行文件路径包含非 UTF-8 字符")?;

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{SERVICE_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe_path}</string>
        <string>--hidden</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#
    );

    let path = plist_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 LaunchAgents 目录失败: {e}"))?;
    }
    fs::write(&path, plist).map_err(|e| format!("写入 LaunchAgent 配置失败: {e}"))?;

    // Load the job into launchd so it takes effect without reboot
    let output = std::process::Command::new("launchctl")
        .args(["load", "-w"])
        .arg(&path)
        .output()
        .map_err(|e| format!("执行 launchctl load 失败: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl load 失败: {stderr}"));
    }

    Ok(())
}

pub fn disable() -> Result<(), String> {
    let path = plist_path();
    if !path.exists() {
        return Ok(());
    }

    // Unload from launchd
    let _ = std::process::Command::new("launchctl")
        .args(["unload", "-w"])
        .arg(&path)
        .output();

    fs::remove_file(&path).map_err(|e| format!("删除 LaunchAgent 配置失败: {e}"))
}
