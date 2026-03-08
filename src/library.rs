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

const MAX_PARALLEL_FETCH: usize = 8;

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

    pub fn has_all_albums_loaded(&self) -> bool {
        self.artists
            .iter()
            .all(|artist| self.albums_by_artist.contains_key(&artist.id))
    }

    pub fn has_all_songs_loaded(&self) -> bool {
        if !self.has_all_albums_loaded() {
            return false;
        }
        let mut seen_album_ids = HashSet::new();
        for albums in self.albums_by_artist.values() {
            for album in albums {
                if seen_album_ids.insert(album.id.clone())
                    && !self.songs_by_album.contains_key(&album.id)
                {
                    return false;
                }
            }
        }
        true
    }

    pub fn loaded_artist_ids(&self) -> HashSet<String> {
        self.albums_by_artist.keys().cloned().collect()
    }

    pub fn loaded_album_ids(&self) -> HashSet<String> {
        let mut ids = HashSet::new();
        for albums in self.albums_by_artist.values() {
            for album in albums {
                ids.insert(album.id.clone());
            }
        }
        ids
    }

    pub fn known_albums_for_artist(&self, artist_id: &str) -> Vec<Album> {
        self.albums_by_artist
            .get(artist_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn known_songs_for_album(&self, album_id: &str) -> Vec<Song> {
        self.songs_by_album
            .get(album_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn known_all_albums(&self) -> Vec<Album> {
        let mut seen = HashSet::new();
        let mut albums = Vec::new();
        for albums_for_artist in self.albums_by_artist.values() {
            for album in albums_for_artist {
                if seen.insert(album.id.clone()) {
                    albums.push(album.clone());
                }
            }
        }
        albums.sort_by_cached_key(|album| album.title.to_lowercase());
        albums
    }

    pub fn known_all_songs(&self) -> Vec<Song> {
        let mut seen = HashSet::new();
        let mut songs = Vec::new();
        for songs_for_album in self.songs_by_album.values() {
            for song in songs_for_album {
                if seen.insert(song.id.clone()) {
                    songs.push(song.clone());
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
        songs
    }

    pub fn upsert_albums_for_artist(&mut self, artist_id: String, albums: Vec<Album>) {
        self.albums_by_artist.insert(artist_id, albums);
        self.all_albums_cache = None;
        self.all_songs_cache = None;
    }

    pub fn upsert_songs_for_album(&mut self, album_id: String, songs: Vec<Song>) {
        self.songs_by_album.insert(album_id, songs);
        self.all_songs_cache = None;
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
            let missing_artist_ids: Vec<String> = self
                .artists
                .iter()
                .filter(|artist| !self.albums_by_artist.contains_key(&artist.id))
                .map(|artist| artist.id.clone())
                .collect();
            self.fetch_missing_albums_for_artists(client, &missing_artist_ids)?;
            self.all_albums_cache = Some(self.known_all_albums());
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
            let missing_album_ids: Vec<String> = album_ids
                .iter()
                .filter(|album_id| !self.songs_by_album.contains_key(*album_id))
                .cloned()
                .collect();
            self.fetch_missing_songs_for_albums(client, &missing_album_ids)?;
            self.all_songs_cache = Some(self.known_all_songs());
        }

        Ok(self.all_songs_cache.as_deref().unwrap_or(&[]))
    }

    fn fetch_missing_albums_for_artists(
        &mut self,
        client: &SubsonicClient,
        artist_ids: &[String],
    ) -> Result<(), ValidateError> {
        for chunk in artist_ids.chunks(MAX_PARALLEL_FETCH) {
            let mut handles = Vec::with_capacity(chunk.len());
            for artist_id in chunk {
                let artist_id = artist_id.clone();
                let client = client.clone();
                handles.push(std::thread::spawn(move || {
                    let result = client.get_albums_by_artist(&artist_id);
                    (artist_id, result)
                }));
            }

            for handle in handles {
                let (artist_id, albums) = handle.join().map_err(|_| {
                    ValidateError::Malformed(
                        "album fetch worker panicked while building library cache".to_string(),
                    )
                })?;
                self.albums_by_artist.insert(artist_id, albums?);
            }
        }
        if !artist_ids.is_empty() {
            self.all_albums_cache = None;
            self.all_songs_cache = None;
        }
        Ok(())
    }

    fn fetch_missing_songs_for_albums(
        &mut self,
        client: &SubsonicClient,
        album_ids: &[String],
    ) -> Result<(), ValidateError> {
        for chunk in album_ids.chunks(MAX_PARALLEL_FETCH) {
            let mut handles = Vec::with_capacity(chunk.len());
            for album_id in chunk {
                let album_id = album_id.clone();
                let client = client.clone();
                handles.push(std::thread::spawn(move || {
                    let result = client.get_songs_by_album(&album_id);
                    (album_id, result)
                }));
            }

            for handle in handles {
                let (album_id, songs) = handle.join().map_err(|_| {
                    ValidateError::Malformed(
                        "song fetch worker panicked while building library cache".to_string(),
                    )
                })?;
                self.songs_by_album.insert(album_id, songs?);
            }
        }
        if !album_ids.is_empty() {
            self.all_songs_cache = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artist(id: &str, name: &str) -> Artist {
        Artist {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    fn album(id: &str, artist_id: &str, artist_name: &str, title: &str) -> Album {
        Album {
            id: id.to_string(),
            artist_id: artist_id.to_string(),
            artist_name: artist_name.to_string(),
            title: title.to_string(),
            year: None,
        }
    }

    fn song(id: &str, album_id: &str, album_title: &str, artist_id: &str, title: &str) -> Song {
        Song {
            id: id.to_string(),
            album_id: album_id.to_string(),
            album_title: album_title.to_string(),
            artist_id: artist_id.to_string(),
            artist_ids: vec![artist_id.to_string()],
            artist_name: artist_id.to_string(),
            title: title.to_string(),
            duration_seconds: None,
            track: None,
        }
    }

    #[test]
    fn has_all_loaded_flags_track_completeness() {
        let mut cache = LibraryCache::new(vec![artist("a1", "A1"), artist("a2", "A2")]);
        assert!(!cache.has_all_albums_loaded());
        assert!(!cache.has_all_songs_loaded());

        cache.upsert_albums_for_artist("a1".to_string(), vec![album("alb1", "a1", "A1", "First")]);
        assert!(!cache.has_all_albums_loaded());

        cache.upsert_albums_for_artist("a2".to_string(), Vec::new());
        assert!(cache.has_all_albums_loaded());
        assert!(!cache.has_all_songs_loaded());

        cache.upsert_songs_for_album(
            "alb1".to_string(),
            vec![song("s1", "alb1", "First", "a1", "Track")],
        );
        assert!(cache.has_all_songs_loaded());
    }

    #[test]
    fn known_views_are_deduplicated_and_sorted() {
        let mut cache = LibraryCache::new(vec![artist("a1", "A1"), artist("a2", "A2")]);
        let shared = album("shared", "a1", "A1", "M Album");
        cache.upsert_albums_for_artist(
            "a1".to_string(),
            vec![shared.clone(), album("z", "a1", "A1", "Z Album")],
        );
        cache.upsert_albums_for_artist(
            "a2".to_string(),
            vec![album("a", "a2", "A2", "A Album"), shared],
        );

        let albums = cache.known_all_albums();
        let album_titles: Vec<&str> = albums.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(album_titles, vec!["A Album", "M Album", "Z Album"]);

        cache.upsert_songs_for_album(
            "a".to_string(),
            vec![song("s2", "a", "A Album", "a2", "Beta")],
        );
        cache.upsert_songs_for_album(
            "z".to_string(),
            vec![song("s1", "z", "Z Album", "a1", "Alpha")],
        );
        cache.upsert_songs_for_album(
            "shared".to_string(),
            vec![song("s1", "shared", "M Album", "a1", "Alpha")],
        );

        let songs = cache.known_all_songs();
        let song_ids: Vec<&str> = songs.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(song_ids, vec!["s1", "s2"]);
    }
}
