mod deployer;
mod ssh;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::deployer::{deploy_existing, teardown_existing, DeployOptions};

#[derive(Parser, Debug)]
#[command(
    name = "disposable-cli",
    about = "Automated control plane for on-demand Disposable Service Workspaces",
    version = "0.1.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    direct_deploy: Option<DeployExistingArgs>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Deploy disposable workspace to a target server
    Deploy(DeployCommand),
    /// Teardown and destroy an active disposable workspace
    Teardown(TeardownCommand),
}

#[derive(Args, Debug)]
struct DeployCommand {
    #[command(flatten)]
    existing: DeployExistingArgs,
}

#[derive(Args, Debug)]
struct TeardownCommand {
    /// Target host IP or hostname
    #[arg(short = 'H', long)]
    host: String,

    /// SSH port
    #[arg(short = 'p', long, default_value = "22")]
    port: u16,

    /// SSH username
    #[arg(short = 'u', long, default_value = "root")]
    user: String,

    /// Path to private SSH key
    #[arg(short = 'k', long)]
    key: PathBuf,

    /// Optional SSH key passphrase
    #[arg(long)]
    passphrase: Option<String>,

    /// Remote directory for workspace deployment
    #[arg(long, default_value = "/opt/disposable-service")]
    remote_dir: String,
}

#[derive(Args, Debug, Clone)]
struct DeployExistingArgs {
    /// Deploy to an existing Linux instance via SSH
    #[arg(long, default_value = "true")]
    existing: bool,

    /// Target host IP or hostname
    #[arg(short = 'H', long)]
    host: Option<String>,

    /// SSH port
    #[arg(short = 'p', long, default_value = "22")]
    port: u16,

    /// SSH username
    #[arg(short = 'u', long, default_value = "root")]
    user: String,

    /// Path to private SSH key
    #[arg(short = 'k', long)]
    key: Option<PathBuf>,

    /// Optional SSH key passphrase
    #[arg(long)]
    passphrase: Option<String>,

    /// Remote directory for workspace deployment
    #[arg(long, default_value = "/opt/disposable-service")]
    remote_dir: String,

    /// Override container image
    #[arg(long)]
    image: Option<String>,
}

fn resolve_default_key_path() -> Option<PathBuf> {
    if let Some(home) = dirs_home() {
        let ed25519 = home.join(".ssh/id_ed25519");
        if ed25519.exists() {
            return Some(ed25519);
        }
        let rsa = home.join(".ssh/id_rsa");
        if rsa.exists() {
            return Some(rsa);
        }
    }
    None
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Deploy(deploy_cmd)) => {
            let args = deploy_cmd.existing;
            execute_deploy(args)?;
        }
        Some(Commands::Teardown(teardown_cmd)) => {
            let opts = DeployOptions {
                host: &teardown_cmd.host,
                port: teardown_cmd.port,
                user: &teardown_cmd.user,
                key_path: &teardown_cmd.key,
                passphrase: teardown_cmd.passphrase.as_deref(),
                remote_dir: &teardown_cmd.remote_dir,
                image: None,
            };
            teardown_existing(&opts)?;
        }
        None => {
            if let Some(args) = cli.direct_deploy {
                if args.host.is_some() {
                    execute_deploy(args)?;
                    return Ok(());
                }
            }
            eprintln!("Usage: disposable-cli deploy --existing --host <IP> --key <PATH_TO_KEY>");
            eprintln!("Run 'disposable-cli --help' for details.");
        }
    }

    Ok(())
}

fn execute_deploy(args: DeployExistingArgs) -> Result<()> {
    let host = match args.host {
        Some(h) => h,
        None => {
            anyhow::bail!("Missing required argument: --host <IP_ADDRESS>");
        }
    };

    let key_path = match args.key.or_else(resolve_default_key_path) {
        Some(k) => k,
        None => {
            anyhow::bail!(
                "No SSH key specified and default ~/.ssh/id_ed25519 or ~/.ssh/id_rsa not found. Please provide --key <PATH>"
            );
        }
    };

    let opts = DeployOptions {
        host: &host,
        port: args.port,
        user: &args.user,
        key_path: &key_path,
        passphrase: args.passphrase.as_deref(),
        remote_dir: &args.remote_dir,
        image: args.image.as_deref(),
    };

    deploy_existing(&opts)?;
    Ok(())
}
