use anyhow::{Context, Result};
use ssh2::Session;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

pub struct SshClient {
    session: Session,
    #[allow(dead_code)]
    host: String,
    #[allow(dead_code)]
    port: u16,
}

impl SshClient {
    pub fn connect(
        host: &str,
        port: u16,
        user: &str,
        key_path: &Path,
        passphrase: Option<&str>,
    ) -> Result<Self> {
        let addr = format!("{}:{}", host, port);
        let tcp = TcpStream::connect(&addr)
            .with_context(|| format!("Failed to establish TCP connection to {}", addr))?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))?;

        let mut session = Session::new().context("Failed to create SSH session")?;
        session.set_tcp_stream(tcp);
        session
            .handshake()
            .with_context(|| format!("SSH handshake failed with {}", addr))?;

        session
            .userauth_pubkey_file(user, None, key_path, passphrase)
            .with_context(|| {
                format!(
                    "SSH authentication failed for user '{}' with key {:?}",
                    user, key_path
                )
            })?;

        if !session.authenticated() {
            anyhow::bail!("SSH authentication incomplete");
        }

        Ok(Self {
            session,
            host: host.to_string(),
            port,
        })
    }

    pub fn execute(&self, command: &str) -> Result<(i32, String, String)> {
        let mut channel = self
            .session
            .channel_session()
            .context("Failed to open SSH channel")?;

        channel
            .exec(command)
            .with_context(|| format!("Failed to exec remote command: {}", command))?;

        let mut stdout = String::new();
        channel
            .read_to_string(&mut stdout)
            .context("Failed to read stdout from remote command")?;

        let mut stderr = String::new();
        channel
            .stderr()
            .read_to_string(&mut stderr)
            .context("Failed to read stderr from remote command")?;

        channel.wait_close()?;
        let exit_status = channel.exit_status()?;

        Ok((exit_status, stdout, stderr))
    }

    pub fn upload_content(&self, remote_path: &str, content: &str) -> Result<()> {
        let sftp = self
            .session
            .sftp()
            .context("Failed to initialize SFTP subsystem")?;

        let path = Path::new(remote_path);
        let mut file = sftp
            .create(path)
            .with_context(|| format!("Failed to create remote file via SFTP at {}", remote_path))?;

        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write content to {}", remote_path))?;

        Ok(())
    }
}
