use crate::{App, Player};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient, activity};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CLIENT_ID_ENV: &str = "CREST_DISCORD_CLIENT_ID";
const DEFAULT_CLIENT_ID: &str = "1537276059186233497";
const LARGE_IMAGE_URL: &str = "https://cdn.discordapp.com/app-icons/1537276059186233497/137374259d43ff62a94f11b3469499c7.png?size=1024";
const PROJECT_URL: &str = "https://github.com/ArvalCode/crest-player";
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(1);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

pub fn is_configured() -> bool {
    valid_client_id(&client_id())
}

fn client_id() -> String {
    std::env::var(CLIENT_ID_ENV).unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string())
}

fn valid_client_id(value: &str) -> bool {
    value.len() >= 17 && value.chars().all(|character| character.is_ascii_digit())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PresenceState {
    enabled: bool,
    title: Option<String>,
    status: String,
    started_at: Option<i64>,
}

pub struct DiscordPresence {
    sender: Sender<PresenceState>,
    last_state: Option<PresenceState>,
}

impl DiscordPresence {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || run_worker(receiver));
        Self {
            sender,
            last_state: None,
        }
    }

    pub fn sync(&mut self, app: &App, player: &Player) {
        let started_at = (player.status == "Playing")
            .then(|| unix_time_seconds().saturating_sub(player.position().as_secs() as i64));
        let state = PresenceState {
            enabled: app.discord_presence_enabled,
            title: player.title.clone(),
            status: player.status.clone(),
            started_at,
        };
        let materially_changed = self.last_state.as_ref().is_none_or(|last| {
            last.enabled != state.enabled
                || last.title != state.title
                || last.status != state.status
                // The computed start remains stable during normal playback, but
                // changes after a seek and corrects Discord's elapsed timer.
                || timestamps_differ(last.started_at, state.started_at)
        });
        if materially_changed {
            let _ = self.sender.send(state.clone());
            self.last_state = Some(state);
        }
    }
}

fn run_worker(receiver: Receiver<PresenceState>) {
    let client_id = client_id();
    if !valid_client_id(&client_id) {
        while receiver.recv().is_ok() {}
        return;
    }
    let Ok(mut client) = DiscordIpcClient::new(&client_id) else {
        while receiver.recv().is_ok() {}
        return;
    };
    let mut connected = false;
    let mut current: Option<PresenceState> = None;
    let mut dirty = false;
    let mut retry_delay = INITIAL_RETRY_DELAY;
    let mut retry_at = Instant::now();

    loop {
        let timeout =
            if current.as_ref().is_some_and(|state| state.enabled) && (!connected || dirty) {
                retry_at.saturating_duration_since(Instant::now())
            } else {
                Duration::from_secs(30)
            };
        match receiver.recv_timeout(timeout) {
            Ok(mut state) => {
                while let Ok(newer) = receiver.try_recv() {
                    state = newer;
                }
                dirty = current.as_ref() != Some(&state);
                current = Some(state);
                retry_at = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        let Some(state) = current.as_ref() else {
            continue;
        };
        if !state.enabled {
            if connected {
                let _ = client.clear_activity();
                let _ = client.close();
                connected = false;
            }
            dirty = false;
            retry_delay = INITIAL_RETRY_DELAY;
            continue;
        }
        if !connected && Instant::now() >= retry_at {
            match client.connect() {
                Ok(()) => {
                    connected = true;
                    dirty = true;
                    retry_delay = INITIAL_RETRY_DELAY;
                }
                Err(_) => {
                    retry_at = Instant::now() + retry_delay;
                    retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
                }
            }
        }
        if !connected || !dirty {
            continue;
        }
        let result = publish(&mut client, state);
        if result.is_err() {
            let _ = client.close();
            connected = false;
            retry_at = Instant::now() + retry_delay;
            retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
        } else {
            dirty = false;
            retry_delay = INITIAL_RETRY_DELAY;
        }
    }
    if connected {
        let _ = client.clear_activity();
        let _ = client.close();
    }
}

fn publish(
    client: &mut DiscordIpcClient,
    state: &PresenceState,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(title) = state.title.as_deref() else {
        return client.clear_activity();
    };
    let playback_state = match state.status.as_str() {
        "Playing" => "Playing",
        "Paused" => "Paused",
        "Downloading..." => "Preparing playback",
        "Reconnecting audio..." => "Reconnecting audio",
        _ => "Stopped",
    };
    let display_title = discord_text(title, "Unknown track");
    let mut presence = activity::Activity::new()
        .activity_type(activity::ActivityType::Listening)
        .details(&display_title)
        .state(playback_state)
        .assets(
            activity::Assets::new()
                .large_image(LARGE_IMAGE_URL)
                .large_text("Crest Player"),
        )
        .buttons(vec![activity::Button::new("View on GitHub", PROJECT_URL)]);
    if let Some(started_at) = state.started_at {
        presence = presence.timestamps(activity::Timestamps::new().start(started_at));
    }
    client.set_activity(presence)
}

fn unix_time_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn timestamps_differ(left: Option<i64>, right: Option<i64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.abs_diff(right) > 2,
        _ => left != right,
    }
}

fn discord_text(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.chars().count() < 2 {
        fallback.to_string()
    } else {
        value.chars().take(128).collect()
    }
}
