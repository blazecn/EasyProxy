use std::process::Command;

const SERVICE_NAME: &str = "com.easyproxy.service";
const PLIST_PATH: &str = "/Library/LaunchDaemons/com.easyproxy.service.plist";
const SERVICE_BIN_NAME: &str = "easyproxy-service";
const INSTALL_DIR: &str = "/usr/local/lib/easyproxy";

pub fn is_installed() -> bool {
    std::path::Path::new(PLIST_PATH).exists()
}

pub fn is_service_loaded() -> bool {
    let output = Command::new("launchctl")
        .arg("list")
        .arg(SERVICE_NAME)
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

pub fn needs_update(_bundled_binary: &str) -> bool {
    let dest = format!("{}/{}", INSTALL_DIR, SERVICE_BIN_NAME);
    if !std::path::Path::new(&dest).exists() || !std::path::Path::new(PLIST_PATH).exists() {
        return true;
    }
    // Only require reinstall if service binary or plist is missing.
    // Skip MD5 comparison to avoid password prompt on every rebuild during development.
    // Once installed, launchd KeepAlive keeps the service running across reboots.
    !is_service_loaded()
}

pub fn install(binary_path: &str) -> Result<(), String> {
    let install_dir = INSTALL_DIR;
    let dest = &format!("{}/{}", install_dir, SERVICE_BIN_NAME);

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{name}</string>
    <key>Program</key>
    <string>{bin}</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>/var/log/easyproxy-service.log</string>
    <key>StandardOutPath</key>
    <string>/var/log/easyproxy-service.log</string>
</dict>
</plist>"#,
        name = SERVICE_NAME,
        bin = dest,
    );

    let escaped_plist = plist
        .replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("$", "\\$")
        .replace("`", "\\`");

    let script = format!(
        r#"do shell script "launchctl unload {plist_path} 2>/dev/null
mkdir -p {install_dir} && cp {src} {dest} && chmod 755 {dest}
cat > {plist_path} << 'PLIST_EOF'
{content}
PLIST_EOF
launchctl load {plist_path}
mkdir -p /var/log && touch /var/log/easyproxy-service.log
" with administrator privileges"#,
        install_dir = install_dir,
        src = shell_escape(binary_path),
        dest = dest,
        plist_path = PLIST_PATH,
        content = escaped_plist,
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("执行安装脚本失败: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("cancel") || stderr.contains("User cancelled") {
            Err("用户取消了授权".to_string())
        } else {
            Err(format!("安装服务失败: {}", stderr.trim()))
        }
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace("'", "'\\''"))
}

pub fn uninstall() -> Result<(), String> {
    let script = format!(
        r#"do shell script "launchctl unload {plist} 2>/dev/null; rm -f {plist}" with administrator privileges"#,
        plist = PLIST_PATH,
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("执行卸载脚本失败: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("卸载服务失败: {}", stderr.trim()))
    }
}
