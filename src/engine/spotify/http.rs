//! One HTTP client for sign-in, the Web API and cover art: librespot's hyper client, with
//! statuses and rate limits handled here so a long `Retry-After` never stalls the app.

use std::time::Duration;

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use librespot_core::http_client::HttpClient;

const MAX_RETRY_AFTER: Duration = Duration::from_secs(4);
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HttpError {
    /// `None` when no answer arrived at all.
    pub status: Option<StatusCode>,
    pub message: String,
}

impl HttpError {
    pub(super) fn other(message: impl Into<String>) -> Self {
        Self { status: None, message: message.into() }
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub(super) struct Http {
    client: HttpClient,
}

impl Http {
    pub(super) fn new() -> Self {
        Self { client: HttpClient::new(None) }
    }

    /// The body of a 2xx answer. A 429 that asks for a short wait is retried once.
    pub(super) async fn send(&self, request: Request<Bytes>) -> Result<Bytes, HttpError> {
        let (parts, body) = request.into_parts();
        let mut retried = false;
        loop {
            let mut attempt = Request::builder().method(parts.method.clone()).uri(parts.uri.clone()).body(body.clone()).map_err(|e| HttpError::other(e.to_string()))?;
            *attempt.headers_mut() = parts.headers.clone();
            let pending = self.client.request_fut(attempt).map_err(|e| HttpError::other(e.to_string()))?;
            let response = tokio::time::timeout(TIMEOUT, pending)
                .await
                .map_err(|_| HttpError::other("The request timed out"))?
                .map_err(|e| HttpError::other(format!("Can't reach Spotify: {e}")))?;
            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS {
                let wait = HttpClient::get_retry_after(response.headers());
                match wait {
                    Some(wait) if wait <= MAX_RETRY_AFTER && !retried => {
                        retried = true;
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                    _ => {
                        let seconds = wait.map_or_else(String::new, |w| format!(" for {} s", w.as_secs()));
                        return Err(HttpError { status: Some(status), message: format!("Spotify is rate limiting requests{seconds}") });
                    }
                }
            }
            let collected = tokio::time::timeout(TIMEOUT, response.into_body().collect())
                .await
                .map_err(|_| HttpError::other("The answer timed out"))?
                .map_err(|e| HttpError::other(e.to_string()))?;
            let bytes = collected.to_bytes();
            if !status.is_success() {
                return Err(HttpError { status: Some(status), message: describe_failure(status, &bytes) });
            }
            return Ok(bytes);
        }
    }

    pub(super) async fn get_bytes(&self, url: &str) -> Result<Bytes, HttpError> {
        let request = Request::get(url).body(Bytes::new()).map_err(|e| HttpError::other(e.to_string()))?;
        self.send(request).await
    }
}

/// Spotify's own words when the body carries them (`{"error": {"message": ..}}` from the
/// Web API, `{"error_description": ..}` from accounts), else the status line.
fn describe_failure(status: StatusCode, body: &[u8]) -> String {
    let json: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let said = json.as_ref().and_then(|json| {
        json["error"]["message"].as_str().or_else(|| json["error_description"].as_str()).or_else(|| json["error"].as_str())
    });
    match said {
        Some(said) => format!("{said} ({})", status.as_u16()),
        None => format!("Spotify answered {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_quote_spotify_when_it_explains_itself() {
        let cases: [(&[u8], &str); 4] = [
            (br#"{"error":{"status":403,"message":"Player command failed: Premium required"}}"#, "Player command failed: Premium required (403)"),
            (br#"{"error":"invalid_grant","error_description":"Refresh token revoked"}"#, "Refresh token revoked (403)"),
            (br#"{"error":"invalid_client"}"#, "invalid_client (403)"),
            (b"<html>", "Spotify answered 403 Forbidden"),
        ];
        for (body, expected) in cases {
            assert_eq!(describe_failure(StatusCode::FORBIDDEN, body), expected);
        }
    }
}
