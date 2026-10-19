use anyhow::{bail, Context, Result};
use console::{style, Emoji};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::time::Duration;

use crate::ssh::SshClient;

static ROCKET: Emoji<'_, '_> = Emoji("🚀 ", "");
static TRASH: Emoji<'_, '_> = Emoji("🧹 ", "");

const DEFAULT_COMPOSE_TEMPLATE: &str = include_str!("../../../deploy/docker-compose.yml");

pub struct LocalDeployOptions<'a> {
    pub remote_dir: &'a str,
    pub image: Option<&'a str>,
    pub install_docker: bool,
}

pub struct DeployOptions<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    pub key_path: &'a Path,
    pub passphrase: Option<&'a str>,
    pub remote_dir: &'a str,
    pub image: Option<&'a str>,
    pub install_docker: bool,
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
    let (code, stdout, stderr) = ssh.execute("docker compose version")?;
    let combined_output = format!("{}\n{}", stdout, stderr);
    if combined_output.contains("Please login as the user") {
        spinner.abandon();
        eprintln!(
            "\n{} Remote host rejected SSH user '{}':\n{}",
            style("Error:").red().bold(),
            opts.user,
            style(combined_output.trim()).yellow().bold()
        );
        bail!(
            "Please re-run specifying the appropriate user (e.g. '--user ubuntu' or '--user ec2-user')."
        );
    }
    let mut ssh = ssh;
    if code != 0 {
        spinner.abandon();

        let should_install = if opts.install_docker {
            println!(
                "\n{} Remote server is missing Docker. Auto-install flag (--install-docker) detected.",
                style("Notice:").cyan().bold()
            );
            true
        } else {
            prompt_install_docker()?
        };

        if should_install {
            install_docker_remote_with_ssh(&ssh, opts.user)?;

            // If non-root user, refresh SSH connection to update supplementary group permissions
            if opts.user != "root" {
                let reconnect_spinner = create_spinner("Refreshing SSH session for group permissions...");
                ssh = SshClient::connect(
                    opts.host,
                    opts.port,
                    opts.user,
                    opts.key_path,
                    opts.passphrase,
                )?;
                reconnect_spinner.finish_with_message("SSH session refreshed.");
            }

            let check_spinner = create_spinner("Verifying installed Docker runtime...");
            let (recheck_code, recheck_out, recheck_err) = ssh.execute("docker compose version")?;
            if recheck_code != 0 {
                check_spinner.abandon();
                bail!(
                    "Docker installation completed but 'docker compose version' failed:\n{}",
                    recheck_err
                );
            }
            check_spinner.finish_with_message(format!("Runtime ready: {}", recheck_out.trim()));
        } else {
            bail!(
                "Docker Compose is required on the target server. Aborting deployment.\nTip: Pass '--install-docker' (or '-i') to install Docker automatically."
            );
        }
    } else {
        spinner.finish_with_message(format!("Runtime found: {}", stdout.trim()));
    }

    // Create remote workspace directory
    let spinner = create_spinner(format!("Setting up workspace at {}...", opts.remote_dir));
    let mkdir_cmd = if opts.user == "root" {
        format!("mkdir -p {}", opts.remote_dir)
    } else {
        format!(
            "sudo mkdir -p {} && sudo chown -R {}:{} {}",
            opts.remote_dir, opts.user, opts.user, opts.remote_dir
        )
    };
    let (mkdir_code, _mkdir_out, mkdir_err) = ssh.execute(&mkdir_cmd)?;
    if mkdir_code != 0 {
        spinner.abandon();
        bail!(
            "Failed to create workspace directory at '{}':\n{}",
            opts.remote_dir,
            mkdir_err
        );
    }
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
    let (pull_code, _, pull_err) =
        ssh.execute(&format!("cd {} && docker compose pull", opts.remote_dir))?;
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
    let (up_code, _up_out, up_err) =
        ssh.execute(&format!("cd {} && docker compose up -d", opts.remote_dir))?;
    if up_code != 0 {
        spinner.abandon();
        bail!("Failed to start docker compose: {}", up_err);
    }
    spinner.finish_with_message("Containers active and listening.");

    // Print final banner
    println!(
        "\n{}",
        style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").dim()
    );
    println!(
        "{} {}",
        ROCKET,
        style("Disposable Service Workspace is LIVE!")
            .bold()
            .green()
    );
    println!(
        "{}",
        style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").dim()
    );
    println!(
        "\n  Access URL:  {}",
        style(format!("http://{}:80", opts.host))
            .bold()
            .underlined()
            .cyan()
    );
    println!("  First Step:  Open the URL in your browser to claim and set your Single Password.");
    println!("  Retention:   Strict Zero-Footprint ephemeral storage.\n");

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
        "cd {} && (docker compose down -v)",
        opts.remote_dir
    ));
    spinner.finish_with_message("Containers stopped and volumes wiped.");

    let spinner = create_spinner("Wiping remote workspace directory...");
    let rm_cmd = if opts.user == "root" {
        format!("rm -rf {}", opts.remote_dir)
    } else {
        format!("sudo rm -rf {}", opts.remote_dir)
    };
    let _ = ssh.execute(&rm_cmd);
    spinner.finish_with_message("Directory cleaned.");

    println!(
        "\n{} {}",
        TRASH,
        style("Disposable Workspace successfully torn down. No residual traces remain.")
            .bold()
            .green()
    );

    Ok(())
}

pub fn install_docker_standalone(opts: &DeployOptions) -> Result<()> {
    println!(
        "\n{} {}",
        style("[Disposable Service Workspace]").bold().cyan(),
        style("Docker Host Setup Engine").dim()
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

    // Check if already installed
    let spinner = create_spinner("Checking existing Docker installation...");
    let (code, stdout, stderr) = ssh.execute("docker compose version")?;
    let combined_output = format!("{}\n{}", stdout, stderr);
    if combined_output.contains("Please login as the user") {
        spinner.abandon();
        eprintln!(
            "\n{} Remote host rejected SSH user '{}':\n{}",
            style("Error:").red().bold(),
            opts.user,
            style(combined_output.trim()).yellow().bold()
        );
        bail!(
            "Please re-run specifying the appropriate user (e.g. '--user ubuntu' or '--user ec2-user')."
        );
    }
    if code == 0 {
        spinner.finish_with_message(format!(
            "Docker is already installed and ready: {}",
            stdout.trim()
        ));
        println!(
            "\n{} {}",
            style("✓").green().bold(),
            style("No installation needed. Docker Compose is already operational.").green()
        );
        return Ok(());
    }
    spinner.abandon();

    println!(
        "\n{} Docker runtime not found. Proceeding with installation...",
        style("Notice:").yellow().bold()
    );

    install_docker_remote_with_ssh(&ssh, opts.user)?;

    let mut ssh = ssh;
    if opts.user != "root" {
        let reconnect_spinner = create_spinner("Refreshing SSH session for group permissions...");
        ssh = SshClient::connect(
            opts.host,
            opts.port,
            opts.user,
            opts.key_path,
            opts.passphrase,
        )?;
        reconnect_spinner.finish_with_message("SSH session refreshed.");
    }

    let check_spinner = create_spinner("Verifying installed Docker runtime...");
    let (recheck_code, recheck_out, recheck_err) = ssh.execute("docker compose version")?;
    if recheck_code != 0 {
        check_spinner.abandon();
        bail!(
            "Docker installation completed but 'docker compose version' failed:\n{}",
            recheck_err
        );
    }
    check_spinner.finish_with_message(format!("Runtime ready: {}", recheck_out.trim()));

    println!(
        "\n{} {}",
        style("✓").green().bold(),
        style("Docker Engine and Docker Compose Plugin installed successfully!")
            .green()
            .bold()
    );
    println!("  You can now deploy with: disposable-cli deploy --host {}\n", opts.host);

    Ok(())
}

pub fn install_docker_remote_with_ssh(ssh: &SshClient, _user: &str) -> Result<()> {
    let spinner = create_spinner("Installing Docker & Docker Compose on remote host (1-2 mins)...");

    let installer_script = r#"#!/bin/sh
set -e
export DEBIAN_FRONTEND=noninteractive

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
    if command -v sudo >/dev/null 2>&1; then
        SUDO="sudo"
    else
        echo "Error: Current user is not root and sudo is not installed." >&2
        exit 1
    fi
fi

# 1. Install prerequisites (curl and ca-certificates) if missing
if ! command -v curl >/dev/null 2>&1; then
    if command -v apt-get >/dev/null 2>&1; then
        $SUDO apt-get update -y && $SUDO apt-get install -y curl ca-certificates
    elif command -v yum >/dev/null 2>&1; then
        $SUDO yum install -y curl ca-certificates
    elif command -v dnf >/dev/null 2>&1; then
        $SUDO dnf install -y curl ca-certificates
    fi
fi

# 2. Run official Docker convenience script
curl -fsSL https://get.docker.com | $SUDO sh

# 3. Ensure Docker daemon is running and enabled
$SUDO systemctl enable --now docker 2>/dev/null || $SUDO service docker start 2>/dev/null || true

# 4. Add current user to docker group if non-root
if [ "$(id -u)" -ne 0 ]; then
    $SUDO usermod -aG docker "$USER" 2>/dev/null || true
fi
"#;

    let remote_script_path = "/tmp/install-docker-disposable.sh";
    ssh.upload_content(remote_script_path, installer_script)?;

    let exec_cmd = format!("sh {} && rm -f {}", remote_script_path, remote_script_path);
    let (code, stdout, stderr) = ssh.execute(&exec_cmd)?;
    let _ = ssh.execute(&format!("rm -f {}", remote_script_path));

    if code != 0 {
        spinner.abandon();
        eprintln!(
            "\n{} Remote Docker installation failed with exit code {}:\n{}\n{}",
            style("Error:").red().bold(),
            code,
            style(stdout).dim(),
            style(stderr).red()
        );
        bail!("Failed to install Docker on remote host.");
    }

    spinner.finish_with_message("Docker and Docker Compose installed successfully.");
    Ok(())
}

fn prompt_install_docker() -> Result<bool> {
    if !console::user_attended() {
        return Ok(false);
    }
    use std::io::{self, Write};
    print!(
        "\n{} Remote server does not have Docker or Docker Compose installed.\n  {} Would you like to install Docker & Docker Compose now? [y/N]: ",
        style("Notice:").yellow().bold(),
        style("?").cyan().bold()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    Ok(trimmed == "y" || trimmed == "yes")
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

pub fn deploy_local(opts: &LocalDeployOptions) -> Result<()> {
    println!(
        "\n{} {}",
        style("[Disposable Service Workspace]").bold().cyan(),
        style("Local Deployment Engine").dim()
    );
    println!("Target: Local Machine\n");

    // Check Docker & Docker Compose locally
    let spinner = create_spinner("Verifying local Docker and Docker Compose runtime...");
    let (code, stdout, _stderr) = run_local_command("docker compose version")?;
    if code != 0 {
        spinner.abandon();

        let should_install = if opts.install_docker {
            println!(
                "\n{} Local Docker runtime is missing. Auto-install flag (--install-docker) detected.",
                style("Notice:").cyan().bold()
            );
            true
        } else {
            prompt_install_docker()?
        };

        if should_install {
            install_docker_local()?;

            let check_spinner = create_spinner("Verifying installed Docker runtime...");
            let docker_prefix = get_docker_cmd_prefix();
            let (recheck_code, recheck_out, recheck_err) =
                run_local_command(&format!("{}docker compose version", docker_prefix))?;
            if recheck_code != 0 {
                check_spinner.abandon();
                bail!(
                    "Docker installation completed but 'docker compose version' failed:\n{}",
                    recheck_err
                );
            }
            check_spinner.finish_with_message(format!("Runtime ready: {}", recheck_out.trim()));
        } else {
            bail!(
                "Docker Compose is required on the local machine. Aborting deployment.\nTip: Pass '--install-docker' (or '-i') to install Docker automatically."
            );
        }
    } else {
        spinner.finish_with_message(format!("Runtime found: {}", stdout.trim()));
    }

    // Create local workspace directory
    let spinner = create_spinner(format!("Setting up local workspace at {}...", opts.remote_dir));
    let is_root = run_local_command("id -u")
        .map(|(c, out, _)| c == 0 && out.trim() == "0")
        .unwrap_or(false);
    let mkdir_cmd = if is_root {
        format!("mkdir -p {}", opts.remote_dir)
    } else {
        format!(
            "sudo mkdir -p {} && sudo chown -R $USER:$USER {}",
            opts.remote_dir, opts.remote_dir
        )
    };
    let (mkdir_code, _mkdir_out, mkdir_err) = run_local_command(&mkdir_cmd)?;
    if mkdir_code != 0 {
        spinner.abandon();
        bail!(
            "Failed to create workspace directory at '{}':\n{}",
            opts.remote_dir,
            mkdir_err
        );
    }
    spinner.finish_with_message("Workspace directory ready.");

    // Write docker-compose.yml
    let spinner = create_spinner("Writing docker-compose.yml template...");
    let compose_path = format!("{}/docker-compose.yml", opts.remote_dir);
    std::fs::write(&compose_path, DEFAULT_COMPOSE_TEMPLATE)
        .with_context(|| format!("Failed to write docker-compose.yml at {}", compose_path))?;

    // If custom image override is requested
    if let Some(custom_img) = opts.image {
        let env_content = format!("DISPOSABLE_IMAGE={}\n", custom_img);
        let env_path = format!("{}/.env", opts.remote_dir);
        std::fs::write(&env_path, env_content)?;
    }
    spinner.finish_with_message("Deployment specification ready.");

    // Pull container image
    let spinner = create_spinner("Pulling pre-built container image (docker compose pull)...");
    let docker_prefix = get_docker_cmd_prefix();
    let (pull_code, _, pull_err) = run_local_command(&format!(
        "cd {} && {}docker compose pull",
        opts.remote_dir, docker_prefix
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
    let (up_code, _up_out, up_err) = run_local_command(&format!(
        "cd {} && {}docker compose up -d",
        opts.remote_dir, docker_prefix
    ))?;
    if up_code != 0 {
        spinner.abandon();
        bail!("Failed to start docker compose locally: {}", up_err);
    }
    spinner.finish_with_message("Containers active and listening.");

    let access_ip = detect_local_ip();

    // Print final banner
    println!(
        "\n{}",
        style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").dim()
    );
    println!(
        "{} {}",
        ROCKET,
        style("Disposable Service Workspace is LIVE (Local Mode)!")
            .bold()
            .green()
    );
    println!(
        "{}",
        style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").dim()
    );
    println!(
        "\n  Access URL:  {}",
        style(format!("http://{}:80", access_ip))
            .bold()
            .underlined()
            .cyan()
    );
    println!("  First Step:  Open the URL in your browser to claim and set your Single Password.");
    println!("  Retention:   Strict Zero-Footprint ephemeral storage.\n");

    Ok(())
}

pub fn teardown_local(opts: &LocalDeployOptions) -> Result<()> {
    println!(
        "\n{} {}",
        style("[Disposable Service Workspace]").bold().cyan(),
        style("Local Teardown Engine").dim()
    );
    println!("Target: Local Machine\n");

    let spinner = create_spinner("Stopping and removing containers (docker compose down -v)...");
    let docker_prefix = get_docker_cmd_prefix();
    let _ = run_local_command(&format!(
        "cd {} && ({}docker compose down -v)",
        opts.remote_dir, docker_prefix
    ));
    spinner.finish_with_message("Containers stopped and volumes wiped.");

    let spinner = create_spinner("Wiping local workspace directory...");
    let is_root = run_local_command("id -u")
        .map(|(c, out, _)| c == 0 && out.trim() == "0")
        .unwrap_or(false);
    let rm_cmd = if is_root {
        format!("rm -rf {}", opts.remote_dir)
    } else {
        format!("sudo rm -rf {}", opts.remote_dir)
    };
    let _ = run_local_command(&rm_cmd);
    spinner.finish_with_message("Directory cleaned.");

    println!(
        "\n{} {}",
        TRASH,
        style("Disposable Workspace successfully torn down locally. No residual traces remain.")
            .bold()
            .green()
    );

    Ok(())
}

pub fn install_docker_standalone_local() -> Result<()> {
    println!(
        "\n{} {}",
        style("[Disposable Service Workspace]").bold().cyan(),
        style("Local Docker Host Setup Engine").dim()
    );
    println!("Target: Local Machine\n");

    let spinner = create_spinner("Checking existing Docker installation...");
    let (code, stdout, _) = run_local_command("docker compose version")?;
    if code == 0 {
        spinner.finish_with_message(format!(
            "Docker is already installed and ready: {}",
            stdout.trim()
        ));
        println!(
            "\n{} {}",
            style("✓").green().bold(),
            style("No installation needed. Docker Compose is already operational.").green()
        );
        return Ok(());
    }
    spinner.abandon();

    println!(
        "\n{} Docker runtime not found. Proceeding with installation...",
        style("Notice:").yellow().bold()
    );

    install_docker_local()?;

    let check_spinner = create_spinner("Verifying installed Docker runtime...");
    let docker_prefix = get_docker_cmd_prefix();
    let (recheck_code, recheck_out, recheck_err) =
        run_local_command(&format!("{}docker compose version", docker_prefix))?;
    if recheck_code != 0 {
        check_spinner.abandon();
        bail!(
            "Docker installation completed but 'docker compose version' failed:\n{}",
            recheck_err
        );
    }
    check_spinner.finish_with_message(format!("Runtime ready: {}", recheck_out.trim()));

    println!(
        "\n{} {}",
        style("✓").green().bold(),
        style("Docker Engine and Docker Compose Plugin installed successfully!")
            .green()
            .bold()
    );
    println!("  You can now deploy locally with: disposable-cli deploy --local\n");

    Ok(())
}

pub fn run_local_command(cmd: &str) -> Result<(i32, String, String)> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .with_context(|| format!("Failed to execute local command: {}", cmd))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    Ok((code, stdout, stderr))
}

pub fn install_docker_local() -> Result<()> {
    let spinner = create_spinner("Installing Docker & Docker Compose locally (1-2 mins)...");

    let script = r#"
set -e
export DEBIAN_FRONTEND=noninteractive

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
    if command -v sudo >/dev/null 2>&1; then
        SUDO="sudo"
    else
        echo "Error: Current user is not root and sudo is not installed." >&2
        exit 1
    fi
fi

# 1. Install prerequisites (curl and ca-certificates) if missing
if ! command -v curl >/dev/null 2>&1; then
    if command -v apt-get >/dev/null 2>&1; then
        $SUDO apt-get update -y && $SUDO apt-get install -y curl ca-certificates
    elif command -v yum >/dev/null 2>&1; then
        $SUDO yum install -y curl ca-certificates
    elif command -v dnf >/dev/null 2>&1; then
        $SUDO dnf install -y curl ca-certificates
    fi
fi

# 2. Run official Docker convenience script
curl -fsSL https://get.docker.com | $SUDO sh

# 3. Ensure Docker daemon is running and enabled
$SUDO systemctl enable --now docker 2>/dev/null || $SUDO service docker start 2>/dev/null || true

# 4. Add current user to docker group if non-root
if [ "$(id -u)" -ne 0 ]; then
    $SUDO usermod -aG docker "$USER" 2>/dev/null || true
fi
"#;

    let (code, stdout, stderr) = run_local_command(script)?;
    if code != 0 {
        spinner.abandon();
        eprintln!(
            "\n{} Local Docker installation failed with exit code {}:\n{}\n{}",
            style("Error:").red().bold(),
            code,
            style(stdout).dim(),
            style(stderr).red()
        );
        bail!("Failed to install Docker locally.");
    }

    spinner.finish_with_message("Docker and Docker Compose installed successfully.");
    Ok(())
}

fn get_docker_cmd_prefix() -> &'static str {
    if let Ok((0, _, _)) = run_local_command("docker compose version") {
        ""
    } else if let Ok((0, _, _)) = run_local_command("sudo docker compose version") {
        "sudo "
    } else {
        ""
    }
}

fn detect_local_ip() -> String {
    if let Ok((0, stdout, _)) = run_local_command(
        "curl -s --connect-timeout 2 ifconfig.me || curl -s --connect-timeout 2 checkip.amazonaws.com",
    ) {
        let ip = stdout.trim();
        if !ip.is_empty() && ip.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return ip.to_string();
        }
    }
    if let Ok((0, stdout, _)) = run_local_command("hostname -I") {
        if let Some(first_ip) = stdout.split_whitespace().next() {
            return first_ip.to_string();
        }
    }
    "localhost".to_string()
}
