use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Artist {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Album {
    pub id: String,
    pub artist_id: String,
    pub artist_name: String,
    pub title: String,
    pub year: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Song {
    pub id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_id: String,
    pub artist_ids: Vec<String>,
    pub artist_name: String,
    pub title: String,
    pub duration_seconds: Option<u32>,
    pub track: Option<u32>,
}

impl Song {
    pub fn has_artist_id(&self, artist_id: &str) -> bool {
        if artist_id.is_empty() {
            return false;
        }
        self.artist_id == artist_id || self.artist_ids.iter().any(|id| id == artist_id)
    }
}
