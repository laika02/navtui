use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use rand::{Rng, distr::Alphanumeric};
use reqwest::blocking::Client;
use reqwest::header::ACCEPT;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use crate::cache;
use crate::model::{Album, Artist, Song};

const API_VERSION: &str = "1.16.1";
const CLIENT_NAME: &str = "navtui";

#[derive(Debug, Error)]
pub enum ValidateError {
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("subsonic API error (code {code:?}): {message}")]
    Api { code: Option<i32>, message: String },
    #[error("request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("malformed server response: {0}")]
    Malformed(String),
}

#[derive(Clone)]
pub struct SubsonicClient {
    server_url: String,
    username: String,
    password: String,
    http: Client,
}

#[derive(Clone, Debug)]
pub struct StreamTarget {
    pub url: String,
}

impl SubsonicClient {
    pub fn new(server_url: &str, username: &str, password: &str) -> Result<Self, reqwest::Error> {
        let server_url = normalize_server_url(server_url);
        let mut builder = Client::builder().timeout(Duration::from_secs(10));
        if let Some((host, port, ip)) = resolve_override(&server_url) {
            builder = builder.resolve(&host, SocketAddr::new(ip, port));
        }
        let http = builder.build()?;
        Ok(Self {
            server_url,
            username: username.to_string(),
            password: password.to_string(),
            http,
        })
    }

    pub fn ping(&self) -> Result<(), ValidateError> {
        let _: Value = self.get("ping", &[])?;
        Ok(())
    }

    pub fn get_artists(&self) -> Result<Vec<Artist>, ValidateError> {
        let payload: GetArtistsPayload = self.get("getArtists", &[])?;
        Ok(map_artists(payload))
    }

    pub fn get_albums_by_artist(&self, artist_id: &str) -> Result<Vec<Album>, ValidateError> {
        let payload: GetArtistPayload = self.get("getArtist", &[("id", artist_id)])?;
        Ok(map_albums_for_artist(payload, artist_id))
    }

    pub fn get_songs_by_album(&self, album_id: &str) -> Result<Vec<Song>, ValidateError> {
        let payload: GetAlbumPayload = self.get("getAlbum", &[("id", album_id)])?;
        Ok(map_songs_for_album(payload, album_id))
    }

    pub fn stream_target(&self, song_id: &str) -> Result<StreamTarget, ValidateError> {
        let salt = generate_salt(12);
        let token = format!("{:x}", md5::compute(format!("{}{}", self.password, salt)));
        let endpoint = format!("{}/rest/stream.view", self.server_url);

        let mut url = reqwest::Url::parse(&endpoint)
            .map_err(|err| ValidateError::Malformed(err.to_string()))?;
        url.query_pairs_mut()
            .append_pair("id", song_id)
            .append_pair("u", &self.username)
            .append_pair("v", API_VERSION)
            .append_pair("c", CLIENT_NAME)
            .append_pair("s", &salt)
            .append_pair("t", &token);
        Ok(StreamTarget {
            url: url.to_string(),
        })
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    fn get<T: DeserializeOwned>(
        &self,
        endpoint_name: &str,
        extra_query: &[(&str, &str)],
    ) -> Result<T, ValidateError> {
        let endpoint = format!("{}/rest/{}.view", self.server_url, endpoint_name);
        let salt = generate_salt(12);
        let token = format!("{:x}", md5::compute(format!("{}{}", self.password, salt)));

        let mut query: Vec<(String, String)> = vec![
            ("u".to_string(), self.username.clone()),
            ("v".to_string(), API_VERSION.to_string()),
            ("c".to_string(), CLIENT_NAME.to_string()),
            ("f".to_string(), "json".to_string()),
            ("s".to_string(), salt),
            ("t".to_string(), token),
        ];
        for (k, v) in extra_query {
            query.push(((*k).to_string(), (*v).to_string()));
        }

        let response = self
            .http
            .get(endpoint)
            .query(&query)
            .send()?
            .error_for_status()?;
        let value: Value = response.json()?;
        let root = value.get("subsonic-response").cloned().ok_or_else(|| {
            ValidateError::Malformed("missing subsonic-response root".to_string())
        })?;

        let status = root
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| ValidateError::Malformed("missing response status".to_string()))?;
        if !status.eq_ignore_ascii_case("ok") {
            return Err(map_failure(root));
        }

        serde_json::from_value(root).map_err(|err| ValidateError::Malformed(err.to_string()))
    }
}

pub fn validate_login(
    server_url: &str,
    username: &str,
    password: &str,
) -> Result<(), ValidateError> {
    let client = SubsonicClient::new(server_url, username, password)?;
    client.ping()
}

#[derive(Debug, Deserialize)]
struct GetArtistsPayload {
    #[serde(default)]
    artists: Option<ArtistsBody>,
}

#[derive(Debug, Deserialize)]
struct ArtistsBody {
    #[serde(default)]
    index: Vec<ArtistIndex>,
}

#[derive(Debug, Deserialize)]
struct ArtistIndex {
    #[serde(default)]
    artist: Vec<ArtistItem>,
}

#[derive(Debug, Deserialize)]
struct ArtistItem {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct GetArtistPayload {
    artist: ArtistBody,
}

#[derive(Debug, Deserialize)]
struct ArtistBody {
    #[serde(default)]
    album: Vec<AlbumItem>,
}

#[derive(Debug, Deserialize)]
struct AlbumItem {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(rename = "artistId", default)]
    artist_id: Option<String>,
    #[serde(default)]
    year: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct GetAlbumPayload {
    album: AlbumBody,
}

#[derive(Debug, Deserialize)]
struct AlbumBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(rename = "artistId", default)]
    artist_id: Option<String>,
    #[serde(default)]
    song: Vec<SongItem>,
}

#[derive(Debug, Deserialize)]
struct SongItem {
    id: String,
    title: String,
    #[serde(default)]
    album: Option<String>,
    #[serde(rename = "albumId", default)]
    album_id: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(rename = "artistId", default)]
    artist_id: Option<String>,
    #[serde(default)]
    artists: Option<Value>,
    #[serde(default)]
    duration: Option<u32>,
    #[serde(default)]
    track: Option<u32>,
}

fn map_artists(payload: GetArtistsPayload) -> Vec<Artist> {
    let mut seen = HashSet::new();
    let mut artists = Vec::new();

    if let Some(body) = payload.artists {
        for idx in body.index {
            for item in idx.artist {
                if item.name.trim().is_empty() {
                    continue;
                }
                if seen.insert(item.id.clone()) {
                    artists.push(Artist {
                        id: item.id,
                        name: item.name,
                    });
                }
            }
        }
    }

    artists.sort_by_cached_key(|artist| artist.name.to_lowercase());
    artists
}

fn map_albums_for_artist(payload: GetArtistPayload, fallback_artist_id: &str) -> Vec<Album> {
    let mut seen = HashSet::new();
    let mut albums = Vec::new();

    for item in payload.artist.album {
        if !seen.insert(item.id.clone()) {
            continue;
        }

        let title = item
            .name
            .or(item.title)
            .unwrap_or_else(|| "Unknown Album".to_string());
        let artist_id = item
            .artist_id
            .unwrap_or_else(|| fallback_artist_id.to_string());
        let artist_name = item.artist.unwrap_or_else(|| "Unknown Artist".to_string());

        albums.push(Album {
            id: item.id,
            artist_id,
            artist_name,
            title,
            year: item.year,
        });
    }

    albums.sort_by_cached_key(|album| album.title.to_lowercase());
    albums
}

fn map_songs_for_album(payload: GetAlbumPayload, fallback_album_id: &str) -> Vec<Song> {
    let album_title = payload
        .album
        .name
        .or(payload.album.title)
        .unwrap_or_else(|| "Unknown Album".to_string());
    let album_artist = payload
        .album
        .artist
        .unwrap_or_else(|| "Unknown Artist".to_string());
    let album_artist_id = payload.album.artist_id.unwrap_or_default();

    let mut seen = HashSet::new();
    let mut songs = Vec::new();
    for item in payload.album.song {
        if !seen.insert(item.id.clone()) {
            continue;
        }
        let artist_ids = collect_song_artist_ids(
            item.artist_id.as_deref(),
            item.artists.as_ref(),
            &album_artist_id,
        );

        songs.push(Song {
            id: item.id,
            album_id: item
                .album_id
                .unwrap_or_else(|| fallback_album_id.to_string()),
            album_title: item.album.unwrap_or_else(|| album_title.clone()),
            artist_id: item.artist_id.unwrap_or_else(|| album_artist_id.clone()),
            artist_ids,
            artist_name: item.artist.unwrap_or_else(|| album_artist.clone()),
            title: item.title,
            duration_seconds: item.duration,
            track: item.track,
        });
    }

    songs.sort_by_cached_key(|song| (song.track.unwrap_or(u32::MAX), song.title.to_lowercase()));
    songs
}

fn map_failure(root: Value) -> ValidateError {
    let error = root.get("error");
    let code = error
        .and_then(|v| v.get("code"))
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok());
    let message = error
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown error")
        .to_string();

    if matches!(code, Some(40) | Some(41)) {
        return ValidateError::InvalidCredentials;
    }

    ValidateError::Api { code, message }
}

fn normalize_server_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

fn generate_salt(len: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

fn collect_song_artist_ids(
    primary_artist_id: Option<&str>,
    artists_field: Option<&Value>,
    fallback_artist_id: &str,
) -> Vec<String> {
    let mut ids = Vec::new();

    if let Some(primary) = primary_artist_id
        && !primary.is_empty()
    {
        ids.push(primary.to_string());
    }

    if let Some(Value::Array(entries)) = artists_field {
        for entry in entries {
            match entry {
                Value::String(id) if !id.is_empty() => push_unique(&mut ids, id),
                Value::Object(map) => {
                    if let Some(id) = map.get("id").and_then(Value::as_str)
                        && !id.is_empty()
                    {
                        push_unique(&mut ids, id);
                    }
                }
                _ => {}
            }
        }
    }

    if ids.is_empty() && !fallback_artist_id.is_empty() {
        ids.push(fallback_artist_id.to_string());
    }

    ids
}

fn push_unique(into: &mut Vec<String>, value: &str) {
    if !into.iter().any(|existing| existing == value) {
        into.push(value.to_string());
    }
}

fn resolve_override(server_url: &str) -> Option<(String, u16, IpAddr)> {
    let parsed = reqwest::Url::parse(server_url).ok()?;
    let host = parsed.host_str()?.to_string();
    if host.parse::<IpAddr>().is_ok() {
        return None;
    }

    let ip = std::env::var("NAVTUI_RESOLVE_IP")
        .ok()
        .and_then(|raw| raw.trim().parse::<IpAddr>().ok())
        .or_else(|| {
            std::env::var("SUBSONIC_TUI_RESOLVE_IP")
                .ok()
                .and_then(|raw| raw.trim().parse::<IpAddr>().ok())
        })
        .or_else(|| cache::load_dns_override(&host))
        .or_else(|| resolve_host_via_doh(&host))?;

    let _ = cache::save_dns_override(&host, ip);
    let port = parsed.port_or_known_default()?;
    Some((host, port, ip))
}

#[derive(Debug, Deserialize)]
struct DohResponse {
    #[serde(rename = "Status", default)]
    status: Option<u32>,
    #[serde(rename = "Answer", default)]
    answer: Vec<DohAnswer>,
}

#[derive(Debug, Deserialize)]
struct DohAnswer {
    #[serde(rename = "type")]
    record_type: u32,
    data: String,
}

fn resolve_host_via_doh(host: &str) -> Option<IpAddr> {
    query_doh(host, "cloudflare-dns.com", "1.1.1.1")
        .or_else(|| query_doh(host, "cloudflare-dns.com", "1.0.0.1"))
        .or_else(|| query_doh(host, "dns.google", "8.8.8.8"))
        .or_else(|| query_doh(host, "dns.google", "8.8.4.4"))
}

fn query_doh(host: &str, doh_host: &str, resolver_ip: &str) -> Option<IpAddr> {
    let resolver_ip = resolver_ip.parse::<IpAddr>().ok()?;
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .resolve(doh_host, SocketAddr::new(resolver_ip, 443))
        .build()
        .ok()?;
    let response = client
        .get(format!("https://{doh_host}/resolve"))
        .header(ACCEPT, "application/dns-json")
        .query(&[("name", host), ("type", "A")])
        .send()
        .ok()?
        .error_for_status()
        .ok()?;

    let body: DohResponse = response.json().ok()?;
    if body.status != Some(0) {
        return None;
    }

    body.answer
        .into_iter()
        .find(|answer| answer.record_type == 1)
        .and_then(|answer| answer.data.parse::<IpAddr>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_artists_deduplicates_and_sorts() {
        let payload = GetArtistsPayload {
            artists: Some(ArtistsBody {
                index: vec![
                    ArtistIndex {
                        artist: vec![
                            ArtistItem {
                                id: "2".to_string(),
                                name: "zeta".to_string(),
                            },
                            ArtistItem {
                                id: "1".to_string(),
                                name: "Alpha".to_string(),
                            },
                        ],
                    },
                    ArtistIndex {
                        artist: vec![ArtistItem {
                            id: "1".to_string(),
                            name: "Alpha".to_string(),
                        }],
                    },
                ],
            }),
        };

        let artists = map_artists(payload);
        assert_eq!(artists.len(), 2);
        assert_eq!(artists[0].name, "Alpha");
        assert_eq!(artists[1].name, "zeta");
    }

    #[test]
    fn map_songs_orders_by_track_then_title() {
        let payload = GetAlbumPayload {
            album: AlbumBody {
                name: Some("Album".to_string()),
                title: None,
                artist: Some("Artist".to_string()),
                artist_id: Some("artist-id".to_string()),
                song: vec![
                    SongItem {
                        id: "2".to_string(),
                        title: "b".to_string(),
                        album: None,
                        album_id: None,
                        artist: None,
                        artist_id: None,
                        artists: None,
                        duration: None,
                        track: Some(2),
                    },
                    SongItem {
                        id: "1".to_string(),
                        title: "a".to_string(),
                        album: None,
                        album_id: None,
                        artist: None,
                        artist_id: None,
                        artists: None,
                        duration: None,
                        track: Some(1),
                    },
                ],
            },
        };

        let songs = map_songs_for_album(payload, "album-id");
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].id, "1");
        assert_eq!(songs[1].id, "2");
    }

    #[test]
    fn collect_song_artist_ids_supports_artists_array() {
        let artists = Value::Array(vec![
            Value::Object(
                [("id".to_string(), Value::String("artist-a".to_string()))]
                    .into_iter()
                    .collect(),
            ),
            Value::String("artist-b".to_string()),
        ]);

        let ids = collect_song_artist_ids(Some("artist-main"), Some(&artists), "fallback");
        assert_eq!(ids, vec!["artist-main", "artist-a", "artist-b"]);
    }
}
