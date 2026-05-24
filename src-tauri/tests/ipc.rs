use app_lib::ipc::IpcMessage;
use serde_json::json;

#[test]
fn valid_message_passes_verification() {
    let msg = IpcMessage::new("start", Some(json!({"key": "value"})));
    assert!(msg.verify());
}

#[test]
fn valid_message_without_payload_passes_verification() {
    let msg = IpcMessage::new("stop", None);
    assert!(msg.verify());
}

#[test]
fn tampered_cmd_fails_verification() {
    let mut msg = IpcMessage::new("start", None);
    msg.cmd = "stop".to_string();
    assert!(!msg.verify());
}

#[test]
fn tampered_payload_fails_verification() {
    let mut msg = IpcMessage::new("start", Some(json!({"key": "value"})));
    msg.payload = Some(json!({"key": "tampered"}));
    assert!(!msg.verify());
}

#[test]
fn tampered_id_fails_verification() {
    let mut msg = IpcMessage::new("start", None);
    msg.id = "deadbeef".to_string();
    assert!(!msg.verify());
}

#[test]
fn tampered_ts_fails_verification() {
    let mut msg = IpcMessage::new("start", None);
    msg.ts = 0;
    assert!(!msg.verify());
}

#[test]
fn forged_signature_fails_verification() {
    let mut msg = IpcMessage::new("start", None);
    msg.sig = "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    assert!(!msg.verify());
}

#[test]
fn sign_produces_different_sigs_for_different_messages() {
    let a = IpcMessage::new("start", None);
    let b = IpcMessage::new("stop", None);
    // Wait a tiny bit to ensure different timestamps
    std::thread::sleep(std::time::Duration::from_millis(5));
    let c = IpcMessage::new("start", None);
    assert_ne!(a.sig, b.sig);
    assert_ne!(a.sig, c.sig);
}
