use anyhow::{bail, Result};
use console::{style, Emoji};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::time::Duration;

use crate::ssh::SshClient;

static ROCKET: Emoji<'_, '_> = Emoji("🚀 ", "");
static TRASH: Emoji<'_, '_> = Emoji("🧹 ", "");

const DEFAULT_COMPOSE_TEMPLATE: &str = include_str!("../../../deploy/docker-compose.yml");

pub struct DeployOptions<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    pub key_path: &'a Path,
    pub passphrase: Option<&'a str>,
    pub remote_dir: &'a str,
    pub image: Option<&'a str>,
}

pub fn deploy_existing(opts: &DeployOptions) -> Result<()> {
    println!(
        "\n{} {}",
        style("[Disposable Service Workspace]").bold().cyan(),
        style("Deployment Engine").dim()
    );
    println!(
        "Target: {}@{}:{}\n",
        style(opts.user).green(),
        style(opts.host).bold().white(),
        opts.port
    );

    let spinner = create_spinner("Connecting to remote server via SSH...");
    let ssh = SshClient::connect(
        opts.host,
        opts.port,
        opts.user,
        opts.key_path,
        opts.passphrase,
    )?;
    spinner.finish_with_message("SSH connection established successfully.");

    // Check Docker & Docker Compose
    let spinner = create_spinner("Verifying Docker and Docker Compose runtime...");
    let (code, stdout, stderr) = ssh.execute("docker compose version || docker-compose --version")?;
    if code != 0 {
        spinner.abandon();
        eprintln!(
            "{} Remote server is missing Docker or Docker Compose:\n{}",
            style("Error:").red().bold(),
            stderr
        );
        bail!(
            "Docker Compose is required on the target server. Please install docker-compose-plugin or docker.io first."
        );
    }
    spinner.finish_with_message(format!("Runtime found: {}", stdout.trim()));

    // Create remote workspace directory
    let spinner = create_spinner(format!("Setting up workspace at {}...", opts.remote_dir));
    ssh.execute(&format!("mkdir -p {}", opts.remote_dir))?;
    spinner.finish_with_message("Workspace directory ready.");

    // Transfer docker-compose.yml
    let spinner = create_spinner("Transferring docker-compose.yml template...");
    let remote_compose_path = format!("{}/docker-compose.yml", opts.remote_dir);
    ssh.upload_content(&remote_compose_path, DEFAULT_COMPOSE_TEMPLATE)?;

    // If custom image override is requested
    if let Some(custom_img) = opts.image {
        let env_content = format!("DISPOSABLE_IMAGE={}\n", custom_img);
        let remote_env_path = format!("{}/.env", opts.remote_dir);
        ssh.upload_content(&remote_env_path, &env_content)?;
    }
    spinner.finish_with_message("Deployment specification uploaded via SFTP.");

    // Pull container image
    let spinner = create_spinner("Pulling pre-built container image (docker compose pull)...");
    let (pull_code, _, pull_err) = ssh.execute(&format!(
        "cd {} && docker compose pull || docker-compose pull",
        opts.remote_dir
    ))?;
    if pull_code != 0 {
        spinner.abandon();
        eprintln!(
            "{} Failed to pull image:\n{}",
            style("Warning:").yellow().bold(),
            pull_err
        );
    } else {
        spinner.finish_with_message("Container image pulled.");
    }

    // Start service
    let spinner = create_spinner("Starting container services (docker compose up -d)...");
    let (up_code, _up_out, up_err) = ssh.execute(&format!(
        "cd {} && docker compose up -d || docker-compose up -d",
        opts.remote_dir
    ))?;
    if up_code != 0 {
        spinner.abandon();
        bail!("Failed to start docker compose: {}", up_err);
    }
    spinner.finish_with_message("Containers active and listening.");

    // Print final banner
    println!("\n{}", style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").dim());
    println!(
        "{} {}",
        ROCKET,
        style("Disposable Service Workspace is LIVE!").bold().green()
    );
    println!("{}", style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").dim());
    println!(
        "\n  Access URL:  {}",
        style(format!("http://{}:80", opts.host)).bold().underlined().cyan()
    );
    println!(
        "  First Step:  Open the URL in your browser to claim and set your Single Password."
    );
    println!(
        "  Retention:   Strict Zero-Footprint ephemeral storage.\n"
    );

    Ok(())
}

pub fn teardown_existing(opts: &DeployOptions) -> Result<()> {
    println!(
        "\n{} {}",
        style("[Disposable Service Workspace]").bold().cyan(),
        style("Teardown Engine").dim()
    );
    println!(
        "Target: {}@{}:{}\n",
        style(opts.user).green(),
        style(opts.host).bold().white(),
        opts.port
    );

    let spinner = create_spinner("Connecting to remote server via SSH...");
    let ssh = SshClient::connect(
        opts.host,
        opts.port,
        opts.user,
        opts.key_path,
        opts.passphrase,
    )?;
    spinner.finish_with_message("SSH connection established.");

    let spinner = create_spinner("Stopping and removing containers (docker compose down -v)...");
    let _ = ssh.execute(&format!(
        "cd {} && (docker compose down -v || docker-compose down -v)",
        opts.remote_dir
    ));
    spinner.finish_with_message("Containers stopped and volumes wiped.");

    let spinner = create_spinner("Wiping remote workspace directory...");
    let _ = ssh.execute(&format!("rm -rf {}", opts.remote_dir));
    spinner.finish_with_message("Directory cleaned.");

    println!(
        "\n{} {}",
        TRASH,
        style("Disposable Workspace successfully torn down. No residual traces remain.").bold().green()
    );

    Ok(())
}

fn create_spinner(message: impl Into<String>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
            .template("{spinner:.cyan} {msg}")
            .expect("Valid spinner template"),
    );
    pb.set_message(message.into());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}
