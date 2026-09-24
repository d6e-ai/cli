use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::access_token_from_store;
use crate::cli::PersonalCommand;
use crate::client::{ApiClient, ApiResponse};
use crate::error::CliError;
use crate::output;
use crate::token_store::TokenStore;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalProfile {
    pub id: String,
    pub email: String,
    pub name: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct MeResponse {
    pub user: PersonalProfile,
}

#[derive(Serialize)]
struct UpdateNameRequest<'a> {
    name: &'a str,
}

pub async fn get(client: &ApiClient) -> Result<ApiResponse<MeResponse>, CliError> {
    client.get("api/v1/me").await
}

pub async fn run(
    auth_url: &Url,
    store: &dyn TokenStore,
    command: PersonalCommand,
) -> Result<(), CliError> {
    let access_token: SecretString = access_token_from_store(auth_url, store).await?;
    let client: ApiClient = ApiClient::new(auth_url.clone(), access_token)?;

    let response: ApiResponse<MeResponse> = match command {
        PersonalCommand::Show => get(&client).await?,
        PersonalCommand::Update { name } => {
            let trimmed: &str = name.trim();
            if trimmed.is_empty() || trimmed.encode_utf16().count() > 100 {
                return Err(CliError::InvalidInput {
                    message: "name must be between 1 and 100 characters".to_owned(),
                });
            }
            let body: UpdateNameRequest<'_> = UpdateNameRequest { name: trimmed };
            client.patch("api/v1/me", &body).await?
        }
    };
    output::print_data(&response.value.user, response.request_id.as_deref())
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use url::Url;

    use super::{MeResponse, UpdateNameRequest, get};
    use crate::client::{ApiClient, ApiResponse};

    #[tokio::test]
    async fn me_requests_use_bearer_access_token_and_name_only_patch() {
        let listener: TcpListener = TcpListener::bind("127.0.0.1:0").await.expect("API server");
        let port: u16 = listener.local_addr().expect("server address").port();
        let auth_url: Url = Url::parse(&format!("http://127.0.0.1:{port}/")).expect("auth origin");
        let server = tokio::spawn(async move {
            for method in ["GET", "PATCH"] {
                let (mut stream, _): (tokio::net::TcpStream, std::net::SocketAddr) =
                    listener.accept().await.expect("API request");
                let mut request: Vec<u8> = Vec::new();
                let mut chunk: [u8; 1024] = [0_u8; 1024];
                loop {
                    let count: usize = stream.read(&mut chunk).await.expect("read request");
                    assert!(count > 0, "request ended before headers");
                    request.extend_from_slice(&chunk[..count]);
                    let text: String = String::from_utf8_lossy(&request).into_owned();
                    if text.contains("\r\n\r\n")
                        && (method == "GET" || text.contains("\"name\":\"New name\""))
                    {
                        assert!(text.starts_with(&format!("{method} /api/v1/me HTTP/1.1")));
                        assert!(
                            text.to_ascii_lowercase()
                                .contains("authorization: bearer access-only")
                        );
                        if method == "PATCH" {
                            assert!(text.ends_with("{\"name\":\"New name\"}"));
                        }
                        break;
                    }
                }
                let body: &str = if method == "GET" {
                    r#"{"user":{"id":"user-1","email":"person@example.com","name":"Old name","updatedAt":"2026-09-24T00:00:00.000Z"}}"#
                } else {
                    r#"{"user":{"id":"user-1","email":"person@example.com","name":"New name","updatedAt":"2026-09-24T00:01:00.000Z"}}"#
                };
                let response: String = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nx-request-id: req-personal\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("send API response");
            }
        });

        let client: ApiClient =
            ApiClient::new(auth_url, SecretString::from("access-only".to_owned()))
                .expect("API client");
        let before: ApiResponse<MeResponse> = get(&client).await.expect("GET /me");
        let body: UpdateNameRequest<'_> = UpdateNameRequest { name: "New name" };
        let after: ApiResponse<MeResponse> =
            client.patch("api/v1/me", &body).await.expect("PATCH /me");
        server.await.expect("API server result");
        assert_eq!(before.value.user.name, "Old name");
        assert_eq!(after.value.user.name, "New name");
        assert_eq!(after.request_id.as_deref(), Some("req-personal"));
    }
}
