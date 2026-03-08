use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::library::LibrarySnapshot;

const CACHE_DIR_NAME: &str = "navtui";
const LEGACY_CACHE_DIR_NAME: &str = "subsonic-tui";
const DNS_CACHE_FILE: &str = "dns.json";
const DNS_CACHE_TTL_SECONDS: u64 = 3600;

#[derive(Debug, Default, Deserialize, Serialize)]
struct DnsCacheFile {
    #[serde(default)]
    entries: HashMap<String, DnsCacheEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DnsCacheEntry {
    ip: String,
    cached_at_unix: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct LibraryCacheFile {
    saved_at_unix: u64,
    snapshot: LibrarySnapshot,
}

pub fn load_library_snapshot(server_url: &str, username: &str) -> Option<LibrarySnapshot> {
    let primary_path = library_cache_path(server_url, username).ok()?;
    read_library_snapshot(&primary_path).or_else(|| {
        legacy_library_cache_path(server_url, username)
            .ok()
            .and_then(|legacy_path| read_library_snapshot(&legacy_path))
    })
}

pub fn save_library_snapshot(
    server_url: &str,
    username: &str,
    snapshot: &LibrarySnapshot,
) -> Result<()> {
    let path = library_cache_path(server_url, username)?;
    ensure_cache_dir_exists()?;
    let body = LibraryCacheFile {
        saved_at_unix: now_unix(),
        snapshot: snapshot.clone(),
    };
    let encoded = serde_json::to_vec(&body).context("failed to encode library cache JSON")?;
    atomic_write(&path, &encoded)
}

pub fn clear_library_snapshot(server_url: &str, username: &str) -> Result<()> {
    let path = library_cache_path(server_url, username)?;
    remove_file_if_exists(&path)?;
    let legacy_path = legacy_library_cache_path(server_url, username)?;
    remove_file_if_exists(&legacy_path)
}

pub fn clear_dns_cache() -> Result<()> {
    let path = dns_cache_path()?;
    remove_file_if_exists(&path)?;
    let legacy_path = legacy_dns_cache_path()?;
    remove_file_if_exists(&legacy_path)
}

pub fn load_dns_override(host: &str) -> Option<IpAddr> {
    let primary_path = dns_cache_path().ok()?;
    read_dns_override_from_path(&primary_path, host).or_else(|| {
        legacy_dns_cache_path()
            .ok()
            .and_then(|legacy_path| read_dns_override_from_path(&legacy_path, host))
    })
}

pub fn save_dns_override(host: &str, ip: IpAddr) -> Result<()> {
    ensure_cache_dir_exists()?;
    let path = dns_cache_path()?;
    let mut cache = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<DnsCacheFile>(&raw).ok())
            .unwrap_or_default()
    } else {
        DnsCacheFile::default()
    };

    cache.entries.insert(
        host.to_string(),
        DnsCacheEntry {
            ip: ip.to_string(),
            cached_at_unix: now_unix(),
        },
    );
    let encoded = serde_json::to_vec(&cache).context("failed to encode DNS cache JSON")?;
    atomic_write(&path, &encoded)
}

fn cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("could not determine cache directory")?;
    Ok(base.join(CACHE_DIR_NAME))
}

fn legacy_cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("could not determine cache directory")?;
    Ok(base.join(LEGACY_CACHE_DIR_NAME))
}

fn ensure_cache_dir_exists() -> Result<()> {
    let dir = cache_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create cache directory {}", dir.display()))?;
    }
    Ok(())
}

fn dns_cache_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join(DNS_CACHE_FILE))
}

fn legacy_dns_cache_path() -> Result<PathBuf> {
    Ok(legacy_cache_dir()?.join(DNS_CACHE_FILE))
}

fn library_cache_path(server_url: &str, username: &str) -> Result<PathBuf> {
    let key = format!("{:x}", md5::compute(format!("{username}|{server_url}")));
    Ok(cache_dir()?.join(format!("library-{key}.json")))
}

fn legacy_library_cache_path(server_url: &str, username: &str) -> Result<PathBuf> {
    let key = format!("{:x}", md5::compute(format!("{username}|{server_url}")));
    Ok(legacy_cache_dir()?.join(format!("library-{key}.json")))
}

fn atomic_write(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)
        .with_context(|| format!("failed to open temp cache file {}", tmp.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to write temp cache file {}", tmp.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync temp cache file {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| {
        format!(
            "failed to replace cache file {} from temp {}",
            path.display(),
            tmp.display()
        )
    })?;
    Ok(())
}

fn remove_file_if_exists(path: &PathBuf) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn read_library_snapshot(path: &PathBuf) -> Option<LibrarySnapshot> {
    let raw = fs::read_to_string(path).ok()?;
    let file: LibraryCacheFile = serde_json::from_str(&raw).ok()?;
    Some(file.snapshot)
}

fn read_dns_override_from_path(path: &PathBuf, host: &str) -> Option<IpAddr> {
    let raw = fs::read_to_string(path).ok()?;
    let file: DnsCacheFile = serde_json::from_str(&raw).ok()?;
    let entry = file.entries.get(host)?;
    if now_unix().saturating_sub(entry.cached_at_unix) > DNS_CACHE_TTL_SECONDS {
        return None;
    }
    entry.ip.parse::<IpAddr>().ok()
}
