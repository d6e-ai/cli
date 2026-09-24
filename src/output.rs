use serde::Serialize;

use crate::error::{CliError, ErrorBody, ErrorEnvelope};

#[derive(Serialize)]
pub struct SuccessEnvelope<'a, T> {
    pub data: &'a T,
    pub meta: ResponseMeta<'a>,
}

#[derive(Serialize)]
pub struct ResponseMeta<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<&'a str>,
    pub api_version: &'static str,
}

pub fn print_data<T>(data: &T, request_id: Option<&str>) -> Result<(), CliError>
where
    T: Serialize,
{
    let envelope: SuccessEnvelope<'_, T> = SuccessEnvelope {
        data,
        meta: ResponseMeta {
            request_id,
            api_version: "v1",
        },
    };
    serde_json::to_writer(std::io::stdout().lock(), &envelope)?;
    println!();
    Ok(())
}

pub fn print_error(error: &CliError) {
    let envelope: ErrorEnvelope<'_> = ErrorEnvelope {
        error: ErrorBody {
            code: error.code(),
            message: error.to_string(),
            request_id: error.request_id(),
            retryable: error.retryable(),
        },
    };

    if serde_json::to_writer(std::io::stderr().lock(), &envelope).is_ok() {
        eprintln!();
    } else {
        eprintln!(r#"{{"error":{{"code":"output_error","retryable":false}}}}"#);
    }
}
