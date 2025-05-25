use csv::Writer;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::{error::Error, time::Duration};
use tokio::time::sleep;

#[derive(Debug, Deserialize)]
struct PaginatedTrackResponse {
    items: Vec<TrackItem>,
    next: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TrackItem {
    track: Option<Track>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Track {
    uri: Option<String>,
    name: Option<String>,
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
    uri: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Album {
    uri: Option<String>,
    name: Option<String>,
    release_date: Option<String>,
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
struct Owner {
    display_name: String,
}

#[derive(Debug, Deserialize)]
struct Tracks {
    href: String,
}

#[derive(Debug)]
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

        let status = res.status();
        let body = res.text().await?;

        if !status.is_success() {
            eprintln!("HTTP {}: {}", status, body);
            return Err(format!("Failed request: {}: {}", status, body).into());
        }

        serde_json::from_str::<T>(&body).map_err(|e| {
            eprintln!("Deserialization error: {}", e);
            eprintln!("Response body: {}", body);
            Box::new(e) as Box<dyn Error>
        })
    }

    async fn get_all_playlists(&self, url: &str) -> Result<Vec<Playlist>, Box<dyn Error>> {
        let mut playlists = Vec::new();
        let mut next = Some(url.to_string());

        while let Some(url) = next {
            let response: PlaylistResponse = self.get(&url).await?;
            playlists.extend(response.items);
            next = response.next;

            if next.is_some() {
                sleep(Duration::from_secs(2)).await;
            }
        }

        Ok(playlists)
    }

    async fn get_playlist_tracks(&self, url: &str) -> Result<Vec<TrackItem>, Box<dyn Error>> {
        let mut all_tracks = Vec::new();
        let mut next_url = Some(url.to_string());

        while let Some(current_url) = next_url {
            let res = self
                .client
                .get(&current_url)
                .header(header::AUTHORIZATION, format!("Bearer {}", self.auth_token))
                .send()
                .await?;

            let status = res.status();
            let body = res.text().await?;

            if !status.is_success() {
                eprintln!("HTTP {}: {}", status, body);
                return Err(format!("Failed request: {}: {}", status, body).into());
            }

            let response: PaginatedTrackResponse = serde_json::from_str(&body)?;
            all_tracks.extend(response.items);
            next_url = response.next;

            if next_url.is_some() {
                sleep(Duration::from_secs(1)).await;
            }
        }

        Ok(all_tracks)
    }
}

async fn export_to_csv(playlists: &[Playlist], api: &SpotifyAPI) -> Result<(), Box<dyn Error>> {
    println!("Exporting playlists to CSV...");

    for playlist in playlists {
        let file_name = format!("{}.csv", playlist.name.replace("/", "_"));
        let mut writer = Writer::from_path(&file_name)?;

        writer.write_record(&[
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

        let tracks = api.get_playlist_tracks(&playlist.tracks.href).await?;

        for track_item in tracks {
            if let Some(track) = track_item.track {
                writer.write_record(&[
                    track.uri.unwrap_or_default(),
                    track.name.unwrap_or_default(),
                    join_artist_uris(&track.artists),
                    join_artist_names(&track.artists),
                    track.album.uri.clone().unwrap_or_default(),
                    track.album.name.clone().unwrap_or_default(),
                    join_artist_uris(&track.album.artists),
                    join_artist_names(&track.album.artists),
                    track
                        .album
                        .release_date
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string()),
                    track
                        .album
                        .images
                        .first()
                        .map_or("No Image".into(), |img| img.url.clone()),
                    track.album.disc_number.unwrap_or(0).to_string(),
                    track.album.track_number.unwrap_or(0).to_string(),
                    track.duration_ms.unwrap_or(0).to_string(),
                    track.preview_url.unwrap_or_default(),
                    track.explicit.unwrap_or(false).to_string(),
                    track.popularity.unwrap_or(0).to_string(),
                    track.isrc.unwrap_or_default(),
                    playlist.owner.display_name.clone(),
                    chrono::Utc::now().to_string(),
                ])?;
            }
        }

        writer.flush()?;
        println!("Finished writing: {}", file_name);
    }

    Ok(())
}

fn join_artist_uris(artists: &[Artist]) -> String {
    artists
        .iter()
        .map(|a| a.uri.clone().unwrap_or_default())
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_artist_names(artists: &[Artist]) -> String {
    artists
        .iter()
        .map(|a| a.name.clone().unwrap_or_default())
        .collect::<Vec<_>>()
        .join(", ")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let token = "Dummy".to_string();
    let api = SpotifyAPI::new(token);

    let playlists = api
        .get_all_playlists("https://api.spotify.com/v1/me/playlists?limit=50")
        .await?;

    export_to_csv(&playlists, &api).await?;
    println!("All playlists backed up successfully.");
    Ok(())
}
