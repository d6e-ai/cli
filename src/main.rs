mod auth;
mod cli;
mod client;
#[cfg(test)]
mod contract;
mod error;
mod organization;
mod output;
mod personal;
mod token_store;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::error::CliError;
use crate::token_store::OsTokenStore;

#[tokio::main]
async fn main() -> ExitCode {
    let cli: Cli = Cli::parse();
    let store: OsTokenStore = OsTokenStore;

    match run(cli, &store).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            output::print_error(&error);
            ExitCode::from(error.exit_code())
        }
    }
}

async fn run(cli: Cli, store: &dyn token_store::TokenStore) -> Result<(), CliError> {
    let auth_url: url::Url = client::normalize_auth_url(cli.auth_url)?;
    match cli.command {
        Command::Auth(args) => auth::run(&auth_url, store, args.command).await,
        Command::Personal(args) => personal::run(&auth_url, store, args.command).await,
        Command::Organization(args) => organization::run(&auth_url, store, args.command).await,
    }
}
