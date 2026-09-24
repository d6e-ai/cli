use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use url::Url;

use crate::cli::{AuthClientCommand, AuthClientStatus};
use crate::client::{ApiClient, ApiResponse};
use crate::error::CliError;
use crate::organization::organization_path;
use crate::output;
use crate::secret_output::PreparedSecretOutput;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthClientMetadata {
    pub id: String,
    pub organization_id: String,
    pub client_id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub allowed_email_domains: Option<Vec<String>>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize, Serialize)]
pub struct AuthClientsResponse {
    pub clients: Vec<AuthClientMetadata>,
}

#[derive(Deserialize, Serialize)]
pub struct AuthClientResponse {
    pub client: AuthClientMetadata,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthClientSecretResponse {
    pub client: AuthClientMetadata,
    pub client_id: String,
    pub client_secret: SecretString,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateAuthClientRequest<'a> {
    name: &'a str,
    redirect_uris: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_email_domains: Option<&'a [String]>,
    status: &'a str,
}

#[derive(Serialize)]
struct EmptyRequest {}

fn auth_client_path(organization_id: &str, client_id: Option<&str>) -> Result<String, CliError> {
    let mut path: String = organization_path(organization_id, "/auth-clients")?;
    if let Some(client_id) = client_id {
        validate_client_id(client_id)?;
        path.push('/');
        path.push_str(client_id);
    }
    Ok(path)
}

fn validate_client_id(value: &str) -> Result<(), CliError> {
    let groups: Vec<&str> = value.split('-').collect();
    let lengths: [usize; 5] = [8, 4, 4, 4, 12];
    let is_uuid: bool = groups.len() == lengths.len()
        && groups
            .iter()
            .zip(lengths)
            .all(|(group, length): (&&str, usize)| {
                group.len() == length && group.bytes().all(|byte: u8| byte.is_ascii_hexdigit())
            });
    let is_generated_id: bool = value.strip_prefix("d6e_").is_some_and(|suffix: &str| {
        suffix.len() == 32 && suffix.bytes().all(|byte: u8| byte.is_ascii_hexdigit())
    });
    if !is_uuid && !is_generated_id {
        return Err(CliError::InvalidInput {
            message: "client_id must be a UUID or d6e_ client ID".to_owned(),
        });
    }
    Ok(())
}

fn non_empty_name(value: &str) -> Result<&str, CliError> {
    let trimmed: &str = value.trim();
    if trimmed.is_empty() || trimmed.encode_utf16().count() > 100 {
        return Err(CliError::InvalidInput {
            message: "name must contain 1 to 100 characters".to_owned(),
        });
    }
    Ok(trimmed)
}

fn redirect_uri_strings(uris: Vec<String>) -> Result<Vec<String>, CliError> {
    let mut values: Vec<String> = Vec::with_capacity(uris.len());
    for raw in uris {
        let value: &str = raw.trim();
        let uri: Url = Url::parse(value).map_err(|_| CliError::InvalidInput {
            message: "redirect URI must be an absolute HTTP or HTTPS URL".to_owned(),
        })?;
        let scheme: &str = uri.scheme();
        let host: Option<&str> = uri.host_str();
        let loopback: bool = matches!(host, Some("localhost" | "127.0.0.1" | "[::1]" | "::1"));
        if (scheme != "http" && scheme != "https") || (scheme == "http" && !loopback) {
            return Err(CliError::InvalidInput {
                message: "redirect URI must use HTTPS or loopback HTTP".to_owned(),
            });
        }
        // d6e-auth stores the trimmed input verbatim; OAuth redirect matching is exact.
        values.push(value.to_owned());
    }
    Ok(values)
}

fn update_body(
    name: Option<String>,
    redirect_uris: Vec<String>,
    clear_redirect_uris: bool,
    allowed_email_domains: Vec<String>,
    clear_allowed_email_domains: bool,
    status: Option<AuthClientStatus>,
) -> Result<Value, CliError> {
    let mut body: Map<String, Value> = Map::new();
    if let Some(name) = name {
        body.insert(
            "name".to_owned(),
            Value::String(non_empty_name(&name)?.to_owned()),
        );
    }
    if clear_redirect_uris || !redirect_uris.is_empty() {
        let values: Vec<String> = if clear_redirect_uris {
            Vec::new()
        } else {
            redirect_uri_strings(redirect_uris)?
        };
        body.insert("redirectUris".to_owned(), serde_json::to_value(values)?);
    }
    if clear_allowed_email_domains {
        body.insert("allowedEmailDomains".to_owned(), Value::Null);
    } else if !allowed_email_domains.is_empty() {
        body.insert(
            "allowedEmailDomains".to_owned(),
            serde_json::to_value(allowed_email_domains)?,
        );
    }
    if let Some(status) = status {
        body.insert(
            "status".to_owned(),
            Value::String(status.as_str().to_owned()),
        );
    }
    if body.is_empty() {
        return Err(CliError::InvalidInput {
            message: "specify at least one Auth Client field to update".to_owned(),
        });
    }
    Ok(Value::Object(body))
}

pub async fn run(client: &ApiClient, command: AuthClientCommand) -> Result<(), CliError> {
    match command {
        AuthClientCommand::List { organization_id } => {
            let path: String = auth_client_path(&organization_id, None)?;
            let response: ApiResponse<AuthClientsResponse> = client.get(&path).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        AuthClientCommand::Show {
            organization_id,
            client_id,
        } => {
            let path: String = auth_client_path(&organization_id, Some(&client_id))?;
            let response: ApiResponse<AuthClientResponse> = client.get(&path).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        AuthClientCommand::Create {
            organization_id,
            name,
            redirect_uris,
            allowed_email_domains,
            status,
            secret_output,
        } => {
            let path: String = auth_client_path(&organization_id, None)?;
            let redirects: Vec<String> = redirect_uri_strings(redirect_uris)?;
            let name: &str = non_empty_name(&name)?;
            let domains: Option<&[String]> = if allowed_email_domains.is_empty() {
                None
            } else {
                Some(&allowed_email_domains)
            };
            let body: CreateAuthClientRequest<'_> = CreateAuthClientRequest {
                name,
                redirect_uris: &redirects,
                allowed_email_domains: domains,
                status: status.as_str(),
            };
            let sink: PreparedSecretOutput = PreparedSecretOutput::prepare(&secret_output)?;
            let response: ApiResponse<AuthClientSecretResponse> = client.post(&path, &body).await?;
            sink.emit(&response.value, response.request_id.as_deref())
        }
        AuthClientCommand::Update {
            organization_id,
            client_id,
            name,
            redirect_uris,
            clear_redirect_uris,
            allowed_email_domains,
            clear_allowed_email_domains,
            status,
        } => {
            let path: String = auth_client_path(&organization_id, Some(&client_id))?;
            let body: Value = update_body(
                name,
                redirect_uris,
                clear_redirect_uris,
                allowed_email_domains,
                clear_allowed_email_domains,
                status,
            )?;
            let response: ApiResponse<AuthClientResponse> = client.patch(&path, &body).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        AuthClientCommand::Revoke {
            organization_id,
            client_id,
        } => {
            let path: String = auth_client_path(&organization_id, Some(&client_id))?;
            let body: Value = serde_json::json!({ "status": "inactive" });
            let response: ApiResponse<AuthClientResponse> = client.patch(&path, &body).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        AuthClientCommand::RotateSecret {
            organization_id,
            client_id,
            secret_output,
        } => {
            let mut path: String = auth_client_path(&organization_id, Some(&client_id))?;
            path.push_str("/secret");
            let sink: PreparedSecretOutput = PreparedSecretOutput::prepare(&secret_output)?;
            let response: ApiResponse<AuthClientSecretResponse> =
                client.post(&path, &EmptyRequest {}).await?;
            sink.emit(&response.value, response.request_id.as_deref())
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::net::SocketAddr;

    use clap::Parser;
    #[cfg(unix)]
    use secrecy::SecretString;
    use serde_json::Value;
    #[cfg(unix)]
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[cfg(unix)]
    use tokio::net::{TcpListener, TcpStream};
    #[cfg(unix)]
    use url::Url;

    #[cfg(unix)]
    use super::run;
    use super::{AuthClientResponse, auth_client_path, redirect_uri_strings, update_body};
    use crate::cli::Cli;
    #[cfg(unix)]
    use crate::cli::{AuthClientCommand, AuthClientStatus};
    #[cfg(unix)]
    use crate::client::ApiClient;

    const ORG_ID: &str = "f58b3dc3-323a-4546-b3ca-567f43b7a032";
    const CLIENT_ID: &str = "d6e_0123456789abcdef0123456789abcdef";
    const CLIENT_BODY: &str = r#"{"id":"5c7a1ad6-fd92-4c48-b37d-7e62d504c96f","organizationId":"f58b3dc3-323a-4546-b3ca-567f43b7a032","clientId":"d6e_0123456789abcdef0123456789abcdef","name":"Example","redirectUris":[],"allowedEmailDomains":null,"status":"active","createdAt":"2026-09-24T00:00:00.000Z","updatedAt":"2026-09-24T00:00:00.000Z"}"#;

    #[cfg(unix)]
    async fn test_client() -> (TcpListener, ApiClient) {
        let listener: TcpListener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("API listener");
        let address: SocketAddr = listener.local_addr().expect("API address");
        let url: Url = Url::parse(&format!("http://{address}/")).expect("API URL");
        let client: ApiClient =
            ApiClient::new(url, SecretString::from("access-only".to_owned())).expect("API client");
        (listener, client)
    }

    #[cfg(unix)]
    async fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        let mut chunk: [u8; 1024] = [0_u8; 1024];
        loop {
            let count: usize = stream.read(&mut chunk).await.expect("read request");
            assert!(count > 0, "request ended early");
            bytes.extend_from_slice(&chunk[..count]);
            let Some(headers_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers: String = String::from_utf8_lossy(&bytes[..headers_end]).into_owned();
            let content_length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|length| length.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= headers_end + 4 + content_length {
                return String::from_utf8(bytes).expect("UTF-8 request");
            }
        }
    }

    #[cfg(unix)]
    async fn send_response(stream: &mut TcpStream, status: &str, body: &str) {
        let response: String = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\nx-request-id: req-client\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("send response");
    }

    #[test]
    fn path_ids_are_checked_before_url_construction() {
        assert_eq!(
            auth_client_path(ORG_ID, Some(CLIENT_ID)).expect("client path"),
            format!("api/v1/organizations/{ORG_ID}/auth-clients/{CLIENT_ID}")
        );
        assert!(auth_client_path(ORG_ID, Some("../../me")).is_err());
        assert!(auth_client_path(ORG_ID, Some("d6e_bad?x=1")).is_err());
        assert!(auth_client_path(ORG_ID, Some("d6e_bad#fragment")).is_err());
    }

    #[test]
    fn update_body_uses_only_selected_fields_and_revoke_is_exact() {
        let body: Value = update_body(
            Some(" Renamed ".to_owned()),
            Vec::new(),
            true,
            Vec::new(),
            true,
            None,
        )
        .expect("update body");
        assert_eq!(
            body,
            serde_json::json!({
                "name": "Renamed",
                "redirectUris": [],
                "allowedEmailDomains": null
            })
        );
        assert!(update_body(None, Vec::new(), false, Vec::new(), false, None).is_err());
    }

    #[test]
    fn redirect_uri_rejects_non_loopback_plain_http() {
        let remote: String = "http://example.com/callback".to_owned();
        assert!(redirect_uri_strings(vec![remote]).is_err());
        let loopback: String = "http://127.0.0.1:9876/callback".to_owned();
        assert_eq!(
            redirect_uri_strings(vec![loopback]).expect("loopback URI"),
            ["http://127.0.0.1:9876/callback"]
        );
    }

    #[test]
    fn redirect_uri_preserves_exact_trimmed_input() {
        let values: Vec<String> = redirect_uri_strings(vec![
            " https://example.com:443 ".to_owned(),
            "https://example.com/a/../callback".to_owned(),
        ])
        .expect("valid redirect URIs");
        assert_eq!(
            values,
            [
                "https://example.com:443",
                "https://example.com/a/../callback"
            ]
        );
    }

    #[test]
    fn secret_output_is_required_by_cli_parser() {
        assert!(
            Cli::try_parse_from([
                "d6e",
                "organization",
                "auth-client",
                "create",
                ORG_ID,
                "--name",
                "Example"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "d6e",
                "organization",
                "auth-client",
                "rotate-secret",
                ORG_ID,
                CLIENT_ID
            ])
            .is_err()
        );
    }

    #[test]
    fn metadata_output_cannot_serialize_a_secret_field() {
        let body: String =
            format!("{{\"client\":{CLIENT_BODY},\"clientSecret\":\"unexpected-server-field\"}}");
        let response: AuthClientResponse = serde_json::from_str(&body).expect("metadata response");
        let output: String = serde_json::to_string(&response).expect("metadata JSON");
        assert!(!output.contains("clientSecret"));
        assert!(!output.contains("unexpected-server-field"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn existing_secret_file_blocks_create_before_api_mutation() {
        use std::fs;
        use std::time::Duration;

        use tempfile::tempdir;

        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("keep.json");
        fs::write(&path, "keep").expect("existing file");
        let (listener, client): (TcpListener, ApiClient) = test_client().await;
        let result = run(
            &client,
            AuthClientCommand::Create {
                organization_id: ORG_ID.to_owned(),
                name: "Example".to_owned(),
                redirect_uris: Vec::new(),
                allowed_email_domains: Vec::new(),
                status: AuthClientStatus::Active,
                secret_output: path.to_str().expect("UTF-8 path").to_owned(),
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).expect("file content"), "keep");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err(),
            "create request must not be sent"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn all_auth_client_operations_use_contract_paths_bodies_and_safe_file_output() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        use tempfile::tempdir;

        let directory = tempdir().expect("temp directory");
        let create_path = directory.path().join("created.json");
        let rotate_path = directory.path().join("rotated.json");
        let create_path_for_server = create_path.clone();
        let rotate_path_for_server = rotate_path.clone();
        let (listener, client): (TcpListener, ApiClient) = test_client().await;

        let server = tokio::spawn(async move {
            let base: String = format!("/api/v1/organizations/{ORG_ID}/auth-clients");
            for step in 0..6 {
                let (mut stream, _): (TcpStream, SocketAddr) =
                    listener.accept().await.expect("Auth Client request");
                let request: String = read_request(&mut stream).await;
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer access-only")
                );
                let request_line: &str = request.lines().next().expect("request line");
                let body: &str = request.split("\r\n\r\n").nth(1).unwrap_or("");
                let (expected_line, expected_body, response_body, status): (
                    String,
                    Option<Value>,
                    String,
                    &str,
                ) = match step {
                    0 => (
                        format!("GET {base} HTTP/1.1"),
                        None,
                        format!("{{\"clients\":[{CLIENT_BODY}]}}"),
                        "200 OK",
                    ),
                    1 => (
                        format!("GET {base}/{CLIENT_ID} HTTP/1.1"),
                        None,
                        format!("{{\"client\":{CLIENT_BODY}}}"),
                        "200 OK",
                    ),
                    2 => {
                        assert!(create_path_for_server.exists(), "sink prepared before POST");
                        let mode: u32 = fs::metadata(&create_path_for_server)
                            .expect("reserved file")
                            .permissions()
                            .mode()
                            & 0o777;
                        assert_eq!(mode, 0o600);
                        (
                            format!("POST {base} HTTP/1.1"),
                            Some(serde_json::json!({
                                "name":"New App",
                                "redirectUris":["https://example.com/callback"],
                                "allowedEmailDomains":["example.com"],
                                "status":"active"
                            })),
                            format!(
                                "{{\"client\":{CLIENT_BODY},\"clientId\":\"{CLIENT_ID}\",\"clientSecret\":\"fake-created-secret\"}}"
                            ),
                            "201 Created",
                        )
                    }
                    3 => (
                        format!("PATCH {base}/{CLIENT_ID} HTTP/1.1"),
                        Some(serde_json::json!({"name":"Renamed","redirectUris":[]})),
                        format!("{{\"client\":{CLIENT_BODY}}}"),
                        "200 OK",
                    ),
                    4 => (
                        format!("PATCH {base}/{CLIENT_ID} HTTP/1.1"),
                        Some(serde_json::json!({"status":"inactive"})),
                        format!("{{\"client\":{CLIENT_BODY}}}"),
                        "200 OK",
                    ),
                    5 => {
                        assert!(rotate_path_for_server.exists(), "sink prepared before POST");
                        (
                            format!("POST {base}/{CLIENT_ID}/secret HTTP/1.1"),
                            Some(serde_json::json!({})),
                            format!(
                                "{{\"client\":{CLIENT_BODY},\"clientId\":\"{CLIENT_ID}\",\"clientSecret\":\"fake-rotated-secret\"}}"
                            ),
                            "200 OK",
                        )
                    }
                    _ => unreachable!(),
                };
                assert_eq!(request_line, expected_line);
                if let Some(expected_body) = expected_body {
                    let actual: Value = serde_json::from_str(body).expect("request JSON");
                    assert_eq!(actual, expected_body);
                } else {
                    assert!(body.is_empty());
                }
                send_response(&mut stream, status, &response_body).await;
            }
        });

        run(
            &client,
            AuthClientCommand::List {
                organization_id: ORG_ID.to_owned(),
            },
        )
        .await
        .expect("list");
        run(
            &client,
            AuthClientCommand::Show {
                organization_id: ORG_ID.to_owned(),
                client_id: CLIENT_ID.to_owned(),
            },
        )
        .await
        .expect("show");
        run(
            &client,
            AuthClientCommand::Create {
                organization_id: ORG_ID.to_owned(),
                name: "New App".to_owned(),
                redirect_uris: vec!["https://example.com/callback".to_owned()],
                allowed_email_domains: vec!["example.com".to_owned()],
                status: AuthClientStatus::Active,
                secret_output: create_path.to_str().expect("UTF-8 path").to_owned(),
            },
        )
        .await
        .expect("create");
        run(
            &client,
            AuthClientCommand::Update {
                organization_id: ORG_ID.to_owned(),
                client_id: CLIENT_ID.to_owned(),
                name: Some("Renamed".to_owned()),
                redirect_uris: Vec::new(),
                clear_redirect_uris: true,
                allowed_email_domains: Vec::new(),
                clear_allowed_email_domains: false,
                status: None,
            },
        )
        .await
        .expect("update");
        run(
            &client,
            AuthClientCommand::Revoke {
                organization_id: ORG_ID.to_owned(),
                client_id: CLIENT_ID.to_owned(),
            },
        )
        .await
        .expect("revoke");
        run(
            &client,
            AuthClientCommand::RotateSecret {
                organization_id: ORG_ID.to_owned(),
                client_id: CLIENT_ID.to_owned(),
                secret_output: rotate_path.to_str().expect("UTF-8 path").to_owned(),
            },
        )
        .await
        .expect("rotate secret");
        server.await.expect("mock API");

        let created: Value = serde_json::from_slice(&fs::read(create_path).expect("created file"))
            .expect("created secret JSON");
        let rotated: Value = serde_json::from_slice(&fs::read(rotate_path).expect("rotated file"))
            .expect("rotated secret JSON");
        assert_eq!(created["clientSecret"], "fake-created-secret");
        assert_eq!(rotated["clientSecret"], "fake-rotated-secret");
    }
}
