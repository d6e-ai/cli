use reqwest::StatusCode;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("run `d6e auth login` first")]
    MissingCredential,

    #[error("{message}")]
    InvalidInput { message: String },

    #[error("{message}")]
    Authentication { message: String },

    #[error("the OS credential store is unavailable")]
    CredentialStore,

    #[error("request failed")]
    Transport(#[from] reqwest::Error),

    #[error("workspace creation may have completed; list workspaces before retrying: {0}")]
    WorkspaceCreateOutcomeUnknown(Box<CliError>),

    #[error("D6E Auth response is missing a required header: {header}")]
    MissingApiHeader { header: &'static str },

    #[error("{message}")]
    ApiProtocol { message: String },

    #[error("the Auth Client secret could not be written; the server operation may have completed")]
    SecretPersistence,

    #[error("{message}")]
    Api {
        status: StatusCode,
        code: String,
        message: String,
        request_id: Option<String>,
    },

    #[error("I/O operation failed")]
    Io(#[from] std::io::Error),

    #[error("could not encode output")]
    Serialization(#[from] serde_json::Error),
}

impl CliError {
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::InvalidInput { .. } => 2,
            Self::MissingCredential | Self::Authentication { .. } => 3,
            Self::Api { status, .. } if status.as_u16() == 400 => 2,
            Self::Api { status, .. } if status.as_u16() == 401 => 3,
            Self::Api { status, .. } if status.as_u16() == 403 => 4,
            Self::Api { status, .. } if status.as_u16() == 404 => 5,
            Self::Api { status, .. } if status.as_u16() == 409 => 6,
            Self::WorkspaceCreateOutcomeUnknown(source) => source.exit_code(),
            Self::Api { .. }
            | Self::Transport(_)
            | Self::MissingApiHeader { .. }
            | Self::ApiProtocol { .. } => 7,
            Self::Io(_)
            | Self::Serialization(_)
            | Self::CredentialStore
            | Self::SecretPersistence => 8,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Self::MissingCredential => "authentication_required",
            Self::InvalidInput { .. } => "invalid_input",
            Self::Authentication { .. } => "authentication_failed",
            Self::CredentialStore => "credential_store_error",
            Self::Transport(_) => "network_error",
            Self::WorkspaceCreateOutcomeUnknown(source) => source.code(),
            Self::MissingApiHeader { .. } => "api_protocol_error",
            Self::ApiProtocol { .. } => "api_protocol_error",
            Self::SecretPersistence => "secret_output_error",
            Self::Api { code, .. } => code,
            Self::Io(_) => "io_error",
            Self::Serialization(_) => "serialization_error",
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Api { request_id, .. } => request_id.as_deref(),
            Self::WorkspaceCreateOutcomeUnknown(source) => source.request_id(),
            _ => None,
        }
    }

    pub fn for_workspace_create(self) -> Self {
        if matches!(&self, Self::Transport(_))
            || matches!(&self, Self::Api { status, .. } if status.is_server_error())
        {
            Self::WorkspaceCreateOutcomeUnknown(Box::new(self))
        } else {
            self
        }
    }

    pub fn retryable(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::Api { status, .. } => status.as_u16() == 429 || status.is_server_error(),
            _ => false,
        }
    }
}

#[derive(Serialize)]
pub struct ErrorEnvelope<'a> {
    pub error: ErrorBody<'a>,
}

#[derive(Serialize)]
pub struct ErrorBody<'a> {
    pub code: &'a str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<&'a str>,
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::{CliError, ErrorBody, ErrorEnvelope};

    #[test]
    fn workspace_create_server_failure_is_not_retryable() {
        let error: CliError = CliError::Api {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "service_unavailable".to_owned(),
            message: "try again later".to_owned(),
            request_id: Some("request-123".to_owned()),
        };
        assert!(error.retryable());

        let create_error: CliError = error.for_workspace_create();
        let envelope: ErrorEnvelope<'_> = ErrorEnvelope {
            error: ErrorBody {
                code: create_error.code(),
                message: create_error.to_string(),
                request_id: create_error.request_id(),
                retryable: create_error.retryable(),
            },
        };
        let json: serde_json::Value = serde_json::to_value(envelope).expect("error JSON");
        assert_eq!(json["error"]["code"], "service_unavailable");
        assert_eq!(json["error"]["request_id"], "request-123");
        assert_eq!(json["error"]["retryable"], false);
        assert!(
            json["error"]["message"]
                .as_str()
                .expect("message")
                .contains("list workspaces before retrying")
        );
        assert_eq!(create_error.exit_code(), 7);

        let rate_limit: CliError = CliError::Api {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limited".to_owned(),
            message: "too many requests".to_owned(),
            request_id: None,
        };
        assert!(rate_limit.for_workspace_create().retryable());
    }

    #[test]
    fn workspace_create_transport_failure_is_not_retryable() {
        let transport: reqwest::Error = reqwest::Client::new()
            .get("not a URL")
            .build()
            .expect_err("invalid request URL");
        let error: CliError = CliError::Transport(transport);
        assert!(error.retryable());

        let create_error: CliError = error.for_workspace_create();
        assert_eq!(create_error.code(), "network_error");
        assert!(!create_error.retryable());
        assert_eq!(create_error.exit_code(), 7);
    }
}
