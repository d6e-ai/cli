use clap::{Args, Parser, Subcommand};
use url::Url;

#[derive(Debug, Parser)]
#[command(name = "d6e", version, about = "D6E user self-service CLI")]
pub struct Cli {
    /// Base URL of D6E Auth.
    #[arg(
        long,
        global = true,
        env = "D6E_AUTH_URL",
        default_value = "https://www.d6e.ai"
    )]
    pub auth_url: Url,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Sign in and manage the local CLI session.
    Auth(AuthArgs),
    /// Show or update your own personal profile.
    Personal(PersonalArgs),
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Sign in using a browser and PKCE S256.
    Login {
        /// Print the sign-in URL instead of opening a browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Check the current CLI session with D6E Auth.
    Status,
    /// Remove this machine's CLI session.
    Logout,
}

#[derive(Debug, Args)]
pub struct PersonalArgs {
    #[command(subcommand)]
    pub command: PersonalCommand,
}

#[derive(Debug, Subcommand)]
pub enum PersonalCommand {
    /// Show your own name and email address.
    Show,
    /// Update your own display name.
    Update {
        #[arg(long)]
        name: String,
    },
}
