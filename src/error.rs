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
            Self::Api { .. } | Self::Transport(_) => 7,
            Self::Io(_) | Self::Serialization(_) | Self::CredentialStore => 8,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Self::MissingCredential => "authentication_required",
            Self::InvalidInput { .. } => "invalid_input",
            Self::Authentication { .. } => "authentication_failed",
            Self::CredentialStore => "credential_store_error",
            Self::Transport(_) => "network_error",
            Self::Api { code, .. } => code,
            Self::Io(_) => "io_error",
            Self::Serialization(_) => "serialization_error",
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Api { request_id, .. } => request_id.as_deref(),
            _ => None,
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
