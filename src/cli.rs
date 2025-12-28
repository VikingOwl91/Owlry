use clap::Parser;

use crate::providers::ProviderType;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "owlry",
    about = "An owl-themed application launcher for Wayland",
    version
)]
pub struct CliArgs {
    /// Start in single-provider mode (app, cmd, uuctl)
    #[arg(long, short = 'm', value_parser = parse_provider)]
    pub mode: Option<ProviderType>,

    /// Comma-separated list of enabled providers (app,cmd,uuctl)
    #[arg(long, short = 'p', value_delimiter = ',', value_parser = parse_provider)]
    pub providers: Option<Vec<ProviderType>>,
}

fn parse_provider(s: &str) -> Result<ProviderType, String> {
    s.parse()
}

impl CliArgs {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
