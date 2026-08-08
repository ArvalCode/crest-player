use std::process::{Command, Stdio};

pub struct Recommendation {
    pub title: String,
    pub video_id: String,
}

pub fn youtube_mix_recommendation(
    title: &str,
    known_video_id: Option<&str>,
) -> Result<Recommendation, String> {
    let seed_id = match known_video_id {
        Some(id) => id.to_string(),
        None => resolve_video_id(title)?,
    };
    let mix_url = format!("https://www.youtube.com/watch?v={seed_id}&list=RD{seed_id}");
    let output = Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--playlist-items",
            "2:10",
            "--dump-json",
            "--no-warnings",
            &mix_url,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("Unable to query YouTube Mix: {error}"))?;
    if !output.status.success() {
        return Err("YouTube Mix recommendation failed".to_string());
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.to_string();
            let title = item.get("title")?.as_str()?.to_string();
            (id != seed_id && !id.is_empty() && !title.is_empty()).then_some(Recommendation {
                title,
                video_id: id,
            })
        })
        .next()
        .ok_or_else(|| "YouTube Mix returned no related tracks".to_string())
}

fn resolve_video_id(title: &str) -> Result<String, String> {
    let query = format!("ytsearch1:{title}");
    let output = Command::new("yt-dlp")
        .args(["--flat-playlist", "--print", "%(id)s", &query])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("Unable to resolve recommendation seed: {error}"))?;
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && !id.is_empty() {
        Ok(id)
    } else {
        Err("Unable to match this track on YouTube".to_string())
    }
}
