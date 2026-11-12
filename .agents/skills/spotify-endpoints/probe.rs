//! A throwaway probe of one Spotify call, signed in with the saved sign-in and without a
//! Connect device. Copy to `examples/probe.rs` (git ignores it), edit the call in `main`, and
//! run `nix develop -c cargo run --example probe`. It prints the answer's outline, never the
//! token. Names and ids from the account can appear in the preview, so keep the output local.

use http::Method;
use librespot_core::{Session, SessionConfig, cache::Cache};

fn outline(body: &[u8]) -> String {
    match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(serde_json::Value::Object(map)) => {
            let keys: Vec<String> = map.iter().map(|(k, v)| format!("{k}{}", v.as_array().map_or(String::new(), |a| format!("[{}]", a.len())))).collect();
            format!("json with keys {keys:?}")
        }
        Ok(_) => "json".into(),
        Err(_) => format!("{} bytes, not json (protobuf?)", body.len()),
    }
}

fn uris(body: &[u8], prefix: &str) -> usize {
    String::from_utf8_lossy(body).matches(prefix).count()
}

#[tokio::main]
async fn main() {
    let data = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".local/share/baliset");
    let cache = Cache::new(Some(data.clone()), None, None, None).unwrap();
    let device_id = std::fs::read_to_string(data.join("device_id")).unwrap().trim().to_owned();
    let session = Session::new(SessionConfig { device_id, ..SessionConfig::default() }, None);
    session.connect(cache.credentials().expect("no saved sign-in; run baliset and sign in"), false).await.expect("connect");
    let spclient = session.spclient();

    let endpoint = format!("/user-profile-view/v3/profile/{}", session.username());
    match spclient.request(&Method::GET, &endpoint, None, None).await {
        Ok(body) => println!("ok: {}, {} track uris", outline(&body), uris(&body, "spotify:track:")),
        Err(error) => println!("FAILED: {error}"),
    }
}
