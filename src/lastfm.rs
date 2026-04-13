use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::Value;
use thiserror::Error;

use crate::model::Song;

const LASTFM_API_URL: &str = "https://ws.audioscrobbler.com/2.0/";

#[derive(Clone)]
pub struct LastFmClient {
    api_key: String,
    api_secret: String,
    session_key: String,
    http: Client,
}

#[derive(Debug, Error)]
pub enum LastFmError {
    #[error("last.fm API error (code {code}): {message}")]
    Api { code: i32, message: String },
    #[error("request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("malformed last.fm response: {0}")]
    Malformed(String),
}

impl LastFmError {
    pub fn is_auth_error(&self) -> bool {
        matches!(
            self,
            Self::Api { code, .. } if matches!(code, 4 | 9 | 10 | 13 | 26)
        )
    }
}

impl LastFmClient {
    pub fn new(api_key: &str, api_secret: &str, session_key: &str) -> Result<Self, LastFmError> {
        let http = Client::builder().timeout(Duration::from_secs(10)).build()?;
        Ok(Self {
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
            session_key: session_key.to_string(),
            http,
        })
    }

    pub fn update_now_playing(&self, song: &Song) -> Result<(), LastFmError> {
        if song.artist_name.trim().is_empty() || song.title.trim().is_empty() {
            return Ok(());
        }

        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "track.updateNowPlaying".to_string());
        params.insert("artist".to_string(), song.artist_name.clone());
        params.insert("track".to_string(), song.title.clone());
        params.insert("album".to_string(), song.album_title.clone());
        params.insert("sk".to_string(), self.session_key.clone());
        if let Some(duration) = song.duration_seconds {
            params.insert("duration".to_string(), duration.to_string());
        }

        let _ = self.post_signed(params)?;
        Ok(())
    }

    pub fn scrobble(&self, song: &Song, started_at_unix: i64) -> Result<(), LastFmError> {
        if song.artist_name.trim().is_empty() || song.title.trim().is_empty() {
            return Ok(());
        }

        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "track.scrobble".to_string());
        params.insert("artist".to_string(), song.artist_name.clone());
        params.insert("track".to_string(), song.title.clone());
        params.insert("album".to_string(), song.album_title.clone());
        params.insert("timestamp".to_string(), started_at_unix.to_string());
        params.insert("chosenByUser".to_string(), "1".to_string());
        params.insert("sk".to_string(), self.session_key.clone());
        if let Some(duration) = song.duration_seconds {
            params.insert("duration".to_string(), duration.to_string());
        }

        let _ = self.post_signed(params)?;
        Ok(())
    }

    fn post_signed(&self, mut params: BTreeMap<String, String>) -> Result<Value, LastFmError> {
        params.insert("api_key".to_string(), self.api_key.clone());
        let api_sig = api_signature(&params, &self.api_secret);
        params.insert("api_sig".to_string(), api_sig);
        params.insert("format".to_string(), "json".to_string());

        post_form(&self.http, params)
    }
}

pub fn create_mobile_session(
    api_key: &str,
    api_secret: &str,
    username: &str,
    password: &str,
) -> Result<String, LastFmError> {
    let http = Client::builder().timeout(Duration::from_secs(10)).build()?;
    let mut params = BTreeMap::new();
    params.insert("method".to_string(), "auth.getMobileSession".to_string());
    params.insert("username".to_string(), username.to_string());
    params.insert("password".to_string(), password.to_string());
    params.insert("api_key".to_string(), api_key.to_string());
    let api_sig = api_signature(&params, api_secret);
    params.insert("api_sig".to_string(), api_sig);
    params.insert("format".to_string(), "json".to_string());

    let value = post_form(&http, params)?;
    let session = value
        .get("session")
        .ok_or_else(|| LastFmError::Malformed("missing session field".to_string()))?;
    let key = session
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| LastFmError::Malformed("missing session key".to_string()))?;
    Ok(key.to_string())
}

fn post_form(http: &Client, params: BTreeMap<String, String>) -> Result<Value, LastFmError> {
    let response = http
        .post(LASTFM_API_URL)
        .form(&params)
        .send()?
        .error_for_status()?;
    let value: Value = response.json()?;

    if let Some(error) = parse_api_error(&value) {
        return Err(error);
    }

    Ok(value)
}

fn parse_api_error(value: &Value) -> Option<LastFmError> {
    let code = value.get("error")?.as_i64()? as i32;
    let message = value.get("message")?.as_str()?.to_string();
    Some(LastFmError::Api { code, message })
}

fn api_signature(params: &BTreeMap<String, String>, api_secret: &str) -> String {
    let mut payload = String::new();
    for (key, value) in params {
        if key == "format" || key == "callback" {
            continue;
        }
        payload.push_str(key);
        payload.push_str(value);
    }
    payload.push_str(api_secret);
    format!("{:x}", md5::compute(payload))
}
