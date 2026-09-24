use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;
use zeroize::Zeroize;

use crate::cli::AuthCommand;
use crate::client::{ApiClient, ApiResponse, decode_response, http_client};
use crate::error::CliError;
use crate::output;
use crate::personal::{MeResponse, get as get_personal};
use crate::token_store::TokenStore;

const CLIENT_ID: &str = "d6e-cli";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_CALLBACK_BYTES: usize = 8192;

#[derive(Deserialize)]
struct TokenResponse {
    access_token: SecretString,
    refresh_token: SecretString,
    token_type: String,
    expires_in: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatus<'a> {
    logged_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<&'a crate::personal::PersonalProfile>,
}

struct AuthAttempt {
    verifier: SecretString,
    state: SecretString,
    challenge: String,
}

impl AuthAttempt {
    fn new() -> Result<Self, CliError> {
        let mut verifier_bytes: [u8; 32] = [0_u8; 32];
        let mut state_bytes: [u8; 32] = [0_u8; 32];
        getrandom::fill(&mut verifier_bytes).map_err(|_| CliError::Authentication {
            message: "could not generate PKCE verifier".to_owned(),
        })?;
        getrandom::fill(&mut state_bytes).map_err(|_| CliError::Authentication {
            message: "could not generate OAuth state".to_owned(),
        })?;
        let verifier: SecretString = SecretString::from(URL_SAFE_NO_PAD.encode(verifier_bytes));
        let state: SecretString = SecretString::from(URL_SAFE_NO_PAD.encode(state_bytes));
        verifier_bytes.zeroize();
        state_bytes.zeroize();
        let digest = Sha256::digest(verifier.expose_secret().as_bytes());
        let challenge: String = URL_SAFE_NO_PAD.encode(digest);
        Ok(Self {
            verifier,
            state,
            challenge,
        })
    }

    fn authorization_url(&self, auth_url: &Url, redirect_uri: &Url) -> Result<Url, CliError> {
        let mut url: Url = auth_url
            .join("auth/login")
            .map_err(|_| CliError::InvalidInput {
                message: "could not construct authorization URL".to_owned(),
            })?;
        url.query_pairs_mut()
            .append_pair("client_id", CLIENT_ID)
            .append_pair("redirect_uri", redirect_uri.as_str())
            .append_pair("state", self.state.expose_secret())
            .append_pair("code_challenge", &self.challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url)
    }
}

fn validate_token_response(token: &TokenResponse) -> Result<(), CliError> {
    if token.token_type != "Bearer"
        || token.expires_in == 0
        || token.access_token.expose_secret().trim().is_empty()
        || token.refresh_token.expose_secret().trim().is_empty()
    {
        return Err(CliError::Authentication {
            message: "D6E Auth returned an invalid token response".to_owned(),
        });
    }
    Ok(())
}

async fn exchange_code(
    auth_url: &Url,
    code: &SecretString,
    verifier: &SecretString,
    redirect_uri: &Url,
) -> Result<TokenResponse, CliError> {
    let token_url: Url =
        auth_url
            .join("api/v1/auth/token")
            .map_err(|_| CliError::InvalidInput {
                message: "could not construct token URL".to_owned(),
            })?;
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "client_id": CLIENT_ID,
        "code": code.expose_secret(),
        "code_verifier": verifier.expose_secret(),
        "redirect_uri": redirect_uri.as_str(),
    });
    let response: reqwest::Response = http_client()?.post(token_url).json(&body).send().await?;
    let decoded: ApiResponse<TokenResponse> = decode_response(response).await?;
    validate_token_response(&decoded.value)?;
    Ok(decoded.value)
}

async fn refresh(auth_url: &Url, refresh_token: &SecretString) -> Result<TokenResponse, CliError> {
    let token_url: Url =
        auth_url
            .join("api/v1/auth/token")
            .map_err(|_| CliError::InvalidInput {
                message: "could not construct token URL".to_owned(),
            })?;
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": CLIENT_ID,
        "refresh_token": refresh_token.expose_secret(),
    });
    let response: reqwest::Response = http_client()?.post(token_url).json(&body).send().await?;
    let decoded: ApiResponse<TokenResponse> = decode_response(response).await?;
    validate_token_response(&decoded.value)?;
    Ok(decoded.value)
}

pub async fn access_token_from_store(
    auth_url: &Url,
    store: &dyn TokenStore,
) -> Result<SecretString, CliError> {
    let refresh_token: SecretString = store.load(auth_url)?.ok_or(CliError::MissingCredential)?;
    let result: Result<TokenResponse, CliError> = refresh(auth_url, &refresh_token).await;
    let token: TokenResponse = match result {
        Ok(token) => token,
        Err(CliError::Api {
            status, ref code, ..
        }) if status == reqwest::StatusCode::BAD_REQUEST && code == "invalid_grant" => {
            store.delete(auth_url)?;
            return Err(CliError::MissingCredential);
        }
        Err(error) => return Err(error),
    };
    store.save(auth_url, &token.refresh_token)?;
    Ok(token.access_token)
}

pub async fn run(
    auth_url: &Url,
    store: &dyn TokenStore,
    command: AuthCommand,
) -> Result<(), CliError> {
    match command {
        AuthCommand::Login { no_open } => login(auth_url, store, no_open).await,
        AuthCommand::Status => status(auth_url, store).await,
        AuthCommand::Logout => {
            store.delete(auth_url)?;
            output::print_data(
                &AuthStatus {
                    logged_in: false,
                    user: None,
                },
                None,
            )
        }
    }
}

async fn status(auth_url: &Url, store: &dyn TokenStore) -> Result<(), CliError> {
    if store.load(auth_url)?.is_none() {
        return output::print_data(
            &AuthStatus {
                logged_in: false,
                user: None,
            },
            None,
        );
    }
    let access_token: SecretString = match access_token_from_store(auth_url, store).await {
        Ok(token) => token,
        Err(CliError::MissingCredential) => {
            return output::print_data(
                &AuthStatus {
                    logged_in: false,
                    user: None,
                },
                None,
            );
        }
        Err(error) => return Err(error),
    };
    let client: ApiClient = ApiClient::new(auth_url.clone(), access_token)?;
    let response: ApiResponse<MeResponse> = get_personal(&client).await?;
    output::print_data(
        &AuthStatus {
            logged_in: true,
            user: Some(&response.value.user),
        },
        response.request_id.as_deref(),
    )
}

async fn login(auth_url: &Url, store: &dyn TokenStore, no_open: bool) -> Result<(), CliError> {
    let listener: TcpListener = TcpListener::bind("127.0.0.1:0").await?;
    let port: u16 = listener.local_addr()?.port();
    let redirect_uri: Url =
        Url::parse(&format!("http://127.0.0.1:{port}/callback")).map_err(|_| {
            CliError::Authentication {
                message: "could not create callback URL".to_owned(),
            }
        })?;
    let attempt: AuthAttempt = AuthAttempt::new()?;
    let authorization_url: Url = attempt.authorization_url(auth_url, &redirect_uri)?;

    if no_open || webbrowser::open(authorization_url.as_str()).is_err() {
        eprintln!("Open this URL to sign in:\n{authorization_url}");
    }

    let code: SecretString = wait_for_callback(listener, port, &attempt.state).await?;
    let tokens: TokenResponse =
        exchange_code(auth_url, &code, &attempt.verifier, &redirect_uri).await?;
    let client: ApiClient = ApiClient::new(auth_url.clone(), tokens.access_token)?;
    let response: ApiResponse<MeResponse> = get_personal(&client).await?;
    store.save(auth_url, &tokens.refresh_token)?;
    output::print_data(
        &AuthStatus {
            logged_in: true,
            user: Some(&response.value.user),
        },
        response.request_id.as_deref(),
    )
}

async fn wait_for_callback(
    listener: TcpListener,
    port: u16,
    state: &SecretString,
) -> Result<SecretString, CliError> {
    tokio::time::timeout(CALLBACK_TIMEOUT, async {
        loop {
            let (mut stream, _peer): (TcpStream, std::net::SocketAddr) = listener.accept().await?;
            let request: Option<String> = read_callback_request(&mut stream).await?;
            let Some(request) = request else {
                send_callback_response(&mut stream, false).await?;
                continue;
            };
            let result: CallbackResult = parse_callback(&request, port, state);
            send_callback_response(&mut stream, matches!(result, CallbackResult::Code(_))).await?;
            match result {
                CallbackResult::Code(code) => return Ok(code),
                CallbackResult::Denied => {
                    return Err(CliError::Authentication {
                        message: "sign-in was denied".to_owned(),
                    });
                }
                CallbackResult::Ignore => continue,
            }
        }
    })
    .await
    .map_err(|_| CliError::Authentication {
        message: "sign-in timed out".to_owned(),
    })?
}

async fn read_callback_request(stream: &mut TcpStream) -> Result<Option<String>, CliError> {
    let mut bytes: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk: [u8; 1024] = [0_u8; 1024];
    loop {
        let count: usize = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .map_err(|_| CliError::Authentication {
                message: "callback request timed out".to_owned(),
            })??;
        if count == 0 {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_CALLBACK_BYTES {
            return Ok(None);
        }
        if bytes.windows(4).any(|window: &[u8]| window == b"\r\n\r\n") {
            return Ok(String::from_utf8(bytes).ok());
        }
    }
}

enum CallbackResult {
    Code(SecretString),
    Denied,
    Ignore,
}

fn parse_callback(request: &str, port: u16, expected_state: &SecretString) -> CallbackResult {
    let Some(first_line) = request.lines().next() else {
        return CallbackResult::Ignore;
    };
    let mut parts = first_line.split_whitespace();
    if parts.next() != Some("GET") {
        return CallbackResult::Ignore;
    }
    let Some(target) = parts.next() else {
        return CallbackResult::Ignore;
    };
    if parts.next() != Some("HTTP/1.1") || parts.next().is_some() || !target.starts_with('/') {
        return CallbackResult::Ignore;
    }
    let expected_host: String = format!("127.0.0.1:{port}");
    let Some((headers, _body)) = request.split_once("\r\n\r\n") else {
        return CallbackResult::Ignore;
    };
    let host_count: usize = headers
        .lines()
        .filter(|line: &&str| line.to_ascii_lowercase().starts_with("host:"))
        .count();
    if host_count != 1
        || !headers
            .lines()
            .any(|line: &str| line.eq_ignore_ascii_case(&format!("host: {expected_host}")))
    {
        return CallbackResult::Ignore;
    }
    let Ok(callback_url) = Url::parse(&format!("http://127.0.0.1:{port}{target}")) else {
        return CallbackResult::Ignore;
    };
    if callback_url.path() != "/callback" || callback_url.fragment().is_some() {
        return CallbackResult::Ignore;
    }
    let mut state: Option<String> = None;
    let mut code: Option<String> = None;
    let mut error: Option<String> = None;
    for (key, value) in callback_url.query_pairs() {
        let slot: &mut Option<String> = match key.as_ref() {
            "state" => &mut state,
            "code" => &mut code,
            "error" => &mut error,
            "error_description" => continue,
            _ => return CallbackResult::Ignore,
        };
        if slot.is_some() {
            return CallbackResult::Ignore;
        }
        *slot = Some(value.into_owned());
    }
    if state.as_deref() != Some(expected_state.expose_secret()) {
        return CallbackResult::Ignore;
    }
    if error.is_some() {
        return CallbackResult::Denied;
    }
    match code {
        Some(code) if !code.is_empty() => CallbackResult::Code(SecretString::from(code)),
        _ => CallbackResult::Ignore,
    }
}

async fn send_callback_response(stream: &mut TcpStream, success: bool) -> Result<(), CliError> {
    let (status, body): (&str, &str) = if success {
        (
            "200 OK",
            "Authorization received. Return to the terminal to confirm D6E CLI sign-in.",
        )
    } else {
        ("400 Bad Request", "D6E CLI could not accept this callback.")
    };
    let response: String = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/plain; charset=utf-8\r\ncache-control: no-store\r\ncontent-security-policy: default-src 'none'\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use secrecy::{ExposeSecret, SecretString};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use url::Url;

    use super::{
        AuthAttempt, CallbackResult, access_token_from_store, exchange_code, parse_callback,
    };
    use crate::error::CliError;
    use crate::token_store::TokenStore;

    struct MemoryStore(Mutex<Option<String>>);

    impl TokenStore for MemoryStore {
        fn load(&self, _auth_url: &Url) -> Result<Option<SecretString>, CliError> {
            Ok(self
                .0
                .lock()
                .expect("memory store lock")
                .clone()
                .map(SecretString::from))
        }

        fn save(&self, _auth_url: &Url, token: &SecretString) -> Result<(), CliError> {
            *self.0.lock().expect("memory store lock") = Some(token.expose_secret().to_owned());
            Ok(())
        }

        fn delete(&self, _auth_url: &Url) -> Result<(), CliError> {
            *self.0.lock().expect("memory store lock") = None;
            Ok(())
        }
    }

    #[test]
    fn authorization_url_uses_public_client_and_pkce() {
        let attempt: AuthAttempt = AuthAttempt::new().expect("PKCE attempt");
        let origin: Url = Url::parse("https://d6e.ai/").expect("auth origin");
        let redirect: Url = Url::parse("http://127.0.0.1:49152/callback").expect("callback URL");
        let url: Url = attempt
            .authorization_url(&origin, &redirect)
            .expect("authorization URL");
        let parameters: std::collections::HashMap<String, String> = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        assert_eq!(url.path(), "/auth/login");
        assert_eq!(parameters["client_id"], "d6e-cli");
        assert_eq!(parameters["redirect_uri"], redirect.as_str());
        assert_eq!(parameters["code_challenge_method"], "S256");
        assert_eq!(parameters["code_challenge"], attempt.challenge);
        assert_eq!(attempt.verifier.expose_secret().len(), 43);
        assert_ne!(
            parameters["code_challenge"],
            *attempt.verifier.expose_secret()
        );
        assert!(!parameters.contains_key("client_secret"));
    }

    #[test]
    fn callback_requires_exact_path_host_and_state() {
        let state: SecretString = SecretString::from("expected-state".to_owned());
        let valid: &str = "GET /callback?code=opaque-code&state=expected-state HTTP/1.1\r\nhost: 127.0.0.1:49152\r\n\r\n";
        let wrong_state: &str =
            "GET /callback?code=opaque-code&state=wrong HTTP/1.1\r\nhost: 127.0.0.1:49152\r\n\r\n";
        let duplicate_state: &str = "GET /callback?code=opaque-code&state=expected-state&state=wrong HTTP/1.1\r\nhost: 127.0.0.1:49152\r\n\r\n";
        let wrong_host: &str = "GET /callback?code=opaque-code&state=expected-state HTTP/1.1\r\nhost: evil.test\r\n\r\n";
        let wrong_path: &str = "GET /other?code=opaque-code&state=expected-state HTTP/1.1\r\nhost: 127.0.0.1:49152\r\n\r\n";

        assert!(matches!(
            parse_callback(valid, 49152, &state),
            CallbackResult::Code(_)
        ));
        for request in [wrong_state, duplicate_state, wrong_host, wrong_path] {
            assert!(matches!(
                parse_callback(request, 49152, &state),
                CallbackResult::Ignore
            ));
        }
    }

    #[tokio::test]
    async fn authorization_code_exchange_binds_verifier_and_redirect_without_client_secret() {
        let listener: TcpListener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("token server");
        let port: u16 = listener.local_addr().expect("server address").port();
        let auth_url: Url = Url::parse(&format!("http://127.0.0.1:{port}/")).expect("auth origin");
        let redirect_uri: Url =
            Url::parse("http://127.0.0.1:49152/callback").expect("callback URI");
        let code: SecretString = SecretString::from("one-time-code".to_owned());
        let verifier: SecretString = SecretString::from("test-verifier".to_owned());

        let server = tokio::spawn(async move {
            let (mut stream, _): (tokio::net::TcpStream, std::net::SocketAddr) =
                listener.accept().await.expect("token request");
            let mut request: Vec<u8> = Vec::new();
            let mut chunk: [u8; 1024] = [0_u8; 1024];
            loop {
                let count: usize = stream.read(&mut chunk).await.expect("read request");
                assert!(count > 0, "request ended before JSON body");
                request.extend_from_slice(&chunk[..count]);
                let text: String = String::from_utf8_lossy(&request).into_owned();
                if text.contains("\r\n\r\n") && text.contains("one-time-code") {
                    assert!(text.starts_with("POST /api/v1/auth/token HTTP/1.1"));
                    assert!(text.contains("\"grant_type\":\"authorization_code\""));
                    assert!(text.contains("\"code_verifier\":\"test-verifier\""));
                    assert!(text.contains("http://127.0.0.1:49152/callback"));
                    assert!(!text.contains("client_secret"));
                    break;
                }
            }
            let body: &str = r#"{"access_token":"access-only","refresh_token":"refresh-only","token_type":"Bearer","expires_in":3600}"#;
            let response: String = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("send token response");
        });

        let tokens = exchange_code(&auth_url, &code, &verifier, &redirect_uri)
            .await
            .expect("token exchange");
        server.await.expect("server result");
        assert_eq!(tokens.access_token.expose_secret(), "access-only");
        assert_eq!(tokens.refresh_token.expose_secret(), "refresh-only");
    }

    #[tokio::test]
    async fn refresh_uses_public_client_without_a_secret_and_replaces_stored_token() {
        let listener: TcpListener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("token server");
        let port: u16 = listener.local_addr().expect("server address").port();
        let auth_url: Url = Url::parse(&format!("http://127.0.0.1:{port}/")).expect("auth origin");
        let store: MemoryStore = MemoryStore(Mutex::new(Some("old-refresh".to_owned())));

        let server = tokio::spawn(async move {
            let (mut stream, _): (tokio::net::TcpStream, std::net::SocketAddr) =
                listener.accept().await.expect("token request");
            let mut request: Vec<u8> = Vec::new();
            let mut chunk: [u8; 1024] = [0_u8; 1024];
            loop {
                let count: usize = stream.read(&mut chunk).await.expect("read request");
                assert!(count > 0, "request ended before JSON body");
                request.extend_from_slice(&chunk[..count]);
                let text: String = String::from_utf8_lossy(&request).into_owned();
                if text.contains("\r\n\r\n") && text.contains("old-refresh") {
                    assert!(text.starts_with("POST /api/v1/auth/token HTTP/1.1"));
                    assert!(text.contains("\"client_id\":\"d6e-cli\""));
                    assert!(!text.contains("client_secret"));
                    break;
                }
            }
            let body: &str = r#"{"access_token":"access-only","refresh_token":"new-refresh","token_type":"Bearer","expires_in":3600}"#;
            let response: String = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("send token response");
        });

        let token: SecretString = access_token_from_store(&auth_url, &store)
            .await
            .expect("refreshed access token");
        server.await.expect("server result");
        assert_eq!(token.expose_secret(), "access-only");
        assert_eq!(
            store.0.lock().expect("memory store lock").as_deref(),
            Some("new-refresh")
        );
    }
}
