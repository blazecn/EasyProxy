use app_lib::ipc;
use std::os::unix::net::UnixListener;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

struct ServiceState {
    child: Mutex<Option<Child>>,
}

fn main() {
    let _ = std::fs::remove_file(ipc::SOCKET_PATH);

    let listener = UnixListener::bind(ipc::SOCKET_PATH)
        .expect("绑定 IPC socket 失败 (需要 root)");

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

            let mut guard = state.child.lock().map_err(|_| "锁定失败")?;
            if guard.is_some() {
                serde_json::json!({"ok": true, "msg": "已在运行"})
            } else {
                let child = Command::new(binary_path)
                    .arg("-f")
                    .arg(config_path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
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
                let _ = child.wait();
            }
            serde_json::json!({"ok": true, "msg": "已停止"})
        }
        "status" => {
            let guard = state.child.lock().map_err(|_| "锁定失败")?;
            serde_json::json!({"running": guard.is_some()})
        }
        _ => serde_json::json!({"ok": false, "msg": "未知命令"}),
    };

    let resp = ipc::IpcMessage::new("response", Some(response));
    ipc::send_message(stream, &resp)?;
    Ok(())
}
