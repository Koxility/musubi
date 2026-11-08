//! AniList-Login über den Implicit Grant.
//!
//! Der Implicit Grant braucht nur die `client_id`, also kein Secret, das eine Desktop-App ohnehin
//! nicht schützen könnte. AniList liefert das Token im URL-Fragment. Das erreicht keinen Server,
//! deshalb liest eine kleine lokale Seite es aus und schickt es an den Loopback-Server.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::{Error, Result};

const MAX_REQUEST: usize = 16 * 1024;

pub fn authorize_url(client_id: &str) -> String {
    let mut url =
        url::Url::parse("https://anilist.co/api/v2/oauth/authorize").expect("static URL is valid");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "token");
    url.to_string()
}

/// Redirect-URL, die in der AniList-Anwendung eingetragen werden muss.
pub fn loopback_redirect(port: u16) -> String {
    format!("http://127.0.0.1:{port}/callback")
}

pub const PIN_REDIRECT: &str = "https://anilist.co/api/v2/oauth/pin";

/// Akzeptiert ein rohes Token oder eine komplette Redirect-URL mit Fragment.
pub fn parse_pin_input(input: &str) -> Option<SecretString> {
    let trimmed = input.trim();
    let token = match trimmed.split_once("access_token=") {
        Some((_, rest)) => rest.split('&').next().unwrap_or(rest),
        None => trimmed,
    };
    let plausible = !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    plausible.then(|| SecretString::from(token.to_owned()))
}

/// Liest den Ablaufzeitpunkt aus dem JWT. Die Signatur prüft der Server, hier geht es nur
/// darum, rechtzeitig vor dem Ablauf zu warnen.
pub fn token_expiry(token: &str) -> Option<DateTime<Utc>> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let exp = claims.get("exp")?;
    let seconds = exp.as_i64().or_else(|| exp.as_f64().map(|f| f as i64))?;
    DateTime::from_timestamp(seconds, 0)
}

fn landing_page(nonce: &str) -> String {
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>Musubi</title>
<body style="font:16px system-ui;margin:4em auto;max-width:30em"><p id="m">Finishing login…</p>
<script>
const p = new URLSearchParams(location.hash.slice(1));
const t = p.get("access_token");
const m = document.getElementById("m");
history.replaceState(null, "", location.pathname);
if (!t) {{ m.textContent = "AniList did not return a token. Please try again."; }}
else fetch("/token/{nonce}", {{method: "POST", body: t}})
  .then(r => {{ m.textContent = r.ok ? "Login complete. You can close this tab." : "Login failed."; }})
  .catch(() => {{ m.textContent = "Login failed."; }});
</script>"#
    )
}

struct Request {
    method: String,
    path: String,
    host: Option<String>,
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    let header_end = loop {
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if buffer.len() > MAX_REQUEST {
            return None;
        }
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = std::str::from_utf8(&buffer[..header_end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split(' ');
    let method = request_line.next()?.to_owned();
    let path = request_line.next()?.to_owned();
    let mut length = 0usize;
    let mut host = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().ok()?;
        } else if name.eq_ignore_ascii_case("host") {
            host = Some(value.trim().to_owned());
        }
    }
    if length > MAX_REQUEST {
        return None;
    }
    while buffer.len() < header_end + length {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = buffer[header_end..header_end + length].to_vec();
    Some(Request {
        method,
        path,
        host,
        body,
    })
}

async fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Bindet den Loopback-Port. Getrennt vom Warten, damit erst danach der Browser startet.
pub async fn bind_loopback(port: u16) -> Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| Error::Login(format!("could not listen on 127.0.0.1:{port}: {e}")))
}

/// Wartet auf das Token. Der Host-Header wird geprüft (Schutz vor DNS-Rebinding) und der
/// Token-Pfad ist pro Login zufällig.
pub async fn wait_for_token(listener: TcpListener, limit: Duration) -> Result<SecretString> {
    let port = listener
        .local_addr()
        .map_err(|e| Error::Login(e.to_string()))?
        .port();
    let expected_host = format!("127.0.0.1:{port}");
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let token_path = format!("/token/{nonce}");

    let serve = async {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            let read = timeout(Duration::from_secs(5), read_request(&mut stream)).await;
            let Ok(Some(request)) = read else { continue };
            if request.host.as_deref() != Some(expected_host.as_str()) {
                respond(&mut stream, "403 Forbidden", "text/plain", "forbidden").await;
                continue;
            }
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/callback") => {
                    let page = landing_page(&nonce);
                    respond(&mut stream, "200 OK", "text/html; charset=utf-8", &page).await;
                }
                ("POST", path) if path == token_path => {
                    let token = String::from_utf8_lossy(&request.body).trim().to_owned();
                    if let Some(token) = parse_pin_input(&token) {
                        respond(&mut stream, "200 OK", "text/plain", "ok").await;
                        return token;
                    }
                    respond(&mut stream, "400 Bad Request", "text/plain", "bad token").await;
                }
                _ => respond(&mut stream, "404 Not Found", "text/plain", "not found").await,
            }
        }
    };
    timeout(limit, serve)
        .await
        .map_err(|_| Error::Login("timed out waiting for the browser".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn authorize_url_uses_implicit_grant() {
        assert_eq!(
            authorize_url("123"),
            "https://anilist.co/api/v2/oauth/authorize?client_id=123&response_type=token"
        );
    }

    #[test]
    fn pin_input_accepts_token_and_url() {
        assert_eq!(
            parse_pin_input(" abc.def-g_h \n").unwrap().expose_secret(),
            "abc.def-g_h"
        );
        let url = "https://anilist.co/api/v2/oauth/pin#access_token=tok123&token_type=Bearer";
        assert_eq!(parse_pin_input(url).unwrap().expose_secret(), "tok123");
        assert!(parse_pin_input("").is_none());
        assert!(parse_pin_input("has spaces in it").is_none());
    }

    #[test]
    fn expiry_comes_from_jwt_claims() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"exp": 1893456000}"#);
        let token = format!("h.{payload}.s");
        assert_eq!(token_expiry(&token).unwrap().timestamp(), 1_893_456_000);
        assert!(token_expiry("not-a-jwt").is_none());
    }

    async fn http(port: u16, request: String) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut out = String::new();
        stream.read_to_string(&mut out).await.unwrap();
        out
    }

    #[tokio::test]
    async fn loopback_flow_serves_page_and_returns_token() {
        let listener = bind_loopback(0).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = format!("127.0.0.1:{port}");
        let waiter = tokio::spawn(wait_for_token(listener, Duration::from_secs(5)));

        let bad_host = http(
            port,
            "GET /callback HTTP/1.1\r\nHost: evil.test\r\n\r\n".into(),
        )
        .await;
        assert!(bad_host.starts_with("HTTP/1.1 403"));

        let page = http(
            port,
            format!("GET /callback HTTP/1.1\r\nHost: {host}\r\n\r\n"),
        )
        .await;
        assert!(page.starts_with("HTTP/1.1 200"));
        let start = page.find("/token/").unwrap();
        let nonce: String = page[start + 7..]
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect();

        let wrong = http(
            port,
            format!("POST /token/wrong HTTP/1.1\r\nHost: {host}\r\nContent-Length: 3\r\n\r\nabc"),
        )
        .await;
        assert!(wrong.starts_with("HTTP/1.1 404"));

        let ok = http(
            port,
            format!(
                "POST /token/{nonce} HTTP/1.1\r\nHost: {host}\r\nContent-Length: 5\r\n\r\ntok99"
            ),
        )
        .await;
        assert!(ok.starts_with("HTTP/1.1 200"));
        assert_eq!(waiter.await.unwrap().unwrap().expose_secret(), "tok99");
    }

    #[tokio::test]
    async fn loopback_times_out() {
        let listener = bind_loopback(0).await.unwrap();
        let result = wait_for_token(listener, Duration::from_millis(30)).await;
        assert!(matches!(result, Err(Error::Login(_))));
    }
}
