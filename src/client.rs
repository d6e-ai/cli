use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::error::CliError;

pub struct ApiClient {
    http: reqwest::Client,
    auth_url: Url,
    access_token: SecretString,
}

pub struct ApiResponse<T> {
    pub value: T,
    pub request_id: Option<String>,
    pub etag: Option<String>,
}

#[derive(serde::Deserialize)]
struct ErrorResponse {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
}

pub fn normalize_auth_url(mut auth_url: Url) -> Result<Url, CliError> {
    let is_secure: bool = auth_url.scheme() == "https";
    let is_local_http: bool =
        auth_url.scheme() == "http" && matches!(auth_url.host_str(), Some("127.0.0.1" | "[::1]"));
    if (!is_secure && !is_local_http)
        || auth_url.cannot_be_a_base()
        || auth_url.query().is_some()
        || auth_url.fragment().is_some()
        || !auth_url.username().is_empty()
        || auth_url.password().is_some()
        || !matches!(auth_url.path(), "" | "/")
    {
        return Err(CliError::InvalidInput {
            message: "--auth-url must be an HTTPS origin or a loopback HTTP origin".to_owned(),
        });
    }
    auth_url.set_path("/");
    Ok(auth_url)
}

pub fn http_client() -> Result<reqwest::Client, CliError> {
    let http: reqwest::Client = reqwest::Client::builder()
        .user_agent(concat!("d6e-cli/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    Ok(http)
}

pub async fn decode_response<T>(response: Response) -> Result<ApiResponse<T>, CliError>
where
    T: DeserializeOwned,
{
    let status: StatusCode = response.status();
    let header_request_id: Option<String> = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let etag: Option<String> = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);

    if status.is_success() {
        let value: T = response.json().await?;
        return Ok(ApiResponse {
            value,
            request_id: header_request_id,
            etag,
        });
    }

    let error_body: Option<ErrorResponse> = response.json().await.ok();
    let code: String = error_body
        .as_ref()
        .and_then(|body| body.error.clone())
        .unwrap_or_else(|| status.canonical_reason().unwrap_or("http_error").to_owned());
    let message: String = error_body
        .as_ref()
        .and_then(|body| body.message.clone())
        .unwrap_or_else(|| format!("D6E Auth returned HTTP {}", status.as_u16()));
    let request_id: Option<String> = error_body
        .and_then(|body| body.request_id)
        .or(header_request_id);

    Err(CliError::Api {
        status,
        code,
        message,
        request_id,
    })
}

impl ApiClient {
    pub fn new(auth_url: Url, access_token: SecretString) -> Result<Self, CliError> {
        Ok(Self {
            http: http_client()?,
            auth_url: normalize_auth_url(auth_url)?,
            access_token,
        })
    }

    pub async fn get<T>(&self, path: &str) -> Result<ApiResponse<T>, CliError>
    where
        T: DeserializeOwned,
    {
        self.send::<(), T>(Method::GET, path, None, None).await
    }

    pub async fn patch<B, T>(&self, path: &str, body: &B) -> Result<ApiResponse<T>, CliError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.send(Method::PATCH, path, Some(body), None).await
    }

    pub async fn post<B, T>(&self, path: &str, body: &B) -> Result<ApiResponse<T>, CliError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.send(Method::POST, path, Some(body), None).await
    }

    pub async fn patch_if_match<B, T>(
        &self,
        path: &str,
        body: &B,
        etag: &str,
    ) -> Result<ApiResponse<T>, CliError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.send(Method::PATCH, path, Some(body), Some(etag)).await
    }

    async fn send<B, T>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        if_match: Option<&str>,
    ) -> Result<ApiResponse<T>, CliError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url: Url = self
            .auth_url
            .join(path)
            .map_err(|_| CliError::InvalidInput {
                message: "could not construct API URL".to_owned(),
            })?;
        let mut request: reqwest::RequestBuilder = self
            .http
            .request(method, url)
            .bearer_auth(self.access_token.expose_secret())
            .header("accept", "application/json");
        if let Some(value) = body {
            request = request.json(value);
        }
        if let Some(value) = if_match {
            request = request.header(reqwest::header::IF_MATCH, value);
        }
        decode_response(request.send().await?).await
    }
}
