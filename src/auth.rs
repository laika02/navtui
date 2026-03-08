#[cfg(target_os = "linux")]
use std::sync::Once;

use anyhow::{Context, Result, bail};
use dialoguer::{Input, Password};
use keyring::{Entry, Error as KeyringError};

use crate::config::{self, Config, KeybindsConfig};
use crate::subsonic::{self, ValidateError};

const KEYRING_SERVICE: &str = "navtui";
const LEGACY_KEYRING_SERVICE: &str = "subsonic-tui";
#[cfg(target_os = "linux")]
static KEYRING_BACKEND_INIT: Once = Once::new();

#[derive(Clone, Debug)]
pub struct Credentials {
    pub config: Config,
    pub password: String,
}

pub fn bootstrap() -> Result<Credentials> {
    init_keyring_backend();

    if let Some(cfg) = config::load()? {
        return login_with_existing_config(cfg);
    }

    first_time_setup()
}

fn first_time_setup() -> Result<Credentials> {
    let server_url = prompt_server_url()?;
    let username = prompt_username()?;
    let password = prompt_password()?;
    let cfg = Config {
        server_url,
        username,
        always_hard_refresh_on_launch: false,
        expand_on_search_collapse: false,
        show_identity_label: true,
        keybinds: KeybindsConfig::default(),
    };

    validate(&cfg, &password).map_err(map_validate_error)?;
    config::save(&cfg)?;
    set_password(&cfg, &password)?;

    Ok(Credentials {
        config: cfg,
        password,
    })
}

fn login_with_existing_config(cfg: Config) -> Result<Credentials> {
    if let Some(saved_password) = get_password(&cfg)? {
        match validate(&cfg, &saved_password) {
            Ok(()) => {
                return Ok(Credentials {
                    config: cfg,
                    password: saved_password,
                });
            }
            Err(ValidateError::InvalidCredentials) => {
                eprintln!("Stored password was rejected by server. Please enter a new password.");
            }
            Err(err) => return Err(map_validate_error(err)),
        }
    } else {
        eprintln!("No saved password found in keyring. Please enter your password.");
    }

    let password = prompt_password()?;
    validate(&cfg, &password).map_err(map_validate_error)?;
    set_password(&cfg, &password)?;

    Ok(Credentials {
        config: cfg,
        password,
    })
}

fn validate(cfg: &Config, password: &str) -> std::result::Result<(), ValidateError> {
    subsonic::validate_login(&cfg.server_url, &cfg.username, password)
}

fn get_password(cfg: &Config) -> Result<Option<String>> {
    if let Some(password) = get_password_from_service(cfg, KEYRING_SERVICE)? {
        return Ok(Some(password));
    }
    get_password_from_service(cfg, LEGACY_KEYRING_SERVICE)
}

fn set_password(cfg: &Config, password: &str) -> Result<()> {
    if password.is_empty() {
        bail!("password cannot be empty");
    }

    let entry = entry_for_service(cfg, KEYRING_SERVICE)?;
    entry
        .set_password(password)
        .context("failed to store password in keyring")?;
    Ok(())
}

fn entry_for_service(cfg: &Config, service: &str) -> Result<Entry> {
    let server_hash = format!("{:x}", md5::compute(cfg.server_url.as_bytes()));
    let key = format!("{}@{}", cfg.username, server_hash);
    Entry::new(service, &key).context("failed to create keyring entry")
}

fn get_password_from_service(cfg: &Config, service: &str) -> Result<Option<String>> {
    let entry = entry_for_service(cfg, service)?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(err) => Err(err).context("failed to read password from keyring"),
    }
}

fn prompt_server_url() -> Result<String> {
    let server_url: String = Input::new()
        .with_prompt("Navidrome server URL")
        .with_initial_text("http://localhost:4533")
        .validate_with(|input: &String| -> std::result::Result<(), &str> {
            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Err("server URL cannot be empty");
            }
            if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
                return Err("URL must start with http:// or https://");
            }
            Ok(())
        })
        .interact_text()
        .context("failed to read server URL input")?;

    Ok(normalize_server_url(&server_url))
}

fn prompt_username() -> Result<String> {
    let username: String = Input::new()
        .with_prompt("Username")
        .validate_with(|input: &String| -> std::result::Result<(), &str> {
            if input.trim().is_empty() {
                return Err("username cannot be empty");
            }
            Ok(())
        })
        .interact_text()
        .context("failed to read username input")?;
    Ok(username.trim().to_string())
}

fn prompt_password() -> Result<String> {
    Password::new()
        .with_prompt("Password")
        .allow_empty_password(false)
        .interact()
        .context("failed to read password input")
}

fn normalize_server_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

fn map_validate_error(err: ValidateError) -> anyhow::Error {
    match err {
        ValidateError::InvalidCredentials => anyhow::anyhow!("invalid username or password"),
        ValidateError::Transport(inner)
            if inner.to_string().contains("dns error")
                || inner.to_string().contains("lookup address information") =>
        {
            anyhow::anyhow!(
                "DNS lookup failed for the server hostname. \
The app attempts DoH and cached-IP fallback automatically, but if resolution is still blocked \
set `NAVTUI_RESOLVE_IP=<server-ip>` and retry (`SUBSONIC_TUI_RESOLVE_IP` is still supported as a legacy alias)."
            )
        }
        other => anyhow::anyhow!(other),
    }
}

fn init_keyring_backend() {
    #[cfg(target_os = "linux")]
    KEYRING_BACKEND_INIT.call_once(|| {
        // Use Linux keyutils directly so headless/CLI sessions work without DBus Secret Service.
        keyring::set_default_credential_builder(keyring::keyutils::default_credential_builder());
    });
}
