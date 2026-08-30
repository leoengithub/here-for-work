use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use serde_json::{Value, json};

const MAX_MESSAGE_BYTES: usize = 1_048_576;

fn main() {
    let origin = env::args().nth(1).unwrap_or_default();
    if !valid_origin(&origin) {
        return;
    }
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    while let Ok(message) = read_framed_message(&mut stdin) {
        let response = forward(&origin, message)
            .unwrap_or_else(|error| json!({ "protocolVersion": 1, "ok": false, "error": error }));
        if write_framed_message(&mut stdout, &response).is_err() {
            break;
        }
    }
}

fn valid_origin(origin: &str) -> bool {
    let Some(id) = origin
        .strip_prefix("chrome-extension://")
        .and_then(|value| value.strip_suffix('/'))
    else {
        return false;
    };
    id.len() == 32 && id.bytes().all(|byte| matches!(byte, b'a'..=b'p'))
}

fn read_framed_message(reader: &mut impl Read) -> Result<Value, String> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|error| error.to_string())?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_MESSAGE_BYTES {
        return Err("invalid_message_length".to_string());
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&payload).map_err(|_| "invalid_json".to_string())
}

fn write_framed_message(writer: &mut impl Write, message: &Value) -> Result<(), String> {
    let payload = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    let length = u32::try_from(payload.len()).map_err(|_| "response_too_large".to_string())?;
    writer
        .write_all(&length.to_le_bytes())
        .and_then(|_| writer.write_all(&payload))
        .and_then(|_| writer.flush())
        .map_err(|error| error.to_string())
}

fn forward(origin: &str, message: Value) -> Result<Value, String> {
    let home = env::var_os("HOME").ok_or("home_unavailable")?;
    let socket = PathBuf::from(home)
        .join("Library/Application Support/com.hereforwork.desktop/browser-bridge.sock");
    forward_to_socket(origin, message, &socket)
}

fn forward_to_socket(
    origin: &str,
    mut message: Value,
    socket: &std::path::Path,
) -> Result<Value, String> {
    if !valid_origin(origin) {
        return Err("invalid_origin".to_string());
    }
    if message.get("protocolVersion").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported_protocol".to_string());
    }
    let extension_id = origin
        .trim_start_matches("chrome-extension://")
        .trim_end_matches('/');
    if let Some(object) = message.as_object_mut() {
        object.insert(
            "extensionId".to_string(),
            Value::String(extension_id.to_string()),
        );
    }
    let mut stream = UnixStream::connect(socket).map_err(|_| "app_not_running".to_string())?;
    let payload = serde_json::to_vec(&message).map_err(|error| error.to_string())?;
    stream
        .write_all(&payload)
        .map_err(|error| error.to_string())?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream
        .take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut response)
        .map_err(|error| error.to_string())?;
    if response.len() > MAX_MESSAGE_BYTES {
        return Err("response_too_large".to_string());
    }
    serde_json::from_slice(&response).map_err(|_| "invalid_app_response".to_string())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Write};
    use std::os::unix::net::UnixListener;

    use serde_json::json;

    #[test]
    fn origin_must_be_an_exact_chrome_extension() {
        assert!(super::valid_origin(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/"
        ));
        assert!(!super::valid_origin("https://example.com/"));
        assert!(!super::valid_origin("chrome-extension://too-short/"));
    }

    #[test]
    fn native_message_frame_round_trips() {
        let message = json!({ "protocolVersion": 1, "type": "hello" });
        let mut bytes = Vec::new();
        super::write_framed_message(&mut bytes, &message).unwrap();

        let decoded = super::read_framed_message(&mut Cursor::new(bytes)).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn native_message_frame_rejects_oversized_payloads_before_allocating() {
        let mut bytes = ((super::MAX_MESSAGE_BYTES as u32) + 1)
            .to_le_bytes()
            .to_vec();
        bytes.extend_from_slice(b"{}");

        let error = super::read_framed_message(&mut Cursor::new(bytes)).unwrap_err();

        assert_eq!(error, "invalid_message_length");
    }

    #[test]
    fn forwarding_injects_the_origin_extension_id() {
        let directory = tempfile::tempdir().unwrap();
        let socket_path = directory.path().join("bridge.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut payload = Vec::new();
            stream.read_to_end(&mut payload).unwrap();
            let message: serde_json::Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(
                message
                    .get("extensionId")
                    .and_then(serde_json::Value::as_str),
                Some("abcdefghijklmnopabcdefghijklmnop")
            );
            stream
                .write_all(br#"{"protocolVersion":1,"ok":true,"type":"hello_ack"}"#)
                .unwrap();
        });

        let response = super::forward_to_socket(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
            json!({ "protocolVersion": 1, "type": "hello", "extensionId": "spoofed" }),
            &socket_path,
        )
        .unwrap();

        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        server.join().unwrap();
    }
}
