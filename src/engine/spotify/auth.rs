//! Signing in: the authorization-code flow with PKCE against accounts.spotify.com, using the
//! desktop client's id as librespot does, and the token refresh that follows it.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::http::{Http, HttpError};

const CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

/// The redirect the desktop client id accepts: loopback, any of these ports, `/login`.
const PORTS: [u16; 4] = [5588, 5589, 5590, 5591];

/// What a Connect device and its library need, as librespot asks for them.
const SCOPES: &[&str] = &[
    "app-remote-control",
    "playlist-modify-private",
    "playlist-modify-public",
    "playlist-read-collaborative",
    "playlist-read-private",
    "streaming",
    "user-follow-modify",
    "user-follow-read",
    "user-library-modify",
    "user-library-read",
    "user-modify-playback-state",
    "user-read-currently-playing",
    "user-read-playback-position",
    "user-read-playback-state",
    "user-read-private",
    "user-read-recently-played",
    "user-top-read",
];

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Baliset</title>\
<body style=\"font:16px system-ui;background:#111;color:#eee;display:grid;place-items:center;height:100vh;margin:0\">\
<p>Signed in. You can close this tab and go back to Baliset.</p>";


/// A sign-in in progress: the page to open and the loopback listener its redirect lands on.
pub(super) struct Login {
    pub url: String,
    listener: TcpListener,
    redirect_uri: String,
    verifier: String,
    state: String,
}

fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 48];
    getrandom::fill(&mut bytes).map_err(|e| format!("no random bytes: {e}"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

impl Login {
    pub(super) async fn begin() -> Result<Self, String> {
        let mut bound = None;
        for port in PORTS {
            if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
                bound = Some((listener, port));
                break;
            }
        }
        let (listener, port) = bound.ok_or("Ports 5588 to 5591 are all in use, so the sign-in page has nowhere to return to")?;
        let redirect_uri = format!("http://127.0.0.1:{port}/login");
        let (verifier, state) = (random_token()?, random_token()?);
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", CLIENT_ID)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &challenge(&verifier))
            .append_pair("state", &state)
            .append_pair("scope", &SCOPES.join(" "))
            .finish();
        Ok(Self { url: format!("https://accounts.spotify.com/authorize?{query}"), listener, redirect_uri, verifier, state })
    }

    /// Wait for the browser to come back, then trade its code for an access token, which
    /// the session turns into credentials it can reuse. Requests that
    /// aren't this sign-in's redirect (a favicon, a stale tab) are answered and ignored.
    pub(super) async fn finish(self, http: &Http) -> Result<String, String> {
        loop {
            let (mut stream, _) = self.listener.accept().await.map_err(|e| e.to_string())?;
            let Ok(head) = tokio::time::timeout(Duration::from_secs(5), read_head(&mut stream)).await else { continue };
            let request = String::from_utf8_lossy(&head);
            let outcome = redirect_outcome(&request, &self.state);
            let body = match &outcome {
                Redirect::Code(_) => DONE_PAGE.to_owned(),
                Redirect::Denied(reason) => format!("Sign-in was not completed: {reason}"),
                Redirect::Unrelated => String::new(),
            };
            let status = if matches!(outcome, Redirect::Unrelated) { "404 Not Found" } else { "200 OK" };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            // The browser may already have gone; the code is what matters.
            let _written = stream.write_all(response.as_bytes()).await;
            match outcome {
                Redirect::Code(code) => {
                    let form = [
                        ("grant_type", "authorization_code"),
                        ("code", code.as_str()),
                        ("redirect_uri", self.redirect_uri.as_str()),
                        ("client_id", CLIENT_ID),
                        ("code_verifier", self.verifier.as_str()),
                    ];
                    return request_token(http, &form).await.map_err(|e| e.to_string());
                }
                Redirect::Denied(reason) => return Err(format!("Spotify said: {reason}")),
                Redirect::Unrelated => {}
            }
        }
    }
}

/// A request up to the end of its headers, however TCP split it.
async fn read_head(stream: &mut (impl AsyncReadExt + Unpin)) -> Vec<u8> {
    const MAX_HEAD: usize = 16 * 1024;
    let mut head = Vec::new();
    let mut chunk = [0u8; 2048];
    while head.len() < MAX_HEAD && !head.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk).await {
            Ok(read) if read > 0 => head.extend_from_slice(&chunk[..read]),
            _ => break,
        }
    }
    head
}

#[derive(Debug, PartialEq, Eq)]
enum Redirect {
    Code(String),
    Denied(String),
    Unrelated,
}

fn redirect_outcome(request: &str, expected_state: &str) -> Redirect {
    let Some(target) = request.strip_prefix("GET ").and_then(|rest| rest.split_whitespace().next()) else {
        return Redirect::Unrelated;
    };
    let Some(query) = target.strip_prefix("/login?") else {
        return Redirect::Unrelated;
    };
    let (mut code, mut state, mut error) = (None, None, None);
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Redirect::Unrelated;
    }
    match (code, error) {
        (Some(code), _) => Redirect::Code(code),
        (None, Some(error)) => Redirect::Denied(error),
        (None, None) => Redirect::Unrelated,
    }
}

async fn request_token(http: &Http, form: &[(&str, &str)]) -> Result<String, HttpError> {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(form).finish();
    let request = http::Request::post("https://accounts.spotify.com/api/token")
        .header(http::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(bytes::Bytes::from(body))
        .map_err(|e| HttpError::other(e.to_string()))?;
    let answer = http.send(request).await?;
    let json: serde_json::Value = serde_json::from_slice(&answer).map_err(|e| HttpError::other(e.to_string()))?;
    json["access_token"].as_str().map(str::to_owned).ok_or_else(|| HttpError::other("the token answer has no access token"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_the_rfc_7636_example() {
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn a_redirect_with_this_sign_ins_state_yields_its_code() {
        let request = "GET /login?code=abc%2Fdef&state=s1 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        assert_eq!(redirect_outcome(request, "s1"), Redirect::Code("abc/def".into()));
    }

    #[test]
    fn other_requests_are_ignored() {
        for request in [
            "GET /favicon.ico HTTP/1.1\r\n\r\n",
            "GET /login?code=abc&state=someone-else HTTP/1.1\r\n\r\n",
            "GET /login?code=abc HTTP/1.1\r\n\r\n",
            "POST /login?code=abc&state=s1 HTTP/1.1\r\n\r\n",
            "",
        ] {
            assert_eq!(redirect_outcome(request, "s1"), Redirect::Unrelated, "{request:?}");
        }
    }

    #[tokio::test]
    async fn a_request_split_across_reads_is_read_whole() {
        let (mut browser, mut listener) = tokio::io::duplex(64);
        let sent = tokio::spawn(async move {
            for part in ["GET /login?code=abc&sta", "te=s1 HTTP/1.1\r\nHost: 127.0.0.1\r\n", "\r\n"] {
                browser.write_all(part.as_bytes()).await.unwrap();
                tokio::task::yield_now().await;
            }
            browser
        });
        let head = read_head(&mut listener).await;
        assert_eq!(redirect_outcome(&String::from_utf8_lossy(&head), "s1"), Redirect::Code("abc".into()));
        drop(sent.await);
    }

    #[test]
    fn a_refusal_is_reported() {
        let request = "GET /login?error=access_denied&state=s1 HTTP/1.1\r\n\r\n";
        assert_eq!(redirect_outcome(request, "s1"), Redirect::Denied("access_denied".into()));
    }
}
