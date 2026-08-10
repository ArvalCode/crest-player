use std::collections::HashSet;
use std::process::{Command, Stdio};

pub struct Recommendation {
    pub title: String,
    pub video_id: String,
}

pub fn youtube_mix_recommendation(
    title: &str,
    known_video_id: Option<&str>,
    excluded_video_ids: &[String],
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
            "2:25",
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

    choose_recommendation(
        &String::from_utf8_lossy(&output.stdout),
        &seed_id,
        excluded_video_ids,
    )
    .ok_or_else(|| "YouTube Mix returned no new related tracks".to_string())
}

fn choose_recommendation(
    output: &str,
    seed_id: &str,
    excluded_video_ids: &[String],
) -> Option<Recommendation> {
    let excluded: HashSet<&str> = excluded_video_ids.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let mut candidates: Vec<_> = output
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.to_string();
            let title = item.get("title")?.as_str()?.to_string();
            (id != seed_id
                && !excluded.contains(id.as_str())
                && !id.is_empty()
                && !title.is_empty()
                && seen.insert(id.clone()))
            .then_some(Recommendation {
                title,
                video_id: id,
            })
        })
        .collect();
    if candidates.is_empty() {
        None
    } else {
        Some(candidates.swap_remove(fastrand::usize(..candidates.len())))
    }
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

#[cfg(test)]
mod tests {
    use super::choose_recommendation;

    #[test]
    fn recommendation_excludes_seed_history_and_duplicates() {
        let output = concat!(
            r#"{"id":"seed","title":"Seed"}"#,
            "\n",
            r#"{"id":"old","title":"Old"}"#,
            "\n",
            r#"{"id":"new","title":"New"}"#,
            "\n",
            r#"{"id":"new","title":"Duplicate"}"#,
            "\n",
        );
        let recommendation = choose_recommendation(output, "seed", &["old".to_string()]).unwrap();
        assert_eq!(recommendation.video_id, "new");
        assert_eq!(recommendation.title, "New");
    }
}
