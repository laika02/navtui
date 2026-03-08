use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{Album, Artist, Song};
use crate::subsonic::{SubsonicClient, ValidateError};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LibrarySnapshot {
    pub artists: Vec<Artist>,
    pub albums_by_artist: HashMap<String, Vec<Album>>,
    pub songs_by_album: HashMap<String, Vec<Song>>,
}

pub struct LibraryCache {
    artists: Vec<Artist>,
    albums_by_artist: HashMap<String, Vec<Album>>,
    songs_by_album: HashMap<String, Vec<Song>>,
    all_albums_cache: Option<Vec<Album>>,
    all_songs_cache: Option<Vec<Song>>,
}

impl LibraryCache {
    pub fn load(client: &SubsonicClient) -> Result<Self, ValidateError> {
        let artists = client.get_artists()?;
        Ok(Self::new(artists))
    }

    pub fn new(artists: Vec<Artist>) -> Self {
        Self {
            artists,
            albums_by_artist: HashMap::new(),
            songs_by_album: HashMap::new(),
            all_albums_cache: None,
            all_songs_cache: None,
        }
    }

    pub fn from_snapshot(snapshot: LibrarySnapshot) -> Self {
        Self {
            artists: snapshot.artists,
            albums_by_artist: snapshot.albums_by_artist,
            songs_by_album: snapshot.songs_by_album,
            all_albums_cache: None,
            all_songs_cache: None,
        }
    }

    pub fn snapshot(&self) -> LibrarySnapshot {
        LibrarySnapshot {
            artists: self.artists.clone(),
            albums_by_artist: self.albums_by_artist.clone(),
            songs_by_album: self.songs_by_album.clone(),
        }
    }

    pub fn artists(&self) -> &[Artist] {
        &self.artists
    }

    pub fn albums_for_artist<'a>(
        &'a mut self,
        client: &SubsonicClient,
        artist_id: &str,
    ) -> Result<&'a [Album], ValidateError> {
        if !self.albums_by_artist.contains_key(artist_id) {
            let albums = client.get_albums_by_artist(artist_id)?;
            self.albums_by_artist.insert(artist_id.to_string(), albums);
            self.all_albums_cache = None;
            self.all_songs_cache = None;
        }

        Ok(self
            .albums_by_artist
            .get(artist_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]))
    }

    pub fn songs_for_album<'a>(
        &'a mut self,
        client: &SubsonicClient,
        album_id: &str,
    ) -> Result<&'a [Song], ValidateError> {
        if !self.songs_by_album.contains_key(album_id) {
            let songs = client.get_songs_by_album(album_id)?;
            self.songs_by_album.insert(album_id.to_string(), songs);
            self.all_songs_cache = None;
        }

        Ok(self
            .songs_by_album
            .get(album_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]))
    }

    pub fn all_albums<'a>(
        &'a mut self,
        client: &SubsonicClient,
    ) -> Result<&'a [Album], ValidateError> {
        if self.all_albums_cache.is_none() {
            let artist_ids: Vec<String> = self.artists.iter().map(|a| a.id.clone()).collect();
            let mut seen = HashSet::new();
            let mut albums = Vec::new();

            for artist_id in artist_ids {
                for album in self.albums_for_artist(client, &artist_id)?.iter().cloned() {
                    if seen.insert(album.id.clone()) {
                        albums.push(album);
                    }
                }
            }

            albums.sort_by_cached_key(|album| album.title.to_lowercase());
            self.all_albums_cache = Some(albums);
        }

        Ok(self.all_albums_cache.as_deref().unwrap_or(&[]))
    }

    pub fn all_songs<'a>(
        &'a mut self,
        client: &SubsonicClient,
    ) -> Result<&'a [Song], ValidateError> {
        if self.all_songs_cache.is_none() {
            let album_ids: Vec<String> = self
                .all_albums(client)?
                .iter()
                .map(|album| album.id.clone())
                .collect();

            let mut seen = HashSet::new();
            let mut songs = Vec::new();
            for album_id in album_ids {
                for song in self.songs_for_album(client, &album_id)?.iter().cloned() {
                    if seen.insert(song.id.clone()) {
                        songs.push(song);
                    }
                }
            }

            songs.sort_by_cached_key(|song| {
                (
                    song.artist_name.to_lowercase(),
                    song.album_title.to_lowercase(),
                    song.track.unwrap_or(u32::MAX),
                    song.title.to_lowercase(),
                )
            });

            self.all_songs_cache = Some(songs);
        }

        Ok(self.all_songs_cache.as_deref().unwrap_or(&[]))
    }
}
