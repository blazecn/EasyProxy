use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreStatus {
    Stopped,
    Running,
}

pub struct CoreManager {
    binary_path: PathBuf,
    config_path: PathBuf,
    child: Mutex<Option<Child>>,
}

impl CoreManager {
    pub fn new(binary_path: PathBuf, config_path: PathBuf) -> Self {
        Self {
            binary_path,
            config_path,
            child: Mutex::new(None),
        }
    }

    pub fn start(&self) -> Result<CoreStatus, String> {
        if !self.binary_path.exists() {
            return Err(format!("Mihomo 内核不存在: {}", self.binary_path.display()));
        }

        if !self.config_path.exists() {
            return Err(format!("Mihomo 配置不存在: {}", self.config_path.display()));
        }

        let mut child_guard = self
            .child
            .lock()
            .map_err(|_| "Mihomo 进程状态锁定失败".to_string())?;
        if child_guard.is_some() {
            return Ok(CoreStatus::Running);
        }

        let child = Command::new(&self.binary_path)
            .arg("-f")
            .arg(&self.config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("启动 Mihomo 失败: {error}"))?;

        *child_guard = Some(child);
        Ok(CoreStatus::Running)
    }

    pub fn stop(&self) -> Result<CoreStatus, String> {
        let mut child_guard = self
            .child
            .lock()
            .map_err(|_| "Mihomo 进程状态锁定失败".to_string())?;

        if let Some(mut child) = child_guard.take() {
            child
                .kill()
                .map_err(|error| format!("停止 Mihomo 失败: {error}"))?;
            let _ = child.wait();
        }

        Ok(CoreStatus::Stopped)
    }

    pub fn status(&self) -> CoreStatus {
        let Ok(child_guard) = self.child.lock() else {
            return CoreStatus::Stopped;
        };

        if child_guard.is_some() {
            CoreStatus::Running
        } else {
            CoreStatus::Stopped
        }
    }
}
