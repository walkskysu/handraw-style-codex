use std::{
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};
use tauri_plugin_updater::UpdaterExt;

// A disposable test key signed these harmless bytes, not an executable. The private key is not shipped.
const PACKAGE: &[u8] = include_bytes!("fixtures/update.txt");
const SIGNATURE: &str = include_str!("fixtures/update.sig");
const PUBLIC_KEY: &str = include_str!("fixtures/update.pub");

fn download_fixture(
    version: &str,
    signature: &str,
    bytes: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let manifest = serde_json::json!({
        "version": version,
        "notes": "• 新增图库搜索\n• 优化图片预览",
        "pub_date": "2026-10-25T09:49:03.685Z",
        "platforms": {
            "windows-x86_64": { "signature": signature.trim(), "url": format!("{origin}/repos/owner/repo/releases/assets/1") },
            "windows-x86_64-nsis": { "signature": signature.trim(), "url": format!("{origin}/repos/owner/repo/releases/assets/1") }
        }
    }).to_string();
    let bytes = bytes.to_vec();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopped = stop.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !stopped.load(std::sync::atomic::Ordering::Relaxed) && Instant::now() < deadline {
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let count = stream.read(&mut buf).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..count]);
            }
            let request = String::from_utf8_lossy(&request).to_lowercase();
            let body = if request.starts_with("get /latest.json ") {
                assert!(request.contains("accept: application/json"));
                manifest.as_bytes()
            } else {
                assert!(request.starts_with("get /repos/owner/repo/releases/assets/1 "));
                assert!(request.contains("accept: application/octet-stream"));
                &bytes
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        }
    });
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    // Use the shipped security options, substituting only the test key and local fixture endpoint.
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let mut updater = config["plugins"]["updater"].clone();
    updater["pubkey"] = PUBLIC_KEY.trim().into();
    updater["endpoints"] = serde_json::json!([format!("{origin}/latest.json")]);
    context
        .config_mut()
        .plugins
        .0
        .insert("updater".into(), updater);
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap();
    let result = tauri::async_runtime::block_on(async {
        let update = app
            .updater_builder()
            .target("windows-x86_64")
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())?;
        let Some(update) = update else {
            return Ok(None);
        };
        assert!(update.body.as_ref().unwrap().contains("新增图库搜索"));
        update
            .download(|_, _| {}, || {})
            .await
            .map(Some)
            .map_err(|e| e.to_string())
    });
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();
    result
}

#[test]
fn github_manifest_and_asset_download_verify_with_the_configured_key() {
    assert_eq!(
        download_fixture("99.0.0", SIGNATURE, PACKAGE).unwrap(),
        Some(PACKAGE.to_vec())
    );
}

#[test]
fn corrupt_package_and_missing_signature_are_rejected() {
    assert!(download_fixture("99.0.0", SIGNATURE, b"tampered").is_err());
    assert!(download_fixture("99.0.0", "", PACKAGE).is_err());
}

#[test]
fn manifest_version_must_match_the_signed_version() {
    assert!(download_fixture("99.0.1", SIGNATURE, PACKAGE).is_err());
}

#[test]
fn older_release_does_not_offer_a_downgrade() {
    assert!(download_fixture("0.0.1", SIGNATURE, PACKAGE)
        .unwrap()
        .is_none());
}
