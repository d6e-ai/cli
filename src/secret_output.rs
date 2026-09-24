use std::io::Write;

#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::path::{Path, PathBuf};

use secrecy::ExposeSecret;
use serde::Serialize;

use crate::auth_client::{AuthClientMetadata, AuthClientSecretResponse};
use crate::error::CliError;
use crate::output::{ResponseMeta, SuccessEnvelope};

pub enum PreparedSecretOutput {
    Stdout,
    #[cfg(unix)]
    File {
        file: File,
        path: PathBuf,
        retain: bool,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretData<'a> {
    client: &'a AuthClientMetadata,
    client_id: &'a str,
    client_secret: &'a str,
}

#[cfg(unix)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretFileData<'a> {
    client_id: &'a str,
    client_secret: &'a str,
}

#[cfg(unix)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileResult<'a> {
    client: &'a AuthClientMetadata,
    secret_output: &'a Path,
}

impl PreparedSecretOutput {
    pub fn prepare(value: &str) -> Result<Self, CliError> {
        if value == "-" {
            return Ok(Self::Stdout);
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

            let path: PathBuf = PathBuf::from(value);
            let file: File = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            let mut output: Self = Self::File {
                file,
                path,
                retain: false,
            };
            if let Self::File { file, .. } = &mut output {
                file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            }
            Ok(output)
        }

        #[cfg(not(unix))]
        {
            let _unused: &str = value;
            Err(CliError::InvalidInput {
                message: "file secret output is unavailable on this platform; use --secret-output - explicitly"
                    .to_owned(),
            })
        }
    }

    pub fn emit(
        mut self,
        response: &AuthClientSecretResponse,
        request_id: Option<&str>,
    ) -> Result<(), CliError> {
        match &mut self {
            Self::Stdout => {
                let data: SecretData<'_> = SecretData {
                    client: &response.client,
                    client_id: &response.client_id,
                    client_secret: response.client_secret.expose_secret(),
                };
                let envelope: SuccessEnvelope<'_, SecretData<'_>> = SuccessEnvelope {
                    data: &data,
                    meta: ResponseMeta {
                        request_id,
                        api_version: "v1",
                    },
                };
                let stdout: std::io::Stdout = std::io::stdout();
                let mut writer: std::io::StdoutLock<'_> = stdout.lock();
                write_json_line(&mut writer, &envelope)?;
            }
            #[cfg(unix)]
            Self::File {
                file, path, retain, ..
            } => {
                // Once the API has returned a secret, keep the file even if a
                // later write fails. It may contain the only recoverable copy.
                *retain = true;
                let data: SecretFileData<'_> = SecretFileData {
                    client_id: &response.client_id,
                    client_secret: response.client_secret.expose_secret(),
                };
                write_json_line(file, &data)?;
                file.sync_all().map_err(|_| CliError::SecretPersistence)?;

                let result: FileResult<'_> = FileResult {
                    client: &response.client,
                    secret_output: path.as_path(),
                };
                crate::output::print_data(&result, request_id)?;
            }
        }
        Ok(())
    }
}

fn write_json_line<T>(writer: &mut impl Write, data: &T) -> Result<(), CliError>
where
    T: Serialize,
{
    serde_json::to_writer(&mut *writer, data).map_err(|_| CliError::SecretPersistence)?;
    writer
        .write_all(b"\n")
        .map_err(|_| CliError::SecretPersistence)
}

impl Drop for PreparedSecretOutput {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Self::File {
            file,
            path,
            retain: false,
            ..
        } = self
        {
            use std::os::unix::fs::MetadataExt;

            if let (Ok(opened), Ok(current)) = (file.metadata(), std::fs::symlink_metadata(&*path))
                && opened.dev() == current.dev()
                && opened.ino() == current.ino()
            {
                let _result: Result<(), std::io::Error> = std::fs::remove_file(&*path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::fs;

    use secrecy::SecretString;
    use serde_json::Value;
    #[cfg(unix)]
    use tempfile::tempdir;

    #[cfg(unix)]
    use super::PreparedSecretOutput;
    use super::{SecretData, write_json_line};
    use crate::auth_client::{AuthClientMetadata, AuthClientSecretResponse};
    #[cfg(unix)]
    use crate::error::CliError;

    fn fake_response() -> AuthClientSecretResponse {
        AuthClientSecretResponse {
            client: AuthClientMetadata {
                id: "5c7a1ad6-fd92-4c48-b37d-7e62d504c96f".to_owned(),
                organization_id: "f58b3dc3-323a-4546-b3ca-567f43b7a032".to_owned(),
                client_id: "d6e_0123456789abcdef0123456789abcdef".to_owned(),
                name: "Example".to_owned(),
                redirect_uris: Vec::new(),
                allowed_email_domains: None,
                status: "active".to_owned(),
                created_at: "2026-09-24T00:00:00.000Z".to_owned(),
                updated_at: "2026-09-24T00:00:00.000Z".to_owned(),
            },
            client_id: "d6e_0123456789abcdef0123456789abcdef".to_owned(),
            client_secret: SecretString::from("fake-one-time-secret".to_owned()),
        }
    }

    #[cfg(unix)]
    #[test]
    fn uncommitted_reservation_is_removed_and_existing_file_is_not_overwritten() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("secret.json");
        let destination: &str = path.to_str().expect("UTF-8 path");
        let output: PreparedSecretOutput =
            PreparedSecretOutput::prepare(destination).expect("reserve file");
        assert!(path.exists());
        drop(output);
        assert!(!path.exists());

        fs::write(&path, "existing content").expect("existing file");
        assert!(PreparedSecretOutput::prepare(destination).is_err());
        assert_eq!(
            fs::read_to_string(&path).expect("read file"),
            "existing content"
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_sink_uses_owner_only_mode_and_writes_one_time_secret() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("secret.json");
        let output: PreparedSecretOutput =
            PreparedSecretOutput::prepare(path.to_str().expect("UTF-8 path"))
                .expect("reserve file");
        let mode: u32 = fs::metadata(&path)
            .expect("file metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        output
            .emit(&fake_response(), Some("request-1"))
            .expect("save secret");
        let contents: String = fs::read_to_string(&path).expect("saved secret");
        let document: Value = serde_json::from_str(&contents).expect("secret JSON");
        assert_eq!(document["clientId"], "d6e_0123456789abcdef0123456789abcdef");
        assert_eq!(document["clientSecret"], "fake-one-time-secret");
        assert!(document.get("client").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn failed_secret_write_keeps_reserved_file_for_recovery() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("secret.json");
        fs::write(&path, "").expect("file fixture");
        let file: fs::File = fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .expect("read-only descriptor");
        let output: PreparedSecretOutput = PreparedSecretOutput::File {
            file,
            path: path.clone(),
            retain: false,
        };
        let error: CliError = output
            .emit(&fake_response(), None)
            .expect_err("write should fail");
        assert_eq!(error.code(), "secret_output_error");
        assert!(path.exists(), "response may contain the only secret copy");
    }

    #[test]
    fn explicit_stdout_document_contains_secret() {
        let response: AuthClientSecretResponse = fake_response();
        let data: SecretData<'_> = SecretData {
            client: &response.client,
            client_id: &response.client_id,
            client_secret: "fake-one-time-secret",
        };
        let mut writer: Vec<u8> = Vec::new();
        write_json_line(&mut writer, &data).expect("JSON output");
        let document: Value = serde_json::from_slice(&writer).expect("JSON document");
        assert_eq!(document["clientSecret"], "fake-one-time-secret");
    }
}
