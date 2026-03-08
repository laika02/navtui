use crate::library::LibraryCache;
use crate::model::{Album, Artist, Song};
use crate::subsonic::{SubsonicClient, ValidateError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Artists,
    Albums,
    Songs,
}

impl Tab {
    fn next(self) -> Self {
        match self {
            Self::Artists => Self::Albums,
            Self::Albums => Self::Songs,
            Self::Songs => Self::Artists,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AlbumScope {
    All,
    Artist(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SongScope {
    All,
    Album(String),
}

#[derive(Clone, Debug)]
struct Snapshot {
    active_tab: Tab,
    album_scope: AlbumScope,
    song_scope: SongScope,
    artist_index: usize,
    album_index: usize,
    song_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Tab,
    Up,
    Down,
    RightOrEnter,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    Play(Song),
}

pub struct BrowserState {
    active_tab: Tab,
    artists_all: Vec<Artist>,
    artists: Vec<Artist>,
    albums_all: Vec<Album>,
    albums: Vec<Album>,
    songs_all: Vec<Song>,
    songs: Vec<Song>,
    artist_index: usize,
    album_index: usize,
    song_index: usize,
    artist_filter: String,
    album_filter: String,
    song_filter: String,
    album_scope: AlbumScope,
    song_scope: SongScope,
    back_stack: Vec<Snapshot>,
}

impl BrowserState {
    pub fn new(artists: Vec<Artist>) -> Self {
        let artists_all = artists.clone();
        Self {
            active_tab: Tab::Artists,
            artists_all,
            artists,
            albums_all: Vec::new(),
            albums: Vec::new(),
            songs_all: Vec::new(),
            songs: Vec::new(),
            artist_index: 0,
            album_index: 0,
            song_index: 0,
            artist_filter: String::new(),
            album_filter: String::new(),
            song_filter: String::new(),
            album_scope: AlbumScope::All,
            song_scope: SongScope::All,
            back_stack: Vec::new(),
        }
    }

    pub fn active_tab(&self) -> Tab {
        self.active_tab
    }

    pub fn artists(&self) -> &[Artist] {
        &self.artists
    }

    pub fn albums(&self) -> &[Album] {
        &self.albums
    }

    pub fn songs(&self) -> &[Song] {
        &self.songs
    }

    pub fn selected_artist(&self) -> Option<&Artist> {
        self.artists.get(self.artist_index)
    }

    pub fn selected_artist_index(&self) -> usize {
        self.artist_index
    }

    pub fn selected_album(&self) -> Option<&Album> {
        self.albums.get(self.album_index)
    }

    pub fn selected_album_index(&self) -> usize {
        self.album_index
    }

    pub fn selected_song(&self) -> Option<&Song> {
        self.songs.get(self.song_index)
    }

    pub fn selected_song_index(&self) -> usize {
        self.song_index
    }

    pub fn active_filter(&self) -> &str {
        match self.active_tab {
            Tab::Artists => &self.artist_filter,
            Tab::Albums => &self.album_filter,
            Tab::Songs => &self.song_filter,
        }
    }

    pub fn active_len(&self) -> usize {
        match self.active_tab {
            Tab::Artists => self.artists.len(),
            Tab::Albums => self.albums.len(),
            Tab::Songs => self.songs.len(),
        }
    }

    pub fn set_filter_for_active_tab(
        &mut self,
        query: String,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<(), ValidateError> {
        match self.active_tab {
            Tab::Artists => {
                if self.artist_filter == query {
                    return Ok(());
                }
                self.artist_filter = query;
                self.apply_artist_filter();
            }
            Tab::Albums => {
                if self.album_filter == query {
                    return Ok(());
                }
                self.album_filter = query;
                self.reload_albums(cache, client)?;
            }
            Tab::Songs => {
                if self.song_filter == query {
                    return Ok(());
                }
                self.song_filter = query;
                self.reload_songs(cache, client)?;
            }
        }
        Ok(())
    }

    pub fn go_to_tab(
        &mut self,
        target: Tab,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<(), ValidateError> {
        self.set_tab_and_reset(target, cache, client)
    }

    pub fn handle_action(
        &mut self,
        action: Action,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<Outcome, ValidateError> {
        match action {
            Action::Tab => {
                self.set_tab_and_reset(self.active_tab.next(), cache, client)?;
                Ok(Outcome::None)
            }
            Action::Up => {
                self.move_cursor_up();
                Ok(Outcome::None)
            }
            Action::Down => {
                self.move_cursor_down();
                Ok(Outcome::None)
            }
            Action::RightOrEnter => self.activate_selection(cache, client),
            Action::Left => {
                self.navigate_back(cache, client)?;
                Ok(Outcome::None)
            }
        }
    }

    fn activate_selection(
        &mut self,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<Outcome, ValidateError> {
        match self.active_tab {
            Tab::Artists => {
                if let Some(artist) = self.selected_artist().cloned() {
                    self.push_snapshot();
                    self.active_tab = Tab::Albums;
                    self.album_scope = AlbumScope::Artist(artist.id);
                    self.song_scope = SongScope::All;
                    self.reload_albums(cache, client)?;
                    self.album_index = 0;
                }
                Ok(Outcome::None)
            }
            Tab::Albums => {
                if let Some(album) = self.selected_album().cloned() {
                    self.push_snapshot();
                    self.active_tab = Tab::Songs;
                    self.song_scope = SongScope::Album(album.id);
                    self.reload_songs(cache, client)?;
                    self.song_index = 0;
                }
                Ok(Outcome::None)
            }
            Tab::Songs => Ok(self
                .selected_song()
                .cloned()
                .map(Outcome::Play)
                .unwrap_or(Outcome::None)),
        }
    }

    fn navigate_back(
        &mut self,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<(), ValidateError> {
        if let Some(prev) = self.back_stack.pop() {
            self.active_tab = prev.active_tab;
            self.album_scope = prev.album_scope;
            self.song_scope = prev.song_scope;
            self.artist_index = prev.artist_index;
            self.album_index = prev.album_index;
            self.song_index = prev.song_index;
            self.ensure_loaded_for_active_tab(cache, client)?;
            return Ok(());
        }

        // Fallback back-navigation when not in a drilled stack context.
        match self.active_tab {
            Tab::Artists => {}
            Tab::Albums => {
                self.active_tab = Tab::Artists;
                self.album_scope = AlbumScope::All;
                self.song_scope = SongScope::All;
            }
            Tab::Songs => {
                self.active_tab = Tab::Albums;
                self.song_scope = SongScope::All;
                self.reload_albums(cache, client)?;
            }
        }
        Ok(())
    }

    fn ensure_loaded_for_active_tab(
        &mut self,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<(), ValidateError> {
        match self.active_tab {
            Tab::Artists => {
                self.apply_artist_filter();
                Ok(())
            }
            Tab::Albums => self.reload_albums(cache, client),
            Tab::Songs => self.reload_songs(cache, client),
        }
    }

    fn set_tab_and_reset(
        &mut self,
        target: Tab,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<(), ValidateError> {
        self.active_tab = target;
        self.album_scope = AlbumScope::All;
        self.song_scope = SongScope::All;
        self.artist_index = 0;
        self.album_index = 0;
        self.song_index = 0;
        self.artist_filter.clear();
        self.album_filter.clear();
        self.song_filter.clear();
        self.back_stack.clear();
        self.ensure_loaded_for_active_tab(cache, client)
    }

    fn reload_albums(
        &mut self,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<(), ValidateError> {
        self.albums_all = match &self.album_scope {
            AlbumScope::All => cache.all_albums(client)?.to_vec(),
            AlbumScope::Artist(artist_id) => cache.albums_for_artist(client, artist_id)?.to_vec(),
        };
        self.apply_album_filter();
        Ok(())
    }

    fn reload_songs(
        &mut self,
        cache: &mut LibraryCache,
        client: &SubsonicClient,
    ) -> Result<(), ValidateError> {
        let mut songs_all = match &self.song_scope {
            SongScope::All => cache.all_songs(client)?.to_vec(),
            SongScope::Album(album_id) => cache.songs_for_album(client, album_id)?.to_vec(),
        };

        if let AlbumScope::Artist(artist_id) = &self.album_scope {
            songs_all.retain(|song| song.has_artist_id(artist_id));
        }

        self.songs_all = songs_all;
        self.apply_song_filter();
        Ok(())
    }

    fn apply_artist_filter(&mut self) {
        self.artists = if let Some(query_lower) = normalized_filter(&self.artist_filter) {
            self.artists_all
                .iter()
                .filter(|artist| contains_ci_lower(&artist.name, &query_lower))
                .cloned()
                .collect()
        } else {
            self.artists_all.clone()
        };
        clamp_index(&mut self.artist_index, self.artists.len());
    }

    fn apply_album_filter(&mut self) {
        self.albums = if let Some(query_lower) = normalized_filter(&self.album_filter) {
            self.albums_all
                .iter()
                .filter(|album| {
                    contains_ci_lower(&album.title, &query_lower)
                        || contains_ci_lower(&album.artist_name, &query_lower)
                })
                .cloned()
                .collect()
        } else {
            self.albums_all.clone()
        };
        clamp_index(&mut self.album_index, self.albums.len());
    }

    fn apply_song_filter(&mut self) {
        self.songs = if let Some(query_lower) = normalized_filter(&self.song_filter) {
            self.songs_all
                .iter()
                .filter(|song| {
                    contains_ci_lower(&song.title, &query_lower)
                        || contains_ci_lower(&song.artist_name, &query_lower)
                        || contains_ci_lower(&song.album_title, &query_lower)
                })
                .cloned()
                .collect()
        } else {
            self.songs_all.clone()
        };
        clamp_index(&mut self.song_index, self.songs.len());
    }

    fn push_snapshot(&mut self) {
        self.back_stack.push(Snapshot {
            active_tab: self.active_tab,
            album_scope: self.album_scope.clone(),
            song_scope: self.song_scope.clone(),
            artist_index: self.artist_index,
            album_index: self.album_index,
            song_index: self.song_index,
        });
    }

    fn move_cursor_up(&mut self) {
        match self.active_tab {
            Tab::Artists => {
                if self.artist_index > 0 {
                    self.artist_index -= 1;
                } else if !self.artists.is_empty() {
                    self.artist_index = self.artists.len() - 1;
                }
            }
            Tab::Albums => {
                if self.album_index > 0 {
                    self.album_index -= 1;
                } else if !self.albums.is_empty() {
                    self.album_index = self.albums.len() - 1;
                }
            }
            Tab::Songs => {
                if self.song_index > 0 {
                    self.song_index -= 1;
                } else if !self.songs.is_empty() {
                    self.song_index = self.songs.len() - 1;
                }
            }
        }
    }

    fn move_cursor_down(&mut self) {
        match self.active_tab {
            Tab::Artists => {
                if self.artist_index + 1 < self.artists.len() {
                    self.artist_index += 1;
                } else if !self.artists.is_empty() {
                    self.artist_index = 0;
                }
            }
            Tab::Albums => {
                if self.album_index + 1 < self.albums.len() {
                    self.album_index += 1;
                } else if !self.albums.is_empty() {
                    self.album_index = 0;
                }
            }
            Tab::Songs => {
                if self.song_index + 1 < self.songs.len() {
                    self.song_index += 1;
                } else if !self.songs.is_empty() {
                    self.song_index = 0;
                }
            }
        }
    }
}

fn clamp_index(index: &mut usize, len: usize) {
    if len == 0 {
        *index = 0;
    } else if *index >= len {
        *index = len - 1;
    }
}

fn normalized_filter(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_lowercase())
    }
}

fn contains_ci_lower(haystack: &str, needle_lower: &str) -> bool {
    haystack.to_lowercase().contains(needle_lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_cycles_in_expected_order() {
        assert_eq!(Tab::Artists.next(), Tab::Albums);
        assert_eq!(Tab::Albums.next(), Tab::Songs);
        assert_eq!(Tab::Songs.next(), Tab::Artists);
    }

    #[test]
    fn clamp_index_bounds_correctly() {
        let mut idx = 5;
        clamp_index(&mut idx, 0);
        assert_eq!(idx, 0);

        let mut idx = 5;
        clamp_index(&mut idx, 3);
        assert_eq!(idx, 2);

        let mut idx = 1;
        clamp_index(&mut idx, 3);
        assert_eq!(idx, 1);
    }

    #[test]
    fn normalized_filter_trims_and_skips_empty_queries() {
        assert_eq!(normalized_filter(""), None);
        assert_eq!(normalized_filter("   "), None);
        assert_eq!(normalized_filter("  Rock  "), Some("rock".to_string()));
    }
}
