//! Integration test: Rust SDK -> MCP Server -> Worker -> Fake Device.
//!
//! Run with:
//!   cargo run --example test_fake_device -- --api-url http://localhost:3199 --api-key pk_test123 --device-id faketest001

use screenmcp::{ClientOptions, ScreenMCPClient, ScreenMCPError, ScrollDirection};
use std::time::Duration;

fn get_arg(args: &[String], name: &str, default: &str) -> String {
    let flag = format!("--{}", name);
    args.iter()
        .position(|a| a == &flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

struct TestResults {
    passed: Vec<String>,
    failed: Vec<(String, String)>,
}

impl TestResults {
    fn new() -> Self {
        Self {
            passed: vec![],
            failed: vec![],
        }
    }

    fn pass(&mut self, name: &str) {
        println!("  PASS  {}", name);
        self.passed.push(name.to_string());
    }

    fn fail(&mut self, name: &str, reason: &str) {
        eprintln!("  FAIL  {}: {}", name, reason);
        self.failed.push((name.to_string(), reason.to_string()));
    }

    fn summary(&self) -> i32 {
        let total = self.passed.len() + self.failed.len();
        println!();
        println!("{}", "=".repeat(60));
        print!("Test Results: {}/{} passed", self.passed.len(), total);
        if !self.failed.is_empty() {
            print!(", {} FAILED", self.failed.len());
        }
        println!();
        if !self.failed.is_empty() {
            println!("\nFailures:");
            for (name, reason) in &self.failed {
                println!("  - {}: {}", name, reason);
            }
        }
        println!("{}", "=".repeat(60));
        if self.failed.is_empty() { 0 } else { 1 }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let api_url = get_arg(&args, "api-url", "http://localhost:3000");
    let api_key = get_arg(&args, "api-key", "pk_test123");
    let device_id = get_arg(&args, "device-id", "faketest001");

    println!("{}", "=".repeat(60));
    println!("ScreenMCP Rust SDK Integration Test");
    println!("  API URL:    {}", api_url);
    println!("  API Key:    {}", api_key);
    println!("  Device ID:  {}", device_id);
    println!("{}", "=".repeat(60));

    let mut results = TestResults::new();

    let client = ScreenMCPClient::new(ClientOptions {
        api_key: api_key.clone(),
        api_url: Some(api_url.clone()),
        command_timeout_ms: Some(10_000),
        auto_reconnect: Some(false),
    });

    // list_devices
    match client.list_devices().await {
        Ok(devices) => results.pass(&format!("list_devices() -> {} devices", devices.len())),
        Err(e) => results.fail("list_devices", &e.to_string()),
    }

    // Connect
    let mut phone = match client.connect(&device_id).await {
        Ok(conn) => {
            let phone_connected = conn.phone_connected().await;
            results.pass(&format!(
                "connect (worker={}, phone={})",
                conn.worker_url().unwrap_or("?"),
                phone_connected
            ));
            conn
        }
        Err(e) => {
            results.fail("connect", &format!("{}", e));
            std::process::exit(results.summary());
        }
    };

    // Wait for phone to connect
    if !phone.phone_connected().await {
        println!("  Waiting for phone to connect...");
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if phone.phone_connected().await {
                break;
            }
        }
        if phone.phone_connected().await {
            results.pass("phone connected");
        } else {
            results.fail("phone_connect", "Phone did not connect within 15s");
            let _ = phone.disconnect().await;
            std::process::exit(results.summary());
        }
    }

    // screenshot
    match phone.screenshot().await {
        Ok(r) => {
            if r.image.is_empty() {
                results.fail("screenshot", "No image data");
            } else {
                use base64_decode;
                let bytes = base64_decode(&r.image);
                let is_png = bytes.len() >= 4 && bytes[0] == 0x89 && bytes[1] == 0x50;
                results.pass(&format!("screenshot ({} bytes, PNG={})", bytes.len(), is_png));
            }
        }
        Err(e) => results.fail("screenshot", &e.to_string()),
    }

    // click
    match phone.click(540, 960).await {
        Ok(()) => results.pass("click(540, 960)"),
        Err(e) => results.fail("click", &e.to_string()),
    }

    // long_click
    match phone.long_click(100, 200).await {
        Ok(()) => results.pass("long_click(100, 200)"),
        Err(e) => results.fail("long_click", &e.to_string()),
    }

    // type_text
    match phone.type_text("hello world").await {
        Ok(()) => results.pass("type_text('hello world')"),
        Err(e) => results.fail("type_text", &e.to_string()),
    }

    // ui_tree
    match phone.ui_tree().await {
        Ok(r) => {
            if r.tree.is_empty() {
                results.fail("ui_tree", "Empty tree");
            } else {
                let root_class = r.tree[0]
                    .get("className")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                results.pass(&format!("ui_tree (root className={})", root_class));
            }
        }
        Err(e) => results.fail("ui_tree", &e.to_string()),
    }

    // back
    match phone.back().await {
        Ok(()) => results.pass("back()"),
        Err(e) => results.fail("back", &e.to_string()),
    }

    // home
    match phone.home().await {
        Ok(()) => results.pass("home()"),
        Err(e) => results.fail("home", &e.to_string()),
    }

    // recents
    match phone.recents().await {
        Ok(()) => results.pass("recents()"),
        Err(e) => results.fail("recents", &e.to_string()),
    }

    // scroll
    match phone.scroll(ScrollDirection::Down, Some(500)).await {
        Ok(()) => results.pass("scroll(Down, 500)"),
        Err(e) => results.fail("scroll", &e.to_string()),
    }

    // get_text
    match phone.get_text().await {
        Ok(r) => results.pass(&format!("get_text() -> '{}'", r.text)),
        Err(e) => results.fail("get_text", &e.to_string()),
    }

    // copy
    match phone.copy().await {
        Ok(_) => results.pass("copy()"),
        Err(e) => results.fail("copy", &e.to_string()),
    }

    // get_clipboard
    match phone.get_clipboard().await {
        Ok(r) => results.pass(&format!("get_clipboard() -> '{}'", r.text)),
        Err(e) => results.fail("get_clipboard", &e.to_string()),
    }

    // set_clipboard
    match phone.set_clipboard("test content").await {
        Ok(()) => results.pass("set_clipboard('test content')"),
        Err(e) => results.fail("set_clipboard", &e.to_string()),
    }

    // paste
    match phone.paste(None).await {
        Ok(()) => results.pass("paste()"),
        Err(e) => results.fail("paste", &e.to_string()),
    }

    // select_all
    match phone.select_all().await {
        Ok(()) => results.pass("select_all()"),
        Err(e) => results.fail("select_all", &e.to_string()),
    }

    // drag
    match phone.drag(100, 200, 500, 600).await {
        Ok(()) => results.pass("drag(100, 200, 500, 600)"),
        Err(e) => results.fail("drag", &e.to_string()),
    }

    // list_cameras
    match phone.list_cameras().await {
        Ok(r) => results.pass(&format!("list_cameras() -> {} cameras", r.cameras.len())),
        Err(e) => results.fail("list_cameras", &e.to_string()),
    }

    // camera
    match phone.camera(Some("0")).await {
        Ok(r) => results.pass(&format!("camera('0') -> {} base64 chars", r.image.len())),
        Err(e) => results.fail("camera", &e.to_string()),
    }

    // press_key
    match phone.press_key("Enter").await {
        Ok(()) => results.pass("press_key('Enter')"),
        Err(e) => results.fail("press_key", &e.to_string()),
    }

    // hold_key + release_key
    match phone.hold_key("Shift").await {
        Ok(()) => match phone.release_key("Shift").await {
            Ok(()) => results.pass("hold_key('Shift') + release_key('Shift')"),
            Err(e) => results.fail("release_key", &e.to_string()),
        },
        Err(e) => results.fail("hold_key", &e.to_string()),
    }

    // unknown command should return error
    match phone.send_command("totally_fake_command_xyz", None).await {
        Ok(_) => results.fail("unknown_command", "Expected error but got success"),
        Err(ScreenMCPError::Command(msg)) => {
            results.pass(&format!("unknown command raises error: {}", msg));
        }
        Err(e) => results.fail("unknown_command", &format!("Wrong error type: {}", e)),
    }

    let _ = phone.disconnect().await;

    let exit_code = results.summary();
    std::process::exit(exit_code);
}

fn base64_decode(input: &str) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in input.as_bytes() {
        let val = if b == b'=' {
            continue;
        } else if let Some(pos) = TABLE.iter().position(|&c| c == b) {
            pos as u32
        } else {
            continue;
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    out
}
