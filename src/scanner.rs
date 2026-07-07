use std::path::Path;
use lofty::prelude::*;
use lofty::probe::Probe;
use walkdir::WalkDir;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn is_audio_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(
            ext.to_lowercase().as_str(),
            "mp3" | "flac" | "ogg" | "wav" | "m4a" | "aac"
        ),
        None => false,
    }
}

// ── Track data extracted from a file ─────────────────────────────────────────

pub struct ScannedTrack {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub file_path: String,
    pub duration: Option<i64>,
    pub track_number: Option<i64>,
    pub kind: Option<String>,
}

// ── Process a single file ─────────────────────────────────────────────────────

pub fn process_file(path: &Path) -> Option<ScannedTrack> {
    let tagged_file = match Probe::open(path).and_then(|p| p.read()) {
        Ok(f) => f,
        Err(e) => {
            println!("⚠️  Could not read {:?}: {}", path, e);
            return None;
        }
    };

    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());

    let title = tag
        .and_then(|t| t.title().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown Title")
                .to_string()
        });

    let artist = tag
        .and_then(|t| t.artist().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown Artist".to_string());

    let album = tag.and_then(|t| t.album().map(|s| s.to_string()));

    let track_number = tag.and_then(|t| t.track()).map(|n| n as i64);

    let duration = tagged_file
        .properties()
        .duration()
        .as_secs()
        .try_into()
        .ok();

    Some(ScannedTrack {
        title,
        artist,
        album,
        file_path: path.to_string_lossy().to_string(),
        duration,
        track_number,
        kind: None,
    })
}

// ── Walk a directory and process all audio files ──────────────────────────────

pub fn scan_directory(dir: &Path) -> Vec<ScannedTrack> {
    let mut tracks = Vec::new();

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_audio_file(e.path()))
    {
        println!("🎵 Found: {:?}", entry.path());
        if let Some(track) = process_file(entry.path()) {
            println!("   ✅ {} — {} ", track.artist, track.title);
            tracks.push(track);
        }
    }

    tracks
}
