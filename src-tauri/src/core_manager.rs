use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreStatus {
    Stopped,
    Running,
}

#[derive(Clone)]
pub struct CoreManager {
    binary_path: PathBuf,
    config_path: PathBuf,
    child: Arc<Mutex<Option<Child>>>,
}

impl CoreManager {
    pub fn new(binary_path: PathBuf, config_path: PathBuf) -> Self {
        Self {
            binary_path,
            config_path,
            child: Arc::new(Mutex::new(None)),
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

        let log_path = self
            .config_path
            .parent()
            .map(|p| p.join("mihomo.log"))
            .unwrap_or_else(|| PathBuf::from("mihomo.log"));
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("打开 Mihomo 日志文件失败: {e}"))?;

        let mut command = Command::new(&self.binary_path);
        command.arg("-f").arg(&self.config_path);
        if let Some(working_dir) = self.config_path.parent() {
            command.arg("-d").arg(working_dir);
        }
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                log_file
                    .try_clone()
                    .map_err(|e| format!("复制文件描述符失败: {e}"))?,
            ))
            .stderr(Stdio::from(log_file))
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
        let Ok(mut child_guard) = self.child.lock() else {
            return CoreStatus::Stopped;
        };

        if let Some(ref mut child) = *child_guard {
            match child.try_wait() {
                Ok(Some(_)) => {
                    *child_guard = None;
                    CoreStatus::Stopped
                }
                Ok(None) => CoreStatus::Running,
                Err(_) => CoreStatus::Stopped,
            }
        } else {
            CoreStatus::Stopped
        }
    }
}

impl Drop for CoreManager {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
