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
    /// Manage organizations you belong to.
    Organization(Box<OrganizationArgs>),
}

#[derive(Debug, Args)]
pub struct OrganizationArgs {
    #[command(subcommand)]
    pub command: OrganizationCommand,
}

#[derive(Debug, Subcommand)]
pub enum OrganizationCommand {
    /// List your current organization memberships.
    List,
    /// Show one organization.
    Show { organization_id: String },
    /// Create an organization and become its owner.
    Create {
        #[arg(long)]
        name: String,
    },
    /// Change an organization's name.
    Update {
        organization_id: String,
        #[arg(long)]
        name: String,
    },
    /// Read or update the organization's billing profile.
    Profile(Box<OrganizationProfileArgs>),
}

#[derive(Debug, Args)]
pub struct OrganizationProfileArgs {
    #[command(subcommand)]
    pub command: OrganizationProfileCommand,
}

#[derive(Debug, Subcommand)]
pub enum OrganizationProfileCommand {
    /// Show the profile, including its current version and tax IDs.
    Show { organization_id: String },
    /// Patch only specified profile fields using the latest ETag.
    Update {
        organization_id: String,
        #[command(flatten)]
        fields: Box<ProfileUpdateFields>,
    },
}

#[derive(Debug, Args)]
pub struct ProfileUpdateFields {
    #[arg(long, conflicts_with = "clear_legal_name")]
    pub legal_name: Option<String>,
    #[arg(long)]
    pub clear_legal_name: bool,
    #[arg(long, conflicts_with = "clear_country")]
    pub country: Option<String>,
    #[arg(long)]
    pub clear_country: bool,
    #[arg(long, conflicts_with = "clear_billing_email")]
    pub billing_email: Option<String>,
    #[arg(long)]
    pub clear_billing_email: bool,
    #[arg(long, conflicts_with = "clear_phone")]
    pub phone: Option<String>,
    #[arg(long)]
    pub clear_phone: bool,
    #[arg(long, conflicts_with_all = ["address_line1", "address_line2", "address_city", "address_state", "address_postal_code", "clear_address_line1", "clear_address_line2", "clear_address_city", "clear_address_state", "clear_address_postal_code"])]
    pub clear_address: bool,
    #[arg(long, conflicts_with = "clear_address_line1")]
    pub address_line1: Option<String>,
    #[arg(long)]
    pub clear_address_line1: bool,
    #[arg(long, conflicts_with = "clear_address_line2")]
    pub address_line2: Option<String>,
    #[arg(long)]
    pub clear_address_line2: bool,
    #[arg(long, conflicts_with = "clear_address_city")]
    pub address_city: Option<String>,
    #[arg(long)]
    pub clear_address_city: bool,
    #[arg(long, conflicts_with = "clear_address_state")]
    pub address_state: Option<String>,
    #[arg(long)]
    pub clear_address_state: bool,
    #[arg(long, conflicts_with = "clear_address_postal_code")]
    pub address_postal_code: Option<String>,
    #[arg(long)]
    pub clear_address_postal_code: bool,
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
