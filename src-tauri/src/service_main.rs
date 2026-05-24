use app_lib::ipc;
use std::fs::OpenOptions;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

const LOG_PATH: &str = "/var/log/easyproxy-service.log";

struct ServiceState {
    child: Mutex<Option<Child>>,
}

fn main() {
    let _ = std::fs::remove_file(ipc::SOCKET_PATH);

    let listener = UnixListener::bind(ipc::SOCKET_PATH)
        .expect("绑定 IPC socket 失败 (需要 root)");

    std::fs::set_permissions(
        ipc::SOCKET_PATH,
        std::fs::Permissions::from_mode(0o666),
    )
    .ok();

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

fn handle_client(state: &ServiceState, stream: &mut std::os::unix::net::UnixStream) -> Result<(), String> {
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
                    Ok(None) => {
                        return Ok(());
                    }
                    Err(e) => eprintln!("检查 Mihomo 状态失败: {e}"),
                }
            }

            if guard.is_some() {
                serde_json::json!({"ok": true, "msg": "已在运行"})
            } else {
                let log_file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(LOG_PATH)
                    .map_err(|e| format!("打开日志文件失败: {e}"))?;

                let child = Command::new(binary_path)
                    .arg("-d")
                    .arg(working_dir)
                    .arg("-f")
                    .arg(config_path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(log_file.try_clone().map_err(|e| format!("复制文件描述符失败: {e}"))?))
                    .stderr(Stdio::from(log_file))
                    .spawn()
                    .map_err(|e| format!("启动 Mihomo 失败: {e}"))?;
                *guard = Some(child);
                serde_json::json!({"ok": true, "msg": "已启动"})
            }
        }
        "stop" => {
            let mut guard = state.child.lock().map_err(|_| "锁定失败")?;
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.try_wait();
            }
            serde_json::json!({"ok": true, "msg": "已停止"})
        }
        "status" => {
            let mut guard = state.child.lock().map_err(|_| "锁定失败")?;
            let running = match *guard {
                Some(ref mut child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        eprintln!("Mihomo 进程已退出: {status:?}");
                        *guard = None;
                        false
                    }
                    Ok(None) => true,
                    Err(_) => false,
                },
                None => false,
            };
            serde_json::json!({"running": running})
        }
        _ => serde_json::json!({"ok": false, "msg": "未知命令"}),
    };

    let resp = ipc::IpcMessage::new("response", Some(response));
    ipc::send_message(stream, &resp)?;
    Ok(())
}
