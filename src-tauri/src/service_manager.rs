use std::path::PathBuf;
use std::process::Command;

const SERVICE_NAME: &str = "com.easyproxy.service";
const PLIST_PATH: &str = "/Library/LaunchDaemons/com.easyproxy.service.plist";
const SERVICE_BIN_NAME: &str = "easyproxy-service";
const INSTALL_DIR: &str = "/usr/local/lib/easyproxy";

pub fn is_installed() -> bool {
    std::path::Path::new(PLIST_PATH).exists()
}

pub fn install(binary_path: &str) -> Result<(), String> {
    let install_dir = PathBuf::from(INSTALL_DIR);
    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("创建安装目录失败: {e}"))?;

    let dest = install_dir.join(SERVICE_BIN_NAME);
    std::fs::copy(binary_path, &dest)
        .map_err(|e| format!("复制 service 二进制失败: {e}"))?;

    Command::new("chmod")
        .args(["755", dest.to_str().unwrap_or("")])
        .status()
        .map_err(|e| format!("设置权限失败: {e}"))?;

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
        bin = dest.display(),
    );

    let script = format!(
        r#"do shell script "cat > {plist} << 'PLIST_EOF'
{content}
PLIST_EOF
launchctl load {plist}
mkdir -p /var/log && touch /var/log/easyproxy-service.log
" with administrator privileges"#,
        plist = PLIST_PATH,
        content = plist.replace("\\", "\\\\").replace("\"", "\\\""),
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
