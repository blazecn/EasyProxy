use app_lib::core_manager::{CoreManager, CoreStatus};
use std::path::PathBuf;

#[test]
fn core_manager_reports_missing_binary_as_readable_error() {
    let manager = CoreManager::new(
        PathBuf::from("/missing/mihomo"),
        PathBuf::from("/tmp/config.yaml"),
    );

    let error = manager.start().expect_err("missing binary should fail");

    assert!(error.contains("Mihomo 内核不存在"));
}

#[test]
fn core_manager_starts_in_stopped_state() {
    let manager = CoreManager::new(
        PathBuf::from("/missing/mihomo"),
        PathBuf::from("/tmp/config.yaml"),
    );

    assert_eq!(manager.status(), CoreStatus::Stopped);
}
