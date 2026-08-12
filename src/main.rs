mod app;
mod desktop_integration;
mod download_commands;
mod download_queue_ui;
mod draw_startup_screen;
mod idle_mode;
mod lyrics;
mod player;
mod recommendations;
mod search;
mod security;
mod storage;
mod ui_downloaded_only;
mod ui_with_player;
mod uninstall;
mod video_cache;
mod video_screensaver;
mod wallpaper;

use app::{App, save_library, save_settings};
use crossterm::{
    ExecutableCommand,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{
        BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen,
        SetTitle, disable_raw_mode, enable_raw_mode,
    },
};
use download_commands::DownloadCommand;
use draw_startup_screen::{
    DELETE_MEDIA_SETTING, HOME_OPTION_COUNT, REMOVE_APPLICATION_SETTING, RESET_WALLPAPER_SETTING,
    SETTINGS_OPTION_COUNT, StartupScreenState, draw_startup_screen,
};
use idle_mode::{IdleMode, IdleRenderState, draw_idle_mode};
use lyrics::{Lyrics, fetch_lyrics_with_caption_fallback};
use player::Player;
use ratatui::Terminal;
use ratatui::prelude::CrosstermBackend;
use recommendations::{Recommendation, youtube_mix_recommendation};
use search::{download_audio, search_youtube};
use security::{
    bounded_output, contained_media_path, external_command, valid_media_url, valid_youtube_id,
};
use std::io::{self, BufWriter, Write};
use std::time::{Duration, Instant};
use ui_with_player::ui_with_player;
use video_screensaver::VideoScreensaver;
use wallpaper::HomeWallpaper;

struct FramePacer {
    fps: u16,
    configured_fps: u16,
    fast_frames: u16,
    average_render_micros: f64,
}

impl FramePacer {
    fn new() -> Self {
        Self {
            fps: 60,
            configured_fps: 60,
            fast_frames: 0,
            average_render_micros: 0.0,
        }
    }

    fn target(&mut self, configured: u16) -> u16 {
        if self.configured_fps != configured {
            self.configured_fps = configured;
            self.fps = if configured == 0 { 30 } else { configured };
            self.fast_frames = 0;
            self.average_render_micros = 0.0;
        }
        if configured != 0 {
            self.fps = configured;
        }
        self.fps
    }

    fn record(&mut self, render_time: Duration, configured: u16) {
        if configured != 0 {
            return;
        }
        const FPS_LEVELS: &[u16] = &[15, 20, 24, 30, 45, 60];
        let elapsed = render_time.as_secs_f64() * 1_000_000.0;
        self.average_render_micros = if self.average_render_micros == 0.0 {
            elapsed
        } else {
            self.average_render_micros * 0.9 + elapsed * 0.1
        };
        let budget_micros = 1_000_000.0 / f64::from(self.fps);
        if self.average_render_micros >= budget_micros * 0.9 {
            self.fps = FPS_LEVELS
                .iter()
                .copied()
                .rev()
                .find(|level| *level < self.fps)
                .unwrap_or(15);
            self.fast_frames = 0;
        } else if self.average_render_micros <= budget_micros * 0.5 {
            self.fast_frames = self.fast_frames.saturating_add(1);
            if self.fast_frames >= 120 {
                self.fps = FPS_LEVELS
                    .iter()
                    .copied()
                    .find(|level| *level > self.fps)
                    .unwrap_or(self.fps);
                self.fast_frames = 0;
            }
        } else {
            self.fast_frames = 0;
        }
    }
}

fn draw_synchronized<W, F>(
    terminal: &mut Terminal<CrosstermBackend<W>>,
    render: F,
) -> io::Result<()>
where
    W: Write,
    F: FnOnce(&mut ratatui::Frame),
{
    terminal.backend_mut().execute(BeginSynchronizedUpdate)?;
    let draw_result = terminal.draw(render).map(|_| ());
    let end_result = terminal
        .backend_mut()
        .execute(EndSynchronizedUpdate)
        .map(|_| ());
    draw_result.and(end_result)
}

struct DownloadFinished {
    title: String,
    queue_path: String,
    playback_path: String,
    youtube_url: String,
    duration: Option<Duration>,
    autoplay: bool,
    success: bool,
}

struct LibraryDownloadFinished {
    title: String,
    path: String,
    success: bool,
}

fn queue_youtube_download(
    app: &mut App,
    player: &mut Player,
    sender: &std::sync::mpsc::Sender<DownloadFinished>,
    title: &str,
    video_id: &str,
) {
    if !valid_youtube_id(video_id) {
        app.error = Some("YouTube returned an invalid media identifier.".to_string());
        return;
    }
    let url = format!("https://www.youtube.com/watch?v={video_id}");
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let queue_path = format!("crest-stream-pending:{unique}");
    let autoplay = player.child.is_none()
        && !player
            .queue
            .iter()
            .any(|(title, _)| title.ends_with("(Downloading...)"));

    player.register_video_source(&queue_path, &url);
    player
        .queue
        .push((format!("{title} (Downloading...)"), queue_path.clone()));
    app.start_download(queue_path.clone(), title.to_string());

    let sender = sender.clone();
    let title = title.to_string();
    std::thread::spawn(move || {
        let mut command = external_command("yt-dlp");
        command.args([
            "--socket-timeout",
            "10",
            "--retries",
            "2",
            "--no-playlist",
            "-f",
            "bestaudio/best",
            "--print",
            "%(duration)s",
            "-g",
            &url,
        ]);
        let output = bounded_output(command, 64 * 1024);
        let output_lines: Vec<String> = output
            .as_ref()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let duration = output_lines
            .first()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(Duration::from_secs_f64);
        let playback_path = output_lines
            .iter()
            .find(|path| valid_media_url(path.trim()))
            .cloned()
            .unwrap_or_default();
        let success = output.as_ref().is_ok_and(|output| output.status.success())
            && !playback_path.is_empty();

        let _ = sender.send(DownloadFinished {
            title,
            queue_path,
            playback_path,
            youtube_url: url,
            duration,
            autoplay,
            success,
        });
    });
}

fn process_download_completions(
    receiver: &std::sync::mpsc::Receiver<DownloadFinished>,
    player: &mut Player,
    app: &mut App,
    video_screensaver: &mut VideoScreensaver,
) -> bool {
    let mut changed = false;
    while let Ok(download) = receiver.try_recv() {
        let cancelled = app.finish_download(&download.queue_path);
        if let Some(index) = player
            .queue
            .iter()
            .position(|(_, path)| path == &download.queue_path)
        {
            if cancelled {
                player.queue.remove(index);
                player.download_failed(&download.queue_path);
            } else if download.success
                && (download.autoplay || player.status == "Downloading...")
                && player.child.is_none()
            {
                player.queue.remove(index);
                player.register_video_source(&download.playback_path, &download.youtube_url);
                if let Some(duration) = download.duration {
                    player.register_stream_duration(&download.playback_path, duration);
                }
                player.play(&download.playback_path, &download.title);
                video_screensaver.restart();
            } else if download.success {
                player.register_video_source(&download.playback_path, &download.youtube_url);
                if let Some(duration) = download.duration {
                    player.register_stream_duration(&download.playback_path, duration);
                }
                player.queue[index] = (download.title, download.playback_path);
            } else {
                player.queue.remove(index);
                player.download_failed(&download.queue_path);
                app.error = Some(format!("Failed to download {}", download.title));
            }
            changed = true;
        }
    }
    changed
}

fn queue_library_download(
    app: &mut App,
    sender: &std::sync::mpsc::Sender<LibraryDownloadFinished>,
    url: String,
    title: String,
    video_cache_plan: Option<(u16, u16, u16)>,
) {
    let Some(directory) = dirs::audio_dir() else {
        app.error = Some("The Music directory is unavailable.".to_string());
        return;
    };
    let Ok(path) = contained_media_path(&directory, &title, "_ytmusic.mp3") else {
        app.error =
            Some("The download title could not be converted to a safe filename.".to_string());
        return;
    };
    let path_string = path.to_string_lossy().into_owned();
    if app.is_library_file_available(&path_string) {
        app.error = Some(format!("{title} is already downloaded."));
        return;
    }
    if app.is_downloading(&path_string) {
        app.error = Some(format!("{title} is already downloading."));
        return;
    }
    app.start_download(path_string.clone(), title.clone());
    let sender = sender.clone();
    std::thread::spawn(move || {
        let downloaded = download_audio(&url, &title, video_cache_plan);
        let success = downloaded.is_some();
        let path = downloaded
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or(path_string);
        let _ = sender.send(LibraryDownloadFinished {
            title,
            path,
            success,
        });
    });
}

fn video_cache_plan(app: &App, width: u16, height: u16) -> Option<(u16, u16, u16)> {
    app.idle_video_enabled.then(|| {
        let samples = app.idle_video_render_mode.samples_per_cell();
        (
            width.saturating_mul(samples.0),
            height.saturating_mul(samples.1),
            if app.idle_video_fps == 0 {
                30
            } else {
                app.idle_video_fps
            },
        )
    })
}

fn process_library_download_completions(
    receiver: &std::sync::mpsc::Receiver<LibraryDownloadFinished>,
    app: &mut App,
) -> bool {
    let mut changed = false;
    while let Ok(download) = receiver.try_recv() {
        let cancelled = app.finish_download(&download.path);
        if cancelled {
            let _ = std::fs::remove_file(&download.path);
            let _ = std::fs::remove_file(
                std::path::Path::new(&download.path).with_extension("crestvid"),
            );
        } else if download.success {
            // This also refreshes availability when an indexed file was missing
            // and the user downloaded it again.
            app.add_library_track(download.title, download.path);
            save_library(&app.library);
        } else {
            app.error = Some(format!("Failed to download {}", download.title));
        }
        changed = true;
    }
    changed
}

fn handle_command_line() -> Result<bool, String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    match arguments.as_slice() {
        [] => Ok(false),
        [argument] if argument == "--help" || argument == "-h" => {
            println!("Crest Player {}", env!("CARGO_PKG_VERSION"));
            println!(
                "Terminal music player with YouTube streaming, synchronized lyrics, and ASCII video"
            );
            println!();
            println!("Usage:");
            println!("  crest-player [OPTION]");
            println!();
            println!("Options:");
            println!("  -h, --help         Show this help message and exit");
            println!(
                "      --install-desktop  Install Crest Player and its per-user application launcher"
            );
            println!("      --remove       Interactively remove Crest Player and its data");
            println!("      --storage      Show application and downloaded-media storage usage");
            println!();
            println!("Run without an option to start Crest Player.");
            Ok(true)
        }
        [argument] if argument == "--install-desktop" => {
            desktop_integration::install().map(|_| true)
        }
        [argument] if argument == "--remove" => uninstall::remove_crest_player().map(|_| true),
        [argument] if argument == "--storage" => storage::display_storage().map(|_| true),
        _ => Err(format!(
            "unknown option or argument: {}\nRun 'crest-player --help' for usage.",
            arguments
                .first()
                .map(|argument| argument.to_string_lossy())
                .unwrap_or_default()
        )),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match handle_command_line() {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(error) => {
            eprintln!("crest-player: {error}");
            std::process::exit(2);
        }
    }
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        SetTitle("Crest Player"),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(BufWriter::with_capacity(1024 * 1024, stdout));
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    let mut player = Player::new();
    let mut last_tick = Instant::now();
    let mut needs_redraw = true;
    let mut idle_mode = IdleMode::new();
    let mut video_screensaver = VideoScreensaver::new();
    let mut frame_pacer = FramePacer::new();
    let mut last_rendered_video_frame = 0u64;
    let mut last_rendered_video_second = 0u64;
    let (lyrics_tx, lyrics_rx) = std::sync::mpsc::channel::<(String, Result<Lyrics, String>)>();
    let (download_tx, download_rx) = std::sync::mpsc::channel::<DownloadFinished>();
    let (library_download_tx, library_download_rx) =
        std::sync::mpsc::channel::<LibraryDownloadFinished>();
    let (recommendation_tx, recommendation_rx) =
        std::sync::mpsc::channel::<(String, Result<Recommendation, String>)>();
    let mut autoplay_requested_for: Option<String>;
    let mut lyrics_requested_for: Option<String> = None;
    let mut lyrics_requested_at: Option<Instant> = None;
    let mut autoplay_history: Vec<String> = Vec::new();

    let mut startup_selected = 0; // 0 = stream+downloaded, 1 = downloaded only
    let mut settings_selected = 0;
    let mut removal_requested = false;

    'home: loop {
        // --- Startup screen state ---
        let mut show_startup = true;
        let mut settings_page = false;
        while show_startup {
            // Home remains an audio-capable view, but never starts the video overlay.
            idle_mode.note_activity();
            video_screensaver.restart();
            process_download_completions(
                &download_rx,
                &mut player,
                &mut app,
                &mut video_screensaver,
            );
            process_library_download_completions(&library_download_rx, &mut app);
            player.is_playing();
            draw_synchronized(&mut terminal, |f| {
                draw_startup_screen(
                    f,
                    StartupScreenState {
                        page: (
                            settings_page,
                            if settings_page {
                                settings_selected
                            } else {
                                startup_selected
                            },
                        ),
                        lyric_settings: (
                            app.lyrics_enabled,
                            app.live_sync_enabled,
                            app.pronunciations_enabled,
                        ),
                        video_settings: (
                            app.idle_video_enabled,
                            app.idle_video_render_mode,
                            app.color_precision,
                            app.idle_video_fps,
                            app.hardware_acceleration_enabled,
                        ),
                        autoplay_enabled: app.autoplay_enabled,
                        library_track_count: app.library.len(),
                        home_wallpaper: app.home_wallpaper.as_ref(),
                        playback: (player.title.as_deref(), player.status.as_str()),
                    },
                )
            })?;
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
            {
                match key.code {
                    KeyCode::Up => {
                        if settings_page {
                            settings_selected = settings_selected
                                .checked_sub(1)
                                .unwrap_or(SETTINGS_OPTION_COUNT - 1);
                        } else {
                            startup_selected = startup_selected
                                .checked_sub(1)
                                .unwrap_or(HOME_OPTION_COUNT - 1);
                        }
                    }
                    KeyCode::Down => {
                        if settings_page {
                            settings_selected = (settings_selected + 1) % SETTINGS_OPTION_COUNT;
                        } else {
                            startup_selected = (startup_selected + 1) % HOME_OPTION_COUNT;
                        }
                    }
                    KeyCode::Enter => {
                        if !settings_page {
                            match startup_selected {
                                0 | 1 => show_startup = false,
                                2 => settings_page = true,
                                _ => {}
                            }
                        } else {
                            match settings_selected {
                                0 => {
                                    app.lyrics_enabled = !app.lyrics_enabled;
                                    if !app.lyrics_enabled {
                                        app.lyrics.clear();
                                        app.lyrics_active = None;
                                        lyrics_requested_for = None;
                                        lyrics_requested_at = None;
                                    }
                                }
                                1 if app.lyrics_enabled => {
                                    app.live_sync_enabled = !app.live_sync_enabled;
                                    app.lyrics_active = None;
                                    app.lyrics_scroll = 0;
                                }
                                2 => {
                                    app.pronunciations_enabled = !app.pronunciations_enabled;
                                }
                                3 => {
                                    app.idle_video_enabled = !app.idle_video_enabled;
                                    idle_mode.note_activity();
                                }
                                4 => {
                                    app.idle_video_render_mode = app.idle_video_render_mode.next();
                                }
                                5 => {
                                    app.color_precision = app.color_precision.next();
                                }
                                6 => {
                                    app.idle_video_fps = match app.idle_video_fps {
                                        15 => 24,
                                        24 => 30,
                                        30 => 60,
                                        60 => 0,
                                        _ => 15,
                                    };
                                }
                                7 => {
                                    app.hardware_acceleration_enabled =
                                        !app.hardware_acceleration_enabled;
                                    video_screensaver.restart();
                                }
                                8 => {
                                    app.autoplay_enabled = !app.autoplay_enabled;
                                }
                                DELETE_MEDIA_SETTING => {
                                    app.cancel_active_downloads();
                                    player.stop();
                                    player.cleanup_temp_media();
                                    player.queue.clear();
                                    let errors = app.delete_all_library_media();
                                    save_library(&app.library);
                                    app.results = app.library.clone();
                                    app.error = Some(if errors.is_empty() {
                                        "Deleted all songs and videos tracked by Crest Player."
                                            .to_string()
                                    } else {
                                        format!(
                                            "Some tracked media could not be deleted: {}",
                                            errors.join(", ")
                                        )
                                    });
                                }
                                RESET_WALLPAPER_SETTING => {
                                    if let Err(error) = HomeWallpaper::remove_saved() {
                                        app.error =
                                            Some(format!("Could not reset wallpaper: {error}"));
                                    } else {
                                        app.home_wallpaper = None;
                                    }
                                }
                                REMOVE_APPLICATION_SETTING => {
                                    removal_requested = true;
                                    break 'home;
                                }
                                _ => {}
                            }
                            save_settings(&app);
                        }
                    }
                    KeyCode::Esc if settings_page => {
                        settings_page = false;
                    }
                    KeyCode::Left
                        if settings_page
                            && key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        settings_page = false;
                    }
                    KeyCode::Char('q') => {
                        break 'home;
                    }
                    _ => {}
                }
            }
        }

        let downloaded_only_mode = startup_selected == 1;

        // If "downloaded only" mode, set up the UI for library-only navigation
        if downloaded_only_mode {
            app.results = app.library.clone();
            app.input.clear();
            app.show_library = false; // results panel is now the library
            app.selected = 0;
        } else {
            app.results.clear();
            app.input.clear();
            app.show_library = false;
            app.selected = 0;
        }
        autoplay_requested_for = None;

        loop {
            let playback_keeps_idle_view = player.status == "Playing"
                || (idle_mode.is_visible()
                    && matches!(player.status.as_str(), "Paused" | "Downloading..."));
            if idle_mode.update(app.idle_video_enabled && playback_keeps_idle_view) {
                needs_redraw = true;
            }
            let screen = terminal.size()?;
            let video_preloading = idle_mode.should_preload_video(player.status == "Playing");
            video_screensaver.update(
                idle_mode.is_visible() || video_preloading,
                player.video_source(),
                player.position(),
                screen.width,
                screen.height,
                (
                    if app.idle_video_fps == 0 {
                        30
                    } else {
                        app.idle_video_fps
                    },
                    app.idle_video_render_mode.samples_per_cell(),
                    player.status == "Playing",
                    app.hardware_acceleration_enabled,
                ),
            );
            if needs_redraw {
                let render_started = Instant::now();
                if idle_mode.is_visible() {
                    draw_synchronized(&mut terminal, |f| {
                        draw_idle_mode(
                            f,
                            IdleRenderState {
                                stage: idle_mode.stage(),
                                title: player.title.as_deref(),
                                position: player.position(),
                                video_frame: video_screensaver.frame(),
                                render_mode: app.idle_video_render_mode,
                                color_precision: app.color_precision,
                                synced_lyrics: if app.lyrics_enabled
                                    && app.live_sync_enabled
                                    && app.lyrics_synced
                                    && !app.lyrics.is_empty()
                                {
                                    Some({
                                        let index = app.lyrics_active.unwrap_or(0);
                                        let display_line = |line: &crate::lyrics::LyricLine| {
                                            if app.pronunciations_enabled
                                                && let Some(pronunciation) = &line.romaji
                                            {
                                                format!("{}  ·  {}", line.text, pronunciation)
                                            } else {
                                                line.text.clone()
                                            }
                                        };
                                        (
                                            display_line(&app.lyrics[index]),
                                            app.lyrics.get(index + 1).map(display_line),
                                        )
                                    })
                                } else {
                                    None
                                },
                            },
                        )
                    })?;
                } else if downloaded_only_mode {
                    draw_synchronized(&mut terminal, |f| {
                        ui_downloaded_only::ui_downloaded_only(f, &app, &player)
                    })?;
                } else {
                    draw_synchronized(&mut terminal, |f| ui_with_player(f, &app, &player))?;
                }
                if idle_mode.is_visible() {
                    frame_pacer.record(render_started.elapsed(), app.idle_video_fps);
                    last_rendered_video_frame = video_screensaver.frame_serial();
                    last_rendered_video_second = player.position().as_secs();
                }
                needs_redraw = false;
            }

            let tick_rate = if idle_mode.is_visible() {
                Duration::from_micros(1_000_000 / u64::from(frame_pacer.target(app.idle_video_fps)))
            } else {
                Duration::from_millis(100)
            };
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if event::poll(timeout)? {
                let input_event = event::read()?;
                let was_idle = idle_mode.is_visible();
                if was_idle {
                    let handled_in_cinema = match &input_event {
                        Event::Key(key)
                            if matches!(key.code, KeyCode::Char('+') | KeyCode::Char('='))
                                && key.modifiers.contains(crossterm::event::KeyModifiers::ALT) =>
                        {
                            player.seek_by(5);
                            video_screensaver.seek_to(player.position());
                            true
                        }
                        Event::Key(key)
                            if key.code == KeyCode::Char('-')
                                && key.modifiers.contains(crossterm::event::KeyModifiers::ALT) =>
                        {
                            player.seek_by(-5);
                            video_screensaver.seek_to(player.position());
                            true
                        }
                        Event::Key(key)
                            if key.code == KeyCode::Char('p')
                                && key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            if player.status == "Playing" {
                                player.pause();
                            } else if player.status == "Paused" {
                                player.resume();
                                video_screensaver.restart();
                            }
                            true
                        }
                        Event::Key(key)
                            if key.code == KeyCode::Char('n')
                                && key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            player.stop();
                            app.lyrics_active = None;
                            if !player.queue.is_empty() {
                                let (title, path) = player.queue.remove(0);
                                player.play(&path, &title);
                            }
                            video_screensaver.restart();
                            true
                        }
                        Event::Key(key)
                            if key.code == KeyCode::Char('`') && key.modifiers.is_empty() =>
                        {
                            if let Some(frame) = video_screensaver.frame().cloned() {
                                let wallpaper = HomeWallpaper::capture(
                                    &frame,
                                    app.idle_video_render_mode,
                                    app.color_precision,
                                );
                                if let Err(error) = wallpaper.save() {
                                    app.error = Some(format!("Could not save wallpaper: {error}"));
                                } else {
                                    app.home_wallpaper = Some(wallpaper);
                                }
                            }
                            true
                        }
                        _ => false,
                    };
                    if handled_in_cinema {
                        needs_redraw = true;
                        continue;
                    }
                }
                let is_mouse_move = matches!(
                    &input_event,
                    Event::Mouse(mouse) if mouse.kind == MouseEventKind::Moved
                );
                if is_mouse_move {
                    // Pointer motion alone should not reset inactivity or dismiss the
                    // screensaver. Clicks and scrolling remain intentional input.
                    continue;
                }
                idle_mode.note_activity();
                needs_redraw = true;
                // Waking the UI consumes the event so a stray key cannot also edit,
                // seek, or activate the currently selected item.
                if was_idle {
                    continue;
                }
                if let Event::Key(key) = input_event {
                    needs_redraw = true;
                    if downloaded_only_mode {
                        // Only allow navigation and playback in the downloaded songs list (results panel)
                        match (key.code, key.modifiers) {
                            (KeyCode::Left, m)
                                if m.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                idle_mode.note_activity();
                                video_screensaver.restart();
                                continue 'home;
                            }
                            (KeyCode::PageDown, m) if m.is_empty() => {
                                app.lyrics_scroll = app.lyrics_scroll.saturating_add(5);
                            }
                            (KeyCode::PageUp, m) if m.is_empty() => {
                                app.lyrics_scroll = app.lyrics_scroll.saturating_sub(5);
                            }
                            (KeyCode::Backspace, m) if m.is_empty() => {
                                app.input.pop();
                                app.error = None;
                            }
                            (KeyCode::Esc, m) if m.is_empty() => {
                                app.input.clear();
                                app.error = None;
                            }
                            (KeyCode::Char('+') | KeyCode::Char('='), m)
                                if m.contains(crossterm::event::KeyModifiers::ALT) =>
                            {
                                player.seek_by(5);
                                video_screensaver.seek_to(player.position());
                            }
                            (KeyCode::Char('-'), m)
                                if m.contains(crossterm::event::KeyModifiers::ALT) =>
                            {
                                player.seek_by(-5);
                                video_screensaver.seek_to(player.position());
                            }
                            (KeyCode::Down, m) if m.is_empty() => {
                                if !app.results.is_empty() {
                                    app.selected = (app.selected + 1).min(app.results.len() - 1);
                                }
                            }
                            (KeyCode::Up, m) if m.is_empty() => {
                                if !app.results.is_empty() && app.selected > 0 {
                                    app.selected -= 1;
                                }
                            }
                            (KeyCode::Enter, m) if m.is_empty() => {
                                if !app.input.trim().is_empty() {
                                    app.error = Some(match DownloadCommand::parse(&app.input) {
                                        Ok(command) => {
                                            command.execute(&app.library, &mut player.queue)
                                        }
                                        Err(message) => message,
                                    });
                                    app.input.clear();
                                } else if !app.results.is_empty() {
                                    let (title, path) = &app.results[app.selected];
                                    if player.child.is_some() {
                                        player.queue.push((title.clone(), path.clone()));
                                    } else {
                                        player.play(path, title);
                                    }
                                }
                            }
                            (KeyCode::Char(character), m) if m.is_empty() => {
                                if !app.input.is_empty() || character == ':' {
                                    app.input.push(character);
                                    app.error = None;
                                }
                            }
                            (KeyCode::Delete, m) if m.is_empty() => {
                                if !app.results.is_empty() {
                                    let path = app.results[app.selected].1.clone();
                                    if let Err(error) = app.remove_library_track(&path) {
                                        app.error = Some(format!("Could not delete song: {error}"));
                                    } else {
                                        app.results.retain(|(_, result_path)| result_path != &path);
                                        app.selected =
                                            app.selected.min(app.results.len().saturating_sub(1));
                                        save_library(&app.library);
                                    }
                                }
                            }
                            (KeyCode::Char('n'), m)
                                if m.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                player.stop();
                                app.lyrics_active = None;
                                if !player.queue.is_empty() {
                                    let (title, url) = player.queue.remove(0);
                                    player.play(&url, &title);
                                }
                            }
                            (KeyCode::Char('q'), m)
                                if m.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                player.stop();
                                player.queue.clear();
                                break 'home;
                            }
                            (KeyCode::Char('p'), m)
                                if m.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                if player.status == "Playing" {
                                    player.pause();
                                } else if player.status == "Paused" {
                                    player.resume();
                                }
                            }
                            _ => {}
                        }
                        let playing_changed = player.is_playing();
                        if playing_changed {
                            needs_redraw = true;
                        }
                        if last_tick.elapsed() >= tick_rate {
                            last_tick = Instant::now();
                        }
                        continue;
                    }
                    match (key.code, key.modifiers) {
                        (KeyCode::Left, m)
                            if m.contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            idle_mode.note_activity();
                            video_screensaver.restart();
                            continue 'home;
                        }
                        (KeyCode::PageDown, m) if m.is_empty() => {
                            app.lyrics_scroll = app.lyrics_scroll.saturating_add(5);
                        }
                        (KeyCode::PageUp, m) if m.is_empty() => {
                            app.lyrics_scroll = app.lyrics_scroll.saturating_sub(5);
                        }
                        (KeyCode::Backspace, m)
                            if m.is_empty() && app.results.is_empty() && !app.searching =>
                        {
                            app.input.pop();
                            app.error = None;
                            needs_redraw = true;
                        }
                        (KeyCode::Char('+') | KeyCode::Char('='), m)
                            if m.contains(crossterm::event::KeyModifiers::ALT) =>
                        {
                            player.seek_by(5);
                            video_screensaver.seek_to(player.position());
                        }
                        (KeyCode::Char('-'), m)
                            if m.contains(crossterm::event::KeyModifiers::ALT) =>
                        {
                            player.seek_by(-5);
                            video_screensaver.seek_to(player.position());
                        }
                        // Special case: if user types exactly :library, show library in results
                        (KeyCode::Char(c), m) if m.is_empty() => {
                            if !app.searching && app.results.is_empty() {
                                app.input.push(c);
                                needs_redraw = true;
                                if app.input == ":library" {
                                    app.results = app.library.clone();
                                    app.selected = 0;
                                    app.show_library = false;
                                    app.input.clear();
                                }
                            }
                        }
                        (KeyCode::Enter, m) if m.is_empty() => {
                            // If results are empty and input is not empty, trigger a search
                            if app.results.is_empty()
                                && !app.input.trim().is_empty()
                                && !app.searching
                            {
                                app.searching = true;
                                let query = app.input.trim().to_string();
                                match search_youtube(&query) {
                                    Ok(results) => {
                                        app.results = results;
                                        app.selected = 0;
                                        app.error = None;
                                    }
                                    Err(e) => {
                                        app.error = Some(e);
                                    }
                                }
                                app.searching = false;
                                needs_redraw = true;
                            } else {
                                // Play or queue selected from results or library
                                if app.show_library {
                                    if !app.library.is_empty() {
                                        let (title, path) = &app.library[app.selected];
                                        if player.child.is_some() {
                                            player.queue.push((title.clone(), path.clone()));
                                        } else {
                                            player.play(path, title);
                                        }
                                        needs_redraw = true;
                                    }
                                } else if !app.results.is_empty() {
                                    let (title, id) = app.results[app.selected].clone();
                                    queue_youtube_download(
                                        &mut app,
                                        &mut player,
                                        &download_tx,
                                        &title,
                                        &id,
                                    );
                                    needs_redraw = true;
                                }
                            }
                        }
                        (KeyCode::Char('n'), m)
                            if m.contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            // Ctrl+n: Skip to next song in queue
                            player.stop();
                            app.lyrics_active = None;
                            // Play next in queue if available (FIFO order)
                            if !player.queue.is_empty() {
                                let (title, url) = player.queue.remove(0);
                                player.play(&url, &title);
                            }
                            needs_redraw = true;
                        }
                        (KeyCode::Char('q'), m)
                            if m.contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            player.stop();
                            player.queue.clear();
                            break 'home;
                        }
                        (KeyCode::Char('p'), m)
                            if m.contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            // Ctrl+p: Toggle pause/resume
                            if player.status == "Playing" {
                                player.pause();
                            } else if player.status == "Paused" {
                                player.resume();
                            }
                            needs_redraw = true;
                        }
                        (KeyCode::Delete, m) if m.is_empty() && app.show_library => {
                            if !app.library.is_empty() {
                                let path = app.library[app.selected].1.clone();
                                if let Err(error) = app.remove_library_track(&path) {
                                    app.error = Some(format!("Could not delete song: {error}"));
                                } else {
                                    app.selected =
                                        app.selected.min(app.library.len().saturating_sub(1));
                                    save_library(&app.library);
                                }
                                needs_redraw = true;
                            }
                        }
                        (KeyCode::Char('l'), m)
                            if m.contains(crossterm::event::KeyModifiers::CONTROL)
                                && !app.show_library =>
                        {
                            // Ctrl+l: Like/download selected
                            if let Some((title, id)) = app.results.get(app.selected).cloned() {
                                let url = format!("https://www.youtube.com/watch?v={}", id);
                                let video_cache_plan =
                                    video_cache_plan(&app, screen.width, screen.height);
                                queue_library_download(
                                    &mut app,
                                    &library_download_tx,
                                    url,
                                    title,
                                    video_cache_plan,
                                );
                                needs_redraw = true;
                            }
                        }
                        (KeyCode::Char('v'), m) if m.is_empty() => {
                            // Toggle library view
                            app.show_library = !app.show_library;
                            needs_redraw = true;
                        }
                        // j/k navigation removed
                        (KeyCode::Down, m) if m.is_empty() => {
                            if app.show_library {
                                if !app.library.is_empty() {
                                    app.selected = (app.selected + 1).min(app.library.len() - 1);
                                    needs_redraw = true;
                                }
                            } else if !app.results.is_empty() {
                                app.selected = (app.selected + 1).min(app.results.len() - 1);
                                needs_redraw = true;
                            }
                        }
                        // j/k navigation removed
                        (KeyCode::Up, m) if m.is_empty() => {
                            if app.show_library {
                                if !app.library.is_empty() && app.selected > 0 {
                                    app.selected -= 1;
                                    needs_redraw = true;
                                }
                            } else if !app.results.is_empty() && app.selected > 0 {
                                app.selected -= 1;
                                needs_redraw = true;
                            }
                        }
                        (KeyCode::Char(c), m) if m.is_empty() => {
                            if !app.searching && app.results.is_empty() {
                                app.input.push(c);
                                needs_redraw = true;
                            }
                        }
                        (KeyCode::Esc, m) if m.is_empty() && !app.results.is_empty() => {
                            app.results.clear();
                            app.input.clear();
                            app.selected = 0;
                            needs_redraw = true;
                        }
                        _ => {}
                    }
                }
            }
            // Only check playback status and redraw if something changed or on tick
            if process_download_completions(
                &download_rx,
                &mut player,
                &mut app,
                &mut video_screensaver,
            ) {
                needs_redraw = true;
            }
            if process_library_download_completions(&library_download_rx, &mut app) {
                needs_redraw = true;
            }
            while let Ok((seed_title, result)) = recommendation_rx.try_recv() {
                let seed_is_relevant = player.title.as_ref() == Some(&seed_title)
                    || player.last_finished_title() == Some(seed_title.as_str());
                if app.autoplay_enabled
                    && seed_is_relevant
                    && player.queue.is_empty()
                    && let Ok(recommendation) = result
                {
                    autoplay_history.push(recommendation.video_id.clone());
                    if autoplay_history.len() > 20 {
                        autoplay_history.remove(0);
                    }
                    queue_youtube_download(
                        &mut app,
                        &mut player,
                        &download_tx,
                        &recommendation.title,
                        &recommendation.video_id,
                    );
                    needs_redraw = true;
                }
            }
            let playing_changed = player.is_playing();
            if playing_changed {
                needs_redraw = true;
            }

            if app.autoplay_enabled && player.status == "Playing" {
                if let Some(title) = player.title.clone()
                    && autoplay_requested_for.as_ref() != Some(&title)
                    && player.queue.is_empty()
                    && player.position() >= Duration::from_secs(30)
                {
                    autoplay_requested_for = Some(title.clone());
                    let video_id = player.current_video_id();
                    if let Some(video_id) = video_id.as_ref()
                        && !autoplay_history.contains(video_id)
                    {
                        autoplay_history.push(video_id.clone());
                        if autoplay_history.len() > 20 {
                            autoplay_history.remove(0);
                        }
                    }
                    let excluded_video_ids = autoplay_history.clone();
                    let tx = recommendation_tx.clone();
                    std::thread::spawn(move || {
                        let result = youtube_mix_recommendation(
                            &title,
                            video_id.as_deref(),
                            &excluded_video_ids,
                        );
                        let _ = tx.send((title, result));
                    });
                }
            } else if player.title.is_none() {
                autoplay_requested_for = None;
            }

            if app.lyrics_enabled {
                if let Some(title) = player.title.as_ref() {
                    let clean_title = title.trim_end_matches(" (Downloading...)").to_string();
                    if lyrics_requested_for.as_ref() != Some(&clean_title) {
                        lyrics_requested_for = Some(clean_title.clone());
                        lyrics_requested_at = Some(Instant::now());
                        app.lyrics.clear();
                        app.lyrics_message = "Loading synchronized lyrics...".to_string();
                        app.lyrics_synced = false;
                        app.lyrics_active = None;
                        app.lyrics_scroll = 0;
                        let tx = lyrics_tx.clone();
                        let video_source = player.video_source().unwrap_or_else(|| {
                            std::sync::Arc::from(format!(
                                "ytsearch1:{clean_title} official music video"
                            ))
                        });
                        std::thread::spawn(move || {
                            let result = std::panic::catch_unwind(|| {
                                fetch_lyrics_with_caption_fallback(
                                    &clean_title,
                                    video_source.as_ref(),
                                )
                            })
                            .unwrap_or_else(|_| {
                                Err("Lyrics processing failed unexpectedly.".to_string())
                            });
                            let _ = tx.send((clean_title, result));
                        });
                        needs_redraw = true;
                    }
                } else {
                    lyrics_requested_for = None;
                    lyrics_requested_at = None;
                }
            }

            while let Ok((title, result)) = lyrics_rx.try_recv() {
                if lyrics_requested_for.as_ref() == Some(&title) {
                    lyrics_requested_at = None;
                    match result {
                        Ok(lyrics) => {
                            app.lyrics = lyrics.lines;
                            app.lyrics_synced = lyrics.synced;
                            app.lyrics_message = if lyrics.synced {
                                "Synchronized lyrics".to_string()
                            } else {
                                "Plain lyrics (timing unavailable)".to_string()
                            };
                            app.lyrics_active = None;
                        }
                        Err(error) => {
                            app.lyrics.clear();
                            app.lyrics_synced = false;
                            app.lyrics_message = error;
                            app.lyrics_active = None;
                        }
                    }
                    needs_redraw = true;
                }
            }
            if lyrics_requested_at
                .map(|started| started.elapsed() > Duration::from_secs(15))
                .unwrap_or(false)
            {
                app.lyrics.clear();
                app.lyrics_synced = false;
                app.lyrics_active = None;
                app.lyrics_message =
                    "Lyrics loading timed out. Start the song again to retry.".to_string();
                lyrics_requested_at = None;
                needs_redraw = true;
            }
            if app.lyrics_enabled && app.live_sync_enabled && app.lyrics_synced {
                let position = player.position();
                let active = app.lyrics.iter().rposition(|line| {
                    line.timestamp
                        .map(|timestamp| timestamp <= position)
                        .unwrap_or(false)
                });
                if active != app.lyrics_active {
                    app.lyrics_active = active;
                    if let Some(index) = active {
                        let display_row: usize = app.lyrics[..index]
                            .iter()
                            .map(|line| {
                                1 + usize::from(app.pronunciations_enabled && line.romaji.is_some())
                            })
                            .sum();
                        app.lyrics_scroll =
                            display_row.saturating_sub(2).min(u16::MAX as usize) as u16;
                    }
                    needs_redraw = true;
                }
            }
            if last_tick.elapsed() >= tick_rate {
                // Skip every elapsed interval in one step. Advancing only one interval
                // makes a slow render trigger a burst of immediate catch-up frames.
                let elapsed_intervals = (last_tick.elapsed().as_nanos() / tick_rate.as_nanos())
                    .max(1)
                    .min(u32::MAX as u128) as u32;
                last_tick += tick_rate * elapsed_intervals;
                if idle_mode.is_visible() {
                    let video_frame = video_screensaver.frame_serial();
                    let video_second = player.position().as_secs();
                    // Do not rebuild and diff an identical terminal frame when the
                    // decoder or network is temporarily between frames. The fallback
                    // animation remains clock-driven and therefore still redraws.
                    if video_screensaver.frame().is_none()
                        || video_frame != last_rendered_video_frame
                        || video_second != last_rendered_video_second
                    {
                        needs_redraw = true;
                        last_rendered_video_frame = video_frame;
                        last_rendered_video_second = video_second;
                    }
                }
            }
        }
    }
    // Save and load library to a file in the Music directory
    player.stop();
    player.cleanup_temp_media();
    player.queue.clear();
    disable_raw_mode()?;
    execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen)?;

    if removal_requested {
        uninstall::remove_crest_player_from_settings().map_err(io::Error::other)?;
    }

    // (Performance summary output removed)
    Ok(())
}

#[cfg(test)]
mod frame_pacer_tests {
    use super::FramePacer;
    use std::time::Duration;

    #[test]
    fn fixed_fps_never_adapts() {
        let mut pacer = FramePacer::new();
        assert_eq!(pacer.target(60), 60);
        pacer.record(Duration::from_millis(100), 60);
        assert_eq!(pacer.target(60), 60);
    }

    #[test]
    fn auto_fps_reduces_an_unsustainable_rate() {
        let mut pacer = FramePacer::new();
        assert_eq!(pacer.target(0), 30);
        pacer.record(Duration::from_millis(40), 0);
        assert_eq!(pacer.target(0), 24);
    }
}
