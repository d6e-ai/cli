use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::error::CliError;

/// The CLI only persists the refresh token. Access tokens stay in process memory.
pub trait TokenStore: Send + Sync {
    fn load(&self, auth_url: &Url) -> Result<Option<SecretString>, CliError>;
    fn save(&self, auth_url: &Url, token: &SecretString) -> Result<(), CliError>;
    fn delete(&self, auth_url: &Url) -> Result<(), CliError>;
}

pub struct OsTokenStore;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn entry(auth_url: &Url) -> Result<keyring::Entry, CliError> {
    let account: String = auth_url.origin().ascii_serialization();
    keyring::Entry::new("d6e-cli-refresh-token", &account).map_err(|_| CliError::CredentialStore)
}

impl TokenStore for OsTokenStore {
    fn load(&self, auth_url: &Url) -> Result<Option<SecretString>, CliError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            match entry(auth_url)?.get_password() {
                Ok(token) => Ok(Some(SecretString::from(token))),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(CliError::CredentialStore),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = auth_url;
            Err(CliError::CredentialStore)
        }
    }

    fn save(&self, auth_url: &Url, token: &SecretString) -> Result<(), CliError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            entry(auth_url)?
                .set_password(token.expose_secret())
                .map_err(|_| CliError::CredentialStore)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = (auth_url, token);
            Err(CliError::CredentialStore)
        }
    }

    fn delete(&self, auth_url: &Url) -> Result<(), CliError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            match entry(auth_url)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(CliError::CredentialStore),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = auth_url;
            Err(CliError::CredentialStore)
        }
    }
}
