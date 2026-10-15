// SPDX-License-Identifier: AGPL-3.0-only

//! The MCP harness smoke (design §8.2/Q19, chunk plan §8): boots the
//! real app binary headless-with-renderer (`SLINT_BACKEND=headless-
//! software`) with the dev-gated `taskboard-ui-slint/mcp` feature
//! compiled in, and speaks the embedded MCP server's HTTP JSON-RPC
//! surface with a hand-rolled minimal client — handshake, window
//! listing, a real screenshot, changes-after-interaction, clean
//! exit. Guards the POST-1.0 LLM-debugging lane from rotting.
//!
//! CI invokes it explicitly (the `ui-mcp-smoke` job):
//! ```sh
//! cargo test -p taskboard-app --features taskboard-ui-slint/mcp \
//!   --test mcp_smoke -- --ignored --nocapture
//! ```
//!
//! No display needed — that is the point of the headless backend.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::Value;

/// The boot/interaction deadline (AGENTS §5: poll a predicate with a
/// deadline; the waits below are bounded polls, never fixed sleeps).
const DEADLINE: Duration = Duration::from_secs(30);

#[test]
#[ignore = "CI-invoked (ui-mcp-smoke job): needs the mcp feature compiled in"]
fn the_mcp_lane_boots_screenshots_and_exits_clean() {
    let temp = tempfile::tempdir().expect("temp dir");
    let config_path = temp.path().join("taskboard.toml");
    std::fs::write(
        &config_path,
        format!(
            "[app]\nmode = 'desktop'\n\n[storage]\ndb_path = '{}'\n",
            temp.path().join("taskboard.db").display()
        ),
    )
    .expect("writes config");
    let stderr_path = std::path::PathBuf::from("/tmp/mcp-smoke-stderr.log");

    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_taskboard"))
        .env("TASKBOARD_CONFIG", &config_path)
        .env("SLINT_BACKEND", "headless-software")
        .env("SLINT_EMIT_DEBUG_INFO", "1")
        .env("SLINT_MCP_PORT", port.to_string())
        .stderr(std::fs::File::create(&stderr_path).expect("stderr file"))
        .stdout(Stdio::null())
        .spawn()
        .expect("the app binary spawns");

    // The smoke body owns a guard that signals and reaps the child on
    // the way out, success or panic.
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
        smoke(&mut child, port);
    }));
    let _ = signal_and_wait(&mut child);
    if let Err(panic) = outcome {
        // Surface the app's stderr (boot failures log there), then
        // resume the original panic payload.
        let log = std::fs::read_to_string(&stderr_path).unwrap_or_default();
        eprintln!("MCP smoke failed; app stderr:\n{log}");
        std::panic::resume_unwind(panic);
    }
}

fn smoke(child: &mut Child, port: u16) {
    // Boot-wait: poll the MCP port with a bounded deadline.
    let deadline = Instant::now() + DEADLINE;
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the MCP server never opened its port"
        );
        assert!(
            child.try_wait().expect("child status").is_none(),
            "the app exited before the MCP server came up"
        );
        std::thread::yield_now();
    }

    // 1. The initialize handshake succeeds.
    let response = post(
        port,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    let response: Value = serde_json::from_str(&response).expect("initialize is JSON");
    assert_eq!(
        response["result"]["serverInfo"]["name"], "slint-mcp-embedded",
        "the embedded MCP server answers the handshake"
    );

    // 2. list_windows returns at least one window (the app mounted).
    let response = post(
        port,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_windows","arguments":{}}}"#,
    );
    let handle = first_window_handle(&response);
    assert!(
        handle.is_some(),
        "the mounted app exposes a window: {response}"
    );
    let handle = handle.expect("checked above");

    // 3. take_screenshot returns a decodable, correctly-sized PNG
    //    (the headless-software backend's real rasterizer).
    let before = screenshot(port, &handle);

    // 4. dispatch_key_event opens the menu; the screen changes.
    for (text, event_type) in [
        ("\u{11}", "Press"), // Control
        ("\u{10}", "Press"), // Shift
        // The chord's key arrives case-carrying (the keymap spells
        // `Ctrl+Shift+M`; printables deliver the cased character).
        ("M", "PressAndRelease"),
        ("\u{10}", "Release"),
        ("\u{11}", "Release"),
    ] {
        let arguments = serde_json::json!({
            "windowHandle": handle,
            "text": text,
            "eventType": event_type,
        });
        let request = format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"dispatch_key_event","arguments":{arguments}}}}}"#
        );
        let response = post(port, &request);
        assert!(
            !response.contains("\"isError\":true"),
            "the key event dispatched: {response}"
        );
    }
    let after = screenshot(port, &handle);
    assert_ne!(before, after, "the screenshot changes after interaction");

    // 5. The exit path: signal_and_wait ran by the guard.
}

fn first_window_handle(response: &str) -> Option<Value> {
    let response: Value = serde_json::from_str(response).ok()?;
    let text = response["result"]["content"][0]["text"].as_str()?;
    let listed: Value = serde_json::from_str(text).ok()?;
    listed["windowHandles"].as_array()?.first().cloned()
}

fn screenshot(port: u16, handle: &Value) -> Vec<u8> {
    let arguments = serde_json::json!({ "windowHandle": handle });
    let request = format!(
        r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"take_screenshot","arguments":{arguments}}}}}"#
    );
    let response = post(port, &request);
    let response: Value = serde_json::from_str(&response).expect("screenshot is JSON");
    assert!(
        response["result"]["content"][0]["type"] == "image",
        "the screenshot came back as an image block: {response}"
    );
    let b64 = response["result"]["content"][0]["data"]
        .as_str()
        .expect("image data");
    let png = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("base64 decodes");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "a real PNG");
    let decoder = png::Decoder::new(std::io::Cursor::new(&png));
    let reader = decoder.read_info().expect("PNG header decodes");
    assert!(
        reader.info().width > 0 && reader.info().height > 0,
        "non-empty"
    );
    png
}

/// One POST to the MCP server, response body as a string. The
/// server's own HTTP layer is the framing authority (Content-Length;
/// `Connection: close` so the read ends with the response).
fn post(port: u16, body: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("the MCP server is up");
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .expect("request written");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("response read");
    raw.split_once("\r\n\r\n")
        .expect("HTTP framing")
        .1
        .to_owned()
}

/// SIGTERM, then a bounded wait for the exit (AGENTS §5: a deadline
/// poll, never a fixed sleep).
fn signal_and_wait(child: &mut Child) -> Option<std::process::ExitStatus> {
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid;
    let _ = signal::kill(Pid::from_raw(child.id().cast_signed()), Signal::SIGTERM);
    let deadline = Instant::now() + DEADLINE;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the app never exited after SIGTERM");
        }
        std::thread::yield_now();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("binds")
        .local_addr()
        .expect("addr")
        .port()
}

// The imports above must stay minimal; this file is a smoke, not a
// driver (the backlog entry owns the POST-1.0 wrapper).
use std::panic::AssertUnwindSafe;
