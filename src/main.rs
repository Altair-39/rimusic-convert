use csv::Writer;
use reqwest::{self, header, Client};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TrackItem {
    track: Option<Track>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Track {
    uri: String,
    name: String,
    artists: Vec<Artist>,
    album: Album,
    duration_ms: Option<u64>,
    popularity: Option<u64>,
    isrc: Option<String>,
    preview_url: Option<String>,
    explicit: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Artist {
    uri: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Album {
    uri: String,
    name: String,
    release_date: String,
    artists: Vec<Artist>,
    images: Vec<Image>,
    disc_number: Option<u64>,
    track_number: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Image {
    url: String,
}

#[derive(Debug, Deserialize)]
struct PlaylistResponse {
    items: Vec<Playlist>,
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Playlist {
    name: String,
    owner: Owner,
    tracks: Tracks,
}

#[derive(Debug, Deserialize)]
struct ExternalUrls {}

#[derive(Debug, Deserialize)]
struct Owner {
    display_name: String,
}

#[derive(Debug, Deserialize)]
struct Tracks {
    href: String,
}

#[derive(Debug, Clone)]
struct SpotifyAPI {
    auth_token: String,
    client: Client,
}

impl SpotifyAPI {
    fn new(auth_token: String) -> Self {
        Self {
            auth_token,
            client: Client::new(),
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T, Box<dyn Error>> {
        let res = self
            .client
            .get(url)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let body = res.text().await?;

        match serde_json::from_str::<T>(&body) {
            Ok(deserialized) => Ok(deserialized),
            Err(e) => {
                eprintln!("Failed to deserialize the response: {}", e);
                eprintln!("Response body: {}", body);
                Err(Box::new(e))
            }
        }
    }

    async fn list(&self, url: &str) -> Result<Vec<Playlist>, Box<dyn Error>> {
        let mut playlists = Vec::new();
        let mut next_url = Some(url.to_string());

        while let Some(url) = next_url {
            let response: PlaylistResponse = self.get(&url).await?;
            playlists.extend(response.items);
            next_url = response.next;
            if next_url.is_some() {
                sleep(Duration::from_secs(2)).await;
            }
        }

        Ok(playlists)
    }

    async fn fetch_tracks(&self, url: &str) -> Result<Vec<TrackItem>, Box<dyn Error>> {
        let response: TrackResponse = self.get(url).await?;
        Ok(response.items)
    }
}

#[derive(Debug, Deserialize)]
struct TrackResponse {
    items: Vec<TrackItem>,
}

async fn write_to_csv(playlists: &[Playlist], api: &SpotifyAPI) -> Result<(), Box<dyn Error>> {
    println!("Writing data to CSV...");

    for playlist in playlists {
        let file_name = format!("{}.csv", playlist.name.replace("/", "_"));
        let mut wtr = Writer::from_path(&file_name)?;

        wtr.write_record([
            "Track URI",
            "Track Name",
            "Artist URI(s)",
            "Artist Name(s)",
            "Album URI",
            "Album Name",
            "Album Artist URI(s)",
            "Album Artist Name(s)",
            "Album Release Date",
            "Album Image URL",
            "Disc Number",
            "Track Number",
            "Track Duration (ms)",
            "Track Preview URL",
            "Explicit",
            "Popularity",
            "ISRC",
            "Added By",
            "Added At",
        ])?;

        let tracks_url = &playlist.tracks.href;
        let tracks = api.fetch_tracks(tracks_url).await?;

        for track_item in tracks {
            if let Some(track) = &track_item.track {
                let artists_uris = track
                    .artists
                    .iter()
                    .map(|a| a.uri.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                let artists_names = track
                    .artists
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<String>>()
                    .join(", ");

                let album_artists_uris = track
                    .album
                    .artists
                    .iter()
                    .map(|a| a.uri.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                let album_artists_names = track
                    .album
                    .artists
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<String>>()
                    .join(", ");

                wtr.write_record([
                    &track.uri,
                    &track.name,
                    &artists_uris,
                    &artists_names,
                    &track.album.uri,
                    &track.album.name,
                    &album_artists_uris,
                    &album_artists_names,
                    &track.album.release_date,
                    &track
                        .album
                        .images
                        .first()
                        .map_or("No Image".to_string(), |image| image.url.clone()),
                    &track.album.disc_number.unwrap_or(0).to_string(),
                    &track.album.track_number.unwrap_or(0).to_string(),
                    &track.duration_ms.unwrap_or(0).to_string(),
                    &track.preview_url.clone().unwrap_or_default(),
                    &track.explicit.unwrap_or(false).to_string(),
                    &track.popularity.unwrap_or(0).to_string(),
                    &track.isrc.clone().unwrap_or_default(),
                    &playlist.owner.display_name,
                    &format!("{:?}", chrono::Utc::now()),
                ])?;
            }
        }

        wtr.flush()?;
        println!("CSV written for playlist: {}", playlist.name);
    }

    println!("All CSVs written successfully.");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let token = "dummy".to_string();
    let api = SpotifyAPI::new(token);

    let playlists = api
        .list("https://api.spotify.com/v1/me/playlists?limit=50")
        .await?;

    write_to_csv(&playlists, &api).await?;
    println!("Backup completed successfully!");
    Ok(())
}
