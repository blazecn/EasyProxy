use app_lib::ipc;
use std::fs::OpenOptions;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;


struct ServiceState {
    child: Mutex<Option<Child>>,
}

fn signal_child(child: &Child, signal: &str) -> Result<(), String> {
    let status = Command::new("/bin/kill")
        .arg(signal)
        .arg(child.id().to_string())
        .status()
        .map_err(|e| format!("发送 {signal} 失败: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("发送 {signal} 失败: {status}"))
    }
}

fn wait_child_exit(child: &mut Child, attempts: usize) -> Result<bool, String> {
    for _ in 0..attempts {
        match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!("Mihomo 进程已退出: {status:?}");
                return Ok(true);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("等待 Mihomo 退出失败: {e}")),
        }
    }
    Ok(false)
}

fn stop_child_gracefully(mut child: Child) -> Result<(), String> {
    let _ = signal_child(&child, "-INT");
    if wait_child_exit(&mut child, 30)? {
        return Ok(());
    }

    let _ = signal_child(&child, "-TERM");
    if wait_child_exit(&mut child, 20)? {
        return Ok(());
    }

    child
        .kill()
        .map_err(|e| format!("强制停止 Mihomo 失败: {e}"))?;
    child
        .wait()
        .map_err(|e| format!("等待强制停止 Mihomo 失败: {e}"))?;
    Ok(())
}

fn easyproxy_mihomo_pids_for_lsof(args: &[&str]) -> Vec<String> {
    let output = Command::new("lsof").args(args).output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let mut pids = Vec::new();
    let mut current_pid = String::new();
    let mut current_command = String::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(pid) = line.strip_prefix('p') {
            current_pid = pid.to_string();
            current_command.clear();
        } else if let Some(command) = line.strip_prefix('c') {
            current_command = command.to_string();
            if current_command == "mihomo" && is_easyproxy_mihomo_pid(&current_pid) {
                pids.push(current_pid.clone());
            }
        }
    }
    pids
}

fn is_easyproxy_mihomo_pid(pid: &str) -> bool {
    if pid.is_empty() {
        return false;
    }
    let output = Command::new("lsof").args(["-p", pid, "-Fn"]).output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let files = String::from_utf8_lossy(&output.stdout);
    files.contains("/EasyProxy.app/Contents/Resources/binaries/mihomo")
        || files.contains("/com.easyproxy.desktop/")
        || files.contains("/com.easyproxy.app/")
}

fn easyproxy_mihomo_listener_pids() -> Vec<String> {
    let mut pids = Vec::new();
    pids.extend(easyproxy_mihomo_pids_for_lsof(&[
        "-nP",
        "-iTCP:7897",
        "-sTCP:LISTEN",
        "-F",
        "pc",
    ]));
    pids.extend(easyproxy_mihomo_pids_for_lsof(&[
        "-nP",
        "-iTCP:9090",
        "-sTCP:LISTEN",
        "-F",
        "pc",
    ]));
    pids.extend(easyproxy_mihomo_pids_for_lsof(&[
        "-nP",
        "-iTCP:53",
        "-sTCP:LISTEN",
        "-F",
        "pc",
    ]));
    pids.extend(easyproxy_mihomo_pids_for_lsof(&[
        "-nP", "-iUDP:53", "-F", "pc",
    ]));
    pids.sort();
    pids.dedup();
    pids
}

fn signal_pid(pid: &str, signal: &str) {
    let _ = Command::new("/bin/kill").arg(signal).arg(pid).status();
}

fn pid_still_exists(pid: &str) -> bool {
    Command::new("/bin/kill")
        .arg("-0")
        .arg(pid)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn kill_pid_gracefully(pid: &str) {
    signal_pid(pid, "-INT");
    for _ in 0..30 {
        if !pid_still_exists(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    signal_pid(pid, "-TERM");
    for _ in 0..20 {
        if !pid_still_exists(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    signal_pid(pid, "-KILL");
}

fn reclaim_orphan_mihomo_listeners(skip_pid: Option<u32>) {
    for pid in easyproxy_mihomo_listener_pids() {
        if skip_pid
            .map(|tracked| pid == tracked.to_string())
            .unwrap_or(false)
        {
            continue;
        }
        eprintln!("清理残留 Mihomo 进程: {pid}");
        kill_pid_gracefully(&pid);
    }
}

fn tracked_child_running(guard: &mut Option<Child>) -> bool {
    match guard {
        Some(child) => match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!("Mihomo 进程已退出: {status:?}");
                *guard = None;
                false
            }
            Ok(None) => true,
            Err(e) => {
                eprintln!("检查 Mihomo 状态失败: {e}");
                false
            }
        },
        None => false,
    }
}

fn main() {
    let _ = std::fs::remove_file(ipc::SOCKET_PATH);

    let listener = UnixListener::bind(ipc::SOCKET_PATH).expect("绑定 IPC socket 失败 (需要 root)");

    std::fs::set_permissions(ipc::SOCKET_PATH, std::fs::Permissions::from_mode(0o666)).ok();

    let state = ServiceState {
        child: Mutex::new(None),
    };

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(e) = handle_client(&state, &mut stream) {
                    eprintln!("IPC 错误: {e}");
                }
            }
            Err(e) => eprintln!("连接错误: {e}"),
        }
    }
}

fn handle_client(
    state: &ServiceState,
    stream: &mut std::os::unix::net::UnixStream,
) -> Result<(), String> {
    let msg = ipc::recv_message(stream)?;

    let response = match msg.cmd.as_str() {
        "start" => {
            let payload = msg.payload.as_ref().ok_or("start 命令缺少参数")?;
            let binary_path = payload["binary_path"].as_str().ok_or("缺少 binary_path")?;
            let config_path = payload["config_path"].as_str().ok_or("缺少 config_path")?;
            let working_dir = payload["working_dir"].as_str().ok_or("缺少 working_dir")?;

            let mut guard = state.child.lock().map_err(|_| "锁定失败")?;

            // Clean up dead child
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        eprintln!("Mihomo 进程已退出: {status:?}");
                        *guard = None;
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("检查 Mihomo 状态失败: {e}"),
                }
            }

            if guard.is_some() {
                serde_json::json!({"ok": true, "msg": "已在运行"})
            } else {
                let core_log_path = std::path::Path::new(working_dir).join("mihomo.log");
                let log_file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&core_log_path)
                    .map_err(|e| format!("打开 Mihomo 日志文件失败: {e}"))?;

                let child = Command::new(binary_path)
                    .arg("-d")
                    .arg(working_dir)
                    .arg("-f")
                    .arg(config_path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(
                        log_file
                            .try_clone()
                            .map_err(|e| format!("复制文件描述符失败: {e}"))?,
                    ))
                    .stderr(Stdio::from(log_file))
                    .spawn()
                    .map_err(|e| format!("启动 Mihomo 失败: {e}"))?;
                *guard = Some(child);
                serde_json::json!({"ok": true, "msg": "已启动"})
            }
        }
        "stop" => {
            let force = msg
                .payload
                .as_ref()
                .and_then(|p| p.get("force"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mut guard = state.child.lock().map_err(|_| "锁定失败")?;
            if let Some(mut child) = guard.take() {
                let tracked_pid = child.id();
                match child.try_wait() {
                    Ok(Some(status)) => {
                        eprintln!("Mihomo 进程已退出: {status:?}");
                    }
                    Ok(None) => {
                        if force {
                            child.kill().map_err(|e| format!("强制停止失败: {e}"))?;
                            child.wait().map_err(|e| format!("等待退出失败: {e}"))?;
                        } else {
                            stop_child_gracefully(child)?;
                        }
                    }
                    Err(e) => return Err(format!("检查 Mihomo 状态失败: {e}")),
                }
                reclaim_orphan_mihomo_listeners(Some(tracked_pid));
            } else {
                reclaim_orphan_mihomo_listeners(None);
            }
            serde_json::json!({"ok": true, "msg": "已停止"})
        }
        "status" => {
            let mut guard = state.child.lock().map_err(|_| "锁定失败")?;
            let running = tracked_child_running(&mut guard);
            serde_json::json!({"running": running})
        }
        _ => serde_json::json!({"ok": false, "msg": "未知命令"}),
    };

    let resp = ipc::IpcMessage::new("response", Some(response));
    ipc::send_message(stream, &resp)?;
    Ok(())
}
