mod deployer;
mod ssh;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::deployer::{
    deploy_existing, deploy_local, install_docker_standalone, install_docker_standalone_local,
    teardown_existing, teardown_local, DeployOptions, LocalDeployOptions,
};

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
    /// Deploy disposable workspace to a target server (or locally with --local)
    Deploy(DeployCommand),
    /// Teardown and destroy an active disposable workspace (or locally with --local)
    Teardown(TeardownCommand),
    /// Install Docker and Docker Compose on a fresh server (or locally with --local)
    InstallDocker(InstallDockerCommand),
}

#[derive(Args, Debug)]
struct DeployCommand {
    #[command(flatten)]
    existing: DeployExistingArgs,
}

#[derive(Args, Debug)]
struct TeardownCommand {
    /// Teardown locally directly on the current machine (without SSH)
    #[arg(long)]
    local: bool,

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
}

#[derive(Args, Debug)]
struct InstallDockerCommand {
    /// Install Docker locally directly on the current machine (without SSH)
    #[arg(long)]
    local: bool,

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
}

#[derive(Args, Debug, Clone)]
struct DeployExistingArgs {
    /// Deploy locally directly on the current machine (without SSH)
    #[arg(long)]
    local: bool,

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

    /// Automatically install Docker and Docker Compose if missing on remote server
    #[arg(long, short = 'i')]
    install_docker: bool,
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
            if teardown_cmd.local {
                let opts = LocalDeployOptions {
                    remote_dir: &teardown_cmd.remote_dir,
                    image: None,
                    install_docker: false,
                };
                teardown_local(&opts)?;
                return Ok(());
            }

            let host = match teardown_cmd.host {
                Some(h) => h,
                None => {
                    anyhow::bail!("Missing required argument: --host <IP_ADDRESS> (or use --local for local teardown)");
                }
            };
            let key_path = match teardown_cmd.key.or_else(resolve_default_key_path) {
                Some(k) => k,
                None => {
                    anyhow::bail!(
                        "No SSH key specified and default ~/.ssh/id_ed25519 or ~/.ssh/id_rsa not found. Please provide --key <PATH>"
                    );
                }
            };
            let opts = DeployOptions {
                host: &host,
                port: teardown_cmd.port,
                user: &teardown_cmd.user,
                key_path: &key_path,
                passphrase: teardown_cmd.passphrase.as_deref(),
                remote_dir: &teardown_cmd.remote_dir,
                image: None,
                install_docker: false,
            };
            teardown_existing(&opts)?;
        }
        Some(Commands::InstallDocker(install_cmd)) => {
            if install_cmd.local {
                install_docker_standalone_local()?;
                return Ok(());
            }

            let host = match install_cmd.host {
                Some(h) => h,
                None => {
                    anyhow::bail!("Missing required argument: --host <IP_ADDRESS> (or use --local for local Docker install)");
                }
            };
            let key_path = match install_cmd.key.or_else(resolve_default_key_path) {
                Some(k) => k,
                None => {
                    anyhow::bail!(
                        "No SSH key specified and default ~/.ssh/id_ed25519 or ~/.ssh/id_rsa not found. Please provide --key <PATH>"
                    );
                }
            };
            let opts = DeployOptions {
                host: &host,
                port: install_cmd.port,
                user: &install_cmd.user,
                key_path: &key_path,
                passphrase: install_cmd.passphrase.as_deref(),
                remote_dir: "/tmp",
                image: None,
                install_docker: true,
            };
            install_docker_standalone(&opts)?;
        }
        None => {
            if let Some(args) = cli.direct_deploy {
                if args.local || args.host.is_some() {
                    execute_deploy(args)?;
                    return Ok(());
                }
            }
            eprintln!("Usage:");
            eprintln!("  Remote Mode (from laptop via SSH):");
            eprintln!("    disposable-cli deploy --existing --host <IP> --key <PATH_TO_KEY>");
            eprintln!("    disposable-cli install-docker --host <IP> --key <PATH_TO_KEY>");
            eprintln!("    disposable-cli teardown --host <IP> --key <PATH_TO_KEY>");
            eprintln!();
            eprintln!("  Local Mode (directly on server):");
            eprintln!("    disposable-cli deploy --local");
            eprintln!("    disposable-cli install-docker --local");
            eprintln!("    disposable-cli teardown --local");
            eprintln!();
            eprintln!("Run 'disposable-cli --help' for details.");
        }
    }

    Ok(())
}

fn execute_deploy(args: DeployExistingArgs) -> Result<()> {
    if args.local {
        let opts = LocalDeployOptions {
            remote_dir: &args.remote_dir,
            image: args.image.as_deref(),
            install_docker: args.install_docker,
        };
        deploy_local(&opts)?;
        return Ok(());
    }

    let host = match args.host {
        Some(h) => h,
        None => {
            anyhow::bail!("Missing required argument: --host <IP_ADDRESS> (or use --local to deploy directly on this server)");
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
        install_docker: args.install_docker,
    };

    deploy_existing(&opts)?;
    Ok(())
}
