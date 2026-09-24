use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use url::Url;

use crate::auth::access_token_from_store;
use crate::cli::{OrganizationCommand, OrganizationProfileCommand, ProfileUpdateFields};
use crate::client::{ApiClient, ApiResponse};
use crate::error::CliError;
use crate::output;
use crate::token_store::TokenStore;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationRecord {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Membership {
    pub role: String,
    pub joined_at: String,
    pub organization: OrganizationRecord,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MembershipsResponse {
    pub memberships: Vec<Membership>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OrganizationResponse {
    pub organization: OrganizationRecord,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OrganizationDetailResponse {
    pub organization: OrganizationRecord,
    pub role: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileValues {
    pub legal_name: Option<String>,
    pub country: Option<String>,
    pub address: Option<Value>,
    pub billing_email: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationProfileResponse {
    pub organization: OrganizationRecord,
    pub profile: ProfileValues,
    pub tax_ids: Vec<Value>,
    pub updated_at: Option<String>,
    pub version: String,
}

#[derive(Serialize)]
struct NameRequest<'a> {
    name: &'a str,
}

fn organization_path(organization_id: &str, suffix: &str) -> Result<String, CliError> {
    let groups: Vec<&str> = organization_id.split('-').collect();
    let lengths: [usize; 5] = [8, 4, 4, 4, 12];
    let valid: bool = groups.len() == lengths.len()
        && groups
            .iter()
            .zip(lengths)
            .all(|(group, length): (&&str, usize)| {
                group.len() == length && group.bytes().all(|byte: u8| byte.is_ascii_hexdigit())
            });
    if !valid {
        return Err(CliError::InvalidInput {
            message: "organization_id must be a UUID".to_owned(),
        });
    }
    Ok(format!("api/v1/organizations/{organization_id}{suffix}"))
}

fn trimmed_name(name: &str) -> Result<&str, CliError> {
    let trimmed: &str = name.trim();
    if trimmed.is_empty() {
        return Err(CliError::InvalidInput {
            message: "name cannot be empty".to_owned(),
        });
    }
    Ok(trimmed)
}

fn insert_nullable_text(
    body: &mut Map<String, Value>,
    key: &str,
    value: Option<String>,
    clear: bool,
    max_length: usize,
) -> Result<(), CliError> {
    if clear {
        body.insert(key.to_owned(), Value::Null);
    } else if let Some(value) = value {
        let trimmed: &str = value.trim();
        if trimmed.is_empty() || trimmed.encode_utf16().count() > max_length {
            return Err(CliError::InvalidInput {
                message: format!("{key} must contain 1 to {max_length} characters"),
            });
        }
        body.insert(key.to_owned(), Value::String(trimmed.to_owned()));
    }
    Ok(())
}

fn profile_patch(fields: ProfileUpdateFields) -> Result<Value, CliError> {
    let mut body: Map<String, Value> = Map::new();
    insert_nullable_text(
        &mut body,
        "legalName",
        fields.legal_name,
        fields.clear_legal_name,
        200,
    )?;
    insert_nullable_text(
        &mut body,
        "country",
        fields.country,
        fields.clear_country,
        2,
    )?;
    if let Some(Value::String(country)) = body.get("country")
        && (country.len() != 2 || !country.bytes().all(|byte: u8| byte.is_ascii_uppercase()))
    {
        return Err(CliError::InvalidInput {
            message: "country must be an uppercase ISO 3166-1 alpha-2 code".to_owned(),
        });
    }
    insert_nullable_text(
        &mut body,
        "billingEmail",
        fields.billing_email,
        fields.clear_billing_email,
        320,
    )?;
    insert_nullable_text(&mut body, "phone", fields.phone, fields.clear_phone, 50)?;

    if fields.clear_address {
        body.insert("address".to_owned(), Value::Null);
    } else {
        let mut address: Map<String, Value> = Map::new();
        insert_nullable_text(
            &mut address,
            "line1",
            fields.address_line1,
            fields.clear_address_line1,
            200,
        )?;
        insert_nullable_text(
            &mut address,
            "line2",
            fields.address_line2,
            fields.clear_address_line2,
            200,
        )?;
        insert_nullable_text(
            &mut address,
            "city",
            fields.address_city,
            fields.clear_address_city,
            200,
        )?;
        insert_nullable_text(
            &mut address,
            "state",
            fields.address_state,
            fields.clear_address_state,
            200,
        )?;
        insert_nullable_text(
            &mut address,
            "postalCode",
            fields.address_postal_code,
            fields.clear_address_postal_code,
            200,
        )?;
        if !address.is_empty() {
            body.insert("address".to_owned(), Value::Object(address));
        }
    }

    if body.is_empty() {
        return Err(CliError::InvalidInput {
            message: "specify at least one profile field to update".to_owned(),
        });
    }
    Ok(Value::Object(body))
}

pub async fn run(
    auth_url: &Url,
    store: &dyn TokenStore,
    command: OrganizationCommand,
) -> Result<(), CliError> {
    let access_token: SecretString = access_token_from_store(auth_url, store).await?;
    let client: ApiClient = ApiClient::new(auth_url.clone(), access_token)?;

    match command {
        OrganizationCommand::List => {
            let response: ApiResponse<MembershipsResponse> =
                client.get("api/v1/organizations").await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        OrganizationCommand::Show { organization_id } => {
            let path: String = organization_path(&organization_id, "")?;
            let response: ApiResponse<OrganizationDetailResponse> = client.get(&path).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        OrganizationCommand::Create { name } => {
            let body: NameRequest<'_> = NameRequest {
                name: trimmed_name(&name)?,
            };
            let response: ApiResponse<OrganizationResponse> =
                client.post("api/v1/organizations", &body).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        OrganizationCommand::Update {
            organization_id,
            name,
        } => {
            let path: String = organization_path(&organization_id, "")?;
            let body: NameRequest<'_> = NameRequest {
                name: trimmed_name(&name)?,
            };
            let response: ApiResponse<OrganizationResponse> = client.patch(&path, &body).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        OrganizationCommand::Profile(args) => run_profile(&client, args.command).await,
    }
}

async fn run_profile(
    client: &ApiClient,
    command: OrganizationProfileCommand,
) -> Result<(), CliError> {
    match command {
        OrganizationProfileCommand::Show { organization_id } => {
            let path: String = organization_path(&organization_id, "/profile")?;
            let response: ApiResponse<OrganizationProfileResponse> = client.get(&path).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
        OrganizationProfileCommand::Update {
            organization_id,
            fields,
        } => {
            let patch: Value = profile_patch(*fields)?;
            let response: ApiResponse<OrganizationProfileResponse> =
                update_profile(client, &organization_id, &patch).await?;
            output::print_data(&response.value, response.request_id.as_deref())
        }
    }
}

async fn update_profile(
    client: &ApiClient,
    organization_id: &str,
    patch: &Value,
) -> Result<ApiResponse<OrganizationProfileResponse>, CliError> {
    let path: String = organization_path(organization_id, "/profile")?;
    let current: ApiResponse<OrganizationProfileResponse> = client.get(&path).await?;
    let etag: String = current
        .etag
        .ok_or(CliError::MissingApiHeader { header: "ETag" })?;
    client.patch_if_match(&path, patch, &etag).await
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use clap::Parser;
    use secrecy::SecretString;
    use serde_json::Value;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use url::Url;

    use super::{
        MembershipsResponse, NameRequest, OrganizationDetailResponse, OrganizationProfileResponse,
        OrganizationResponse, organization_path, profile_patch, update_profile,
    };
    use crate::cli::{Cli, Command, OrganizationCommand, OrganizationProfileCommand};
    use crate::client::{ApiClient, ApiResponse};
    use crate::error::CliError;

    const ORG_ID: &str = "5c7a1ad6-fd92-4c48-b37d-7e62d504c96f";
    const PROFILE_BODY: &str = r#"{"organization":{"id":"5c7a1ad6-fd92-4c48-b37d-7e62d504c96f","name":"Example","status":"active"},"profile":{"legalName":null,"country":null,"address":null,"billingEmail":null,"phone":null},"taxIds":[],"updatedAt":null,"version":"0"}"#;

    fn parse_profile_command(args: &[&str]) -> OrganizationProfileCommand {
        let cli: Cli = Cli::try_parse_from(args).expect("valid command");
        let Command::Organization(organization) = cli.command else {
            panic!("organization command expected")
        };
        let OrganizationCommand::Profile(profile) = organization.command else {
            panic!("profile command expected")
        };
        profile.command
    }

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

    async fn send_response(stream: &mut TcpStream, status: &str, body: &str, etag: Option<&str>) {
        let etag_header: String = etag
            .map(|value| format!("etag: {value}\r\n"))
            .unwrap_or_default();
        let response: String = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\nx-request-id: req-org\r\n{etag_header}content-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("send response");
    }

    async fn test_client() -> (TcpListener, ApiClient) {
        let listener: TcpListener = TcpListener::bind("127.0.0.1:0").await.expect("bind API");
        let address: SocketAddr = listener.local_addr().expect("API address");
        let auth_url: Url = Url::parse(&format!("http://{address}/")).expect("API URL");
        let client: ApiClient =
            ApiClient::new(auth_url, SecretString::from("access-only".to_owned()))
                .expect("API client");
        (listener, client)
    }

    #[test]
    fn profile_patch_preserves_omitted_fields_and_clears_explicit_fields() {
        let command: OrganizationProfileCommand = parse_profile_command(&[
            "d6e",
            "organization",
            "profile",
            "update",
            ORG_ID,
            "--legal-name",
            " Example Ltd. ",
            "--clear-billing-email",
            "--address-line1",
            " 123 Main St ",
            "--clear-address-line2",
        ]);
        let OrganizationProfileCommand::Update { fields, .. } = command else {
            panic!("update expected")
        };
        let patch: Value = profile_patch(*fields).expect("valid patch");
        assert_eq!(
            patch,
            serde_json::json!({
                "legalName": "Example Ltd.",
                "billingEmail": null,
                "address": {"line1": "123 Main St", "line2": null}
            })
        );
        let clear_command: OrganizationProfileCommand = parse_profile_command(&[
            "d6e",
            "organization",
            "profile",
            "update",
            ORG_ID,
            "--clear-address",
        ]);
        let OrganizationProfileCommand::Update { fields, .. } = clear_command else {
            panic!("update expected")
        };
        assert_eq!(
            profile_patch(*fields).expect("clear address patch"),
            serde_json::json!({"address": null})
        );
    }

    #[tokio::test]
    async fn organization_resource_methods_follow_api_contract() {
        let (listener, client): (TcpListener, ApiClient) = test_client().await;
        let server = tokio::spawn(async move {
            let expected: [(&str, &str, &str, &str); 4] = [
                (
                    "GET",
                    "/api/v1/organizations",
                    "",
                    r#"{"memberships":[{"role":"owner","joinedAt":"2026-09-24T00:00:00.000Z","organization":{"id":"5c7a1ad6-fd92-4c48-b37d-7e62d504c96f","name":"Example","status":"active"}}]}"#,
                ),
                (
                    "GET",
                    "/api/v1/organizations/5c7a1ad6-fd92-4c48-b37d-7e62d504c96f",
                    "",
                    r#"{"organization":{"id":"5c7a1ad6-fd92-4c48-b37d-7e62d504c96f","name":"Example","status":"active","createdAt":"2026-09-24T00:00:00.000Z"},"role":"owner"}"#,
                ),
                (
                    "POST",
                    "/api/v1/organizations",
                    r#"{"name":"New organization"}"#,
                    r#"{"organization":{"id":"5c7a1ad6-fd92-4c48-b37d-7e62d504c96f","name":"New organization","status":"active"}}"#,
                ),
                (
                    "PATCH",
                    "/api/v1/organizations/5c7a1ad6-fd92-4c48-b37d-7e62d504c96f",
                    r#"{"name":"Renamed"}"#,
                    r#"{"organization":{"id":"5c7a1ad6-fd92-4c48-b37d-7e62d504c96f","name":"Renamed","status":"active"}}"#,
                ),
            ];
            for (method, path, expected_body, response_body) in expected {
                let (mut stream, _): (TcpStream, SocketAddr) =
                    listener.accept().await.expect("resource request");
                let request: String = read_request(&mut stream).await;
                assert!(request.starts_with(&format!("{method} {path} HTTP/1.1")));
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer access-only")
                );
                let actual_body: &str = request.split("\r\n\r\n").nth(1).unwrap_or("");
                assert_eq!(actual_body, expected_body);
                let status: &str = if method == "POST" {
                    "201 Created"
                } else {
                    "200 OK"
                };
                send_response(&mut stream, status, response_body, None).await;
            }
        });
        let memberships: ApiResponse<MembershipsResponse> =
            client.get("api/v1/organizations").await.expect("list");
        assert_eq!(memberships.value.memberships[0].role, "owner");
        let path: String = organization_path(ORG_ID, "").expect("organization path");
        let detail: ApiResponse<OrganizationDetailResponse> =
            client.get(&path).await.expect("detail");
        assert_eq!(detail.value.role, "owner");
        let create: NameRequest<'_> = NameRequest {
            name: "New organization",
        };
        let created: ApiResponse<OrganizationResponse> = client
            .post("api/v1/organizations", &create)
            .await
            .expect("create");
        assert_eq!(created.value.organization.name, "New organization");
        let rename: NameRequest<'_> = NameRequest { name: "Renamed" };
        let updated: ApiResponse<OrganizationResponse> =
            client.patch(&path, &rename).await.expect("rename");
        assert_eq!(updated.value.organization.name, "Renamed");
        assert_eq!(updated.request_id.as_deref(), Some("req-org"));
        server.await.expect("API server");
    }

    #[tokio::test]
    async fn profile_update_copies_get_etag_into_if_match() {
        let (listener, client): (TcpListener, ApiClient) = test_client().await;
        let server = tokio::spawn(async move {
            let (mut first, _): (TcpStream, SocketAddr) = listener.accept().await.expect("GET");
            let get_request: String = read_request(&mut first).await;
            assert!(get_request.starts_with(&format!(
                "GET /api/v1/organizations/{ORG_ID}/profile HTTP/1.1"
            )));
            assert!(
                get_request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer access-only")
            );
            send_response(
                &mut first,
                "200 OK",
                PROFILE_BODY,
                Some("\"organization-profile-0\""),
            )
            .await;

            let (mut second, _): (TcpStream, SocketAddr) = listener.accept().await.expect("PATCH");
            let patch_request: String = read_request(&mut second).await;
            assert!(patch_request.starts_with(&format!(
                "PATCH /api/v1/organizations/{ORG_ID}/profile HTTP/1.1"
            )));
            assert!(
                patch_request
                    .to_ascii_lowercase()
                    .contains("if-match: \"organization-profile-0\"")
            );
            let body: &str = patch_request.split("\r\n\r\n").nth(1).expect("JSON body");
            let patch: Value = serde_json::from_str(body).expect("JSON patch");
            assert_eq!(patch, serde_json::json!({"legalName":"Example Ltd."}));
            send_response(
                &mut second,
                "200 OK",
                PROFILE_BODY,
                Some("\"organization-profile-2\""),
            )
            .await;
        });
        let command: OrganizationProfileCommand = parse_profile_command(&[
            "d6e",
            "organization",
            "profile",
            "update",
            ORG_ID,
            "--legal-name",
            "Example Ltd.",
        ]);
        let OrganizationProfileCommand::Update { fields, .. } = command else {
            panic!("update expected")
        };
        let patch: Value = profile_patch(*fields).expect("valid patch");
        let response: ApiResponse<OrganizationProfileResponse> =
            update_profile(&client, ORG_ID, &patch)
                .await
                .expect("profile updated");
        assert_eq!(response.request_id.as_deref(), Some("req-org"));
        server.await.expect("API server");
    }

    #[tokio::test]
    async fn profile_update_reports_stale_write_without_retrying() {
        let (listener, client): (TcpListener, ApiClient) = test_client().await;
        let server = tokio::spawn(async move {
            let (mut first, _): (TcpStream, SocketAddr) = listener.accept().await.expect("GET");
            let _: String = read_request(&mut first).await;
            send_response(
                &mut first,
                "200 OK",
                PROFILE_BODY,
                Some("\"organization-profile-0\""),
            )
            .await;
            let (mut second, _): (TcpStream, SocketAddr) = listener.accept().await.expect("PATCH");
            let _: String = read_request(&mut second).await;
            send_response(
                &mut second,
                "409 Conflict",
                r#"{"error":"conflict","message":"Organization profile is stale"}"#,
                None,
            )
            .await;
        });
        let command: OrganizationProfileCommand = parse_profile_command(&[
            "d6e",
            "organization",
            "profile",
            "update",
            ORG_ID,
            "--clear-phone",
        ]);
        let OrganizationProfileCommand::Update { fields, .. } = command else {
            panic!("update expected")
        };
        let patch: Value = profile_patch(*fields).expect("valid patch");
        let error: CliError = update_profile(&client, ORG_ID, &patch)
            .await
            .err()
            .expect("stale write");
        assert_eq!(error.code(), "conflict");
        assert_eq!(error.exit_code(), 6);
        assert_eq!(error.request_id(), Some("req-org"));
        server.await.expect("API server");
    }

    #[tokio::test]
    async fn profile_update_requires_get_etag_and_does_not_patch_without_it() {
        let (listener, client): (TcpListener, ApiClient) = test_client().await;
        let server = tokio::spawn(async move {
            let (mut first, _): (TcpStream, SocketAddr) = listener.accept().await.expect("GET");
            let _: String = read_request(&mut first).await;
            send_response(&mut first, "200 OK", PROFILE_BODY, None).await;
        });
        let command: OrganizationProfileCommand = parse_profile_command(&[
            "d6e",
            "organization",
            "profile",
            "update",
            ORG_ID,
            "--clear-phone",
        ]);
        let OrganizationProfileCommand::Update { fields, .. } = command else {
            panic!("update expected")
        };
        let patch: Value = profile_patch(*fields).expect("valid patch");
        let error: CliError = update_profile(&client, ORG_ID, &patch)
            .await
            .err()
            .expect("ETag missing");
        assert_eq!(error.code(), "api_protocol_error");
        server.await.expect("API server");
    }

    #[test]
    fn organization_paths_reject_path_injection() {
        assert_eq!(
            organization_path(ORG_ID, "/profile").expect("UUID path"),
            format!("api/v1/organizations/{ORG_ID}/profile")
        );
        assert!(organization_path("../me", "").is_err());
        assert!(organization_path(&format!("{ORG_ID}/members"), "").is_err());
    }

    #[test]
    fn contract_profile_response_shape_is_decodable() {
        let profile: OrganizationProfileResponse =
            serde_json::from_str(PROFILE_BODY).expect("profile response");
        assert_eq!(profile.version, "0");
        assert!(profile.profile.legal_name.is_none());
    }

    #[tokio::test]
    async fn profile_precondition_error_preserves_code_and_request_id() {
        let (listener, client): (TcpListener, ApiClient) = test_client().await;
        let server = tokio::spawn(async move {
            let (mut stream, _): (TcpStream, SocketAddr) = listener.accept().await.expect("PATCH");
            let request: String = read_request(&mut stream).await;
            assert!(request.starts_with(&format!(
                "PATCH /api/v1/organizations/{ORG_ID}/profile HTTP/1.1"
            )));
            send_response(
                &mut stream,
                "428 Precondition Required",
                r#"{"error":"precondition_required","message":"If-Match is required"}"#,
                None,
            )
            .await;
        });
        let path: String = organization_path(ORG_ID, "/profile").expect("profile path");
        let result: Result<ApiResponse<OrganizationProfileResponse>, CliError> = client
            .patch_if_match(
                &path,
                &serde_json::json!({"phone": null}),
                "\"organization-profile-0\"",
            )
            .await;
        let error: CliError = result.err().expect("precondition error");
        assert_eq!(error.code(), "precondition_required");
        assert_eq!(error.request_id(), Some("req-org"));
        server.await.expect("API server");
    }
}
