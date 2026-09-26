use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::auth::access_token_from_store;
use crate::cli::{InstanceCommand, InstanceWorkspaceCommand};
use crate::client::{ApiClient, ApiResponse, normalize_origin};
use crate::error::CliError;
use crate::output;
use crate::token_store::TokenStore;

const WORKSPACES_PATH: &str = "api/cli/workspaces";
const EXCHANGE_PATH: &str = "api/v1/auth/instances/exchange";

#[derive(Serialize)]
struct ExchangeRequest<'a> {
    instance_url: &'a str,
}

#[derive(Deserialize)]
struct ExchangeResponse {
    access_token: SecretString,
    token_type: String,
    expires_in: u64,
    instance_url: String,
}

#[derive(Serialize)]
struct CreateWorkspaceRequest<'a> {
    name: &'a str,
}

#[derive(Deserialize, Serialize)]
struct WorkspacesResponse {
    workspaces: Vec<Value>,
}

#[derive(Deserialize, Serialize)]
struct CreateWorkspaceResponse {
    workspace: Value,
    billing: BillingProvisioning,
}

#[derive(Deserialize, Serialize)]
struct BillingProvisioning {
    status: BillingStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum BillingStatus {
    Pending,
    ProvisioningFailed,
    // Unrecognized status still means the workspace was created; don't fail decoding.
    #[serde(other)]
    Unknown,
}

fn validate_exchange_response(
    response: ExchangeResponse,
    instance_origin: &str,
) -> Result<SecretString, CliError> {
    if response.token_type != "Bearer"
        || response.expires_in == 0
        || response.access_token.expose_secret().trim().is_empty()
        || response.instance_url != instance_origin
    {
        return Err(CliError::ApiProtocol {
            message: "D6E Auth returned an invalid instance token exchange response".to_owned(),
        });
    }
    Ok(response.access_token)
}

async fn exchange_instance_token(
    auth_url: &Url,
    cli_access_token: SecretString,
    instance_origin: &str,
) -> Result<SecretString, CliError> {
    let auth_client: ApiClient = ApiClient::new(auth_url.clone(), cli_access_token)?;
    let request: ExchangeRequest<'_> = ExchangeRequest {
        instance_url: instance_origin,
    };
    let response: ApiResponse<ExchangeResponse> = auth_client.post(EXCHANGE_PATH, &request).await?;
    validate_exchange_response(response.value, instance_origin)
}

pub async fn run(
    auth_url: &Url,
    store: &dyn TokenStore,
    instance_url: Option<Url>,
    command: InstanceCommand,
) -> Result<(), CliError> {
    let instance_url: Url = instance_url.ok_or_else(|| CliError::InvalidInput {
        message: "specify --instance-url or D6E_INSTANCE_URL".to_owned(),
    })?;
    let instance_url: Url = normalize_origin(instance_url, "--instance-url")?;
    let instance_origin: String = instance_url.origin().ascii_serialization();
    let InstanceCommand::Workspace(args) = &command;
    if let InstanceWorkspaceCommand::Create { name } = &args.command {
        validate_workspace_name(name)?;
    }
    let cli_access_token: SecretString = access_token_from_store(auth_url, store).await?;
    let instance_access_token: SecretString =
        exchange_instance_token(auth_url, cli_access_token, &instance_origin).await?;
    let instance_client: ApiClient = ApiClient::new(instance_url, instance_access_token)?;

    match command {
        InstanceCommand::Workspace(args) => match args.command {
            InstanceWorkspaceCommand::List => {
                let response: ApiResponse<WorkspacesResponse> =
                    instance_client.get(WORKSPACES_PATH).await?;
                output::print_data(&response.value, response.request_id.as_deref())
            }
            InstanceWorkspaceCommand::Create { name } => {
                let request: CreateWorkspaceRequest<'_> =
                    CreateWorkspaceRequest { name: name.trim() };
                let response: ApiResponse<CreateWorkspaceResponse> = instance_client
                    .post(WORKSPACES_PATH, &request)
                    .await
                    .map_err(CliError::for_workspace_create)?;
                output::print_data(&response.value, response.request_id.as_deref())
            }
        },
    }
}

fn validate_workspace_name(name: &str) -> Result<(), CliError> {
    let trimmed: &str = name.trim();
    if !(2..=255).contains(&trimmed.chars().count()) {
        return Err(CliError::InvalidInput {
            message: "workspace name must contain 2 to 255 characters".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use url::Url;

    use super::{ExchangeResponse, validate_exchange_response};
    use crate::client::normalize_origin;

    #[test]
    fn exchange_requires_the_requested_instance_origin() {
        let requested: Url = Url::parse("https://instance.example/").expect("requested URL");
        let requested_origin: String = requested.origin().ascii_serialization();
        assert_eq!(requested_origin, "https://instance.example");
        let response: ExchangeResponse = ExchangeResponse {
            access_token: SecretString::from("instance-token".to_owned()),
            token_type: "Bearer".to_owned(),
            expires_in: 300,
            instance_url: "https://instance.example/".to_owned(),
        };
        assert!(validate_exchange_response(response, &requested_origin).is_err());

        let matching: ExchangeResponse = ExchangeResponse {
            access_token: SecretString::from("instance-token".to_owned()),
            token_type: "Bearer".to_owned(),
            expires_in: 300,
            instance_url: requested_origin.clone(),
        };
        assert!(validate_exchange_response(matching, &requested_origin).is_ok());
    }

    #[test]
    fn instance_url_requires_secure_or_loopback_origin() {
        let insecure: Url = Url::parse("http://instance.example/").expect("URL");
        let with_path: Url = Url::parse("https://instance.example/path").expect("URL");
        let local: Url = Url::parse("http://127.0.0.1:8080").expect("URL");
        let other_loopback: Url = Url::parse("http://127.0.0.2:8080").expect("URL");
        assert!(normalize_origin(insecure, "--instance-url").is_err());
        assert!(normalize_origin(with_path, "--instance-url").is_err());
        assert!(normalize_origin(other_loopback, "--instance-url").is_err());
        assert_eq!(
            normalize_origin(local, "--instance-url")
                .expect("loopback HTTP")
                .as_str(),
            "http://127.0.0.1:8080/"
        );
    }
}
