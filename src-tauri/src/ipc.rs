use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{SystemTime, UNIX_EPOCH};

const APP_SECRET: &[u8] = b"easyproxy-ipc-key-2026";
pub const SOCKET_PATH: &str = "/var/run/easyproxy.sock";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: String,
    pub ts: u64,
    pub cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    pub sig: String,
}

impl IpcMessage {
    pub fn new(cmd: &str, payload: Option<Value>) -> Self {
        let id = random_hex(16);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let sig = sign(&id, ts, cmd, &payload);
        Self { id, ts, cmd: cmd.to_string(), payload, sig }
    }

    pub fn verify(&self) -> bool {
        let expected = sign(&self.id, self.ts, &self.cmd, &self.payload);
        expected == self.sig
    }
}

fn sign(id: &str, ts: u64, cmd: &str, payload: &Option<Value>) -> String {
    let mut mac = HmacSha256::new_from_slice(APP_SECRET).expect("HMAC key creation");
    mac.update(id.as_bytes());
    mac.update(&ts.to_le_bytes());
    mac.update(cmd.as_bytes());
    if let Some(ref p) = payload {
        mac.update(serde_json::to_string(p).unwrap_or_default().as_bytes());
    }
    hex::encode(mac.finalize().into_bytes())
}

fn random_hex(len: usize) -> String {
    let mut buf = vec![0u8; len / 2];
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = seed;
    for byte in buf.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 56) as u8;
    }
    hex::encode(buf)
}

pub fn send_message(stream: &mut UnixStream, msg: &IpcMessage) -> Result<(), String> {
    let json = serde_json::to_vec(msg).map_err(|e| format!("序列化 IPC 消息失败: {e}"))?;
    let len = json.len() as u32;
    stream.write_all(&len.to_be_bytes()).map_err(|e| format!("写入 IPC 长度失败: {e}"))?;
    stream.write_all(&json).map_err(|e| format!("写入 IPC 消息失败: {e}"))?;
    stream.flush().map_err(|e| format!("刷新 IPC 流失败: {e}"))?;
    Ok(())
}

pub fn recv_message(stream: &mut UnixStream) -> Result<IpcMessage, String> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).map_err(|e| format!("读取 IPC 长度失败: {e}"))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 65536 {
        return Err("IPC 消息过大".to_string());
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).map_err(|e| format!("读取 IPC 消息数据失败: {e}"))?;
    let msg: IpcMessage = serde_json::from_slice(&buf).map_err(|e| format!("解析 IPC 消息失败: {e}"))?;
    if !msg.verify() {
        return Err("IPC 签名验证失败".to_string());
    }
    Ok(msg)
}

pub fn connect() -> Result<UnixStream, String> {
    let mut last_err = String::new();
    for _ in 0..5 {
        match UnixStream::connect(SOCKET_PATH) {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                last_err = format!("连接 easyproxy 服务失败 (TUN 服务未安装?): {e}");
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
    Err(last_err)
}
