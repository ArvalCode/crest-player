
mod app;
mod player;
mod ui_with_player;
mod ui_downloaded_only;
mod draw_startup_screen;
mod lyrics;
mod search;
mod idle_mode;
mod video_screensaver;


use std::io;
use std::time::{Duration, Instant};
use std::process::{Command, Stdio};
use crossterm::{event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use app::{App, save_library};
use player::Player;
use lyrics::{Lyrics, fetch_lyrics};
use ui_with_player::ui_with_player;
use draw_startup_screen::draw_startup_screen;
use search::{search_youtube, download_audio};
use idle_mode::{draw_idle_mode, IdleMode};
use video_screensaver::VideoScreensaver;

struct DownloadFinished {
    title: String,
    path: String,
    autoplay: bool,
    success: bool,
}

fn queue_youtube_download(
    player: &mut Player,
    sender: &std::sync::mpsc::Sender<DownloadFinished>,
    title: &str,
    video_id: &str,
) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let url = format!("https://www.youtube.com/watch?v={video_id}");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = std::env::temp_dir().join(format!("ytmusic_play_{unique}.mp3"));
    let path_string = path.to_string_lossy().into_owned();
    let autoplay = player.child.is_none()
        && !player.queue.iter().any(|(title, _)| title.ends_with("(Downloading...)"));

    player.register_video_source(&path_string, &url);
    player
        .queue
        .push((format!("{title} (Downloading...)"), path_string.clone()));

    let sender = sender.clone();
    let title = title.to_string();
    std::thread::spawn(move || {
        let success = Command::new("yt-dlp")
            .args([
                "-f",
                "bestaudio",
                "-x",
                "--audio-format",
                "mp3",
                "-o",
                &path_string,
                &url,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        let _ = sender.send(DownloadFinished {
            title,
            path: path_string,
            autoplay,
            success,
        });
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    let mut player = Player::new();
    let mut last_tick = Instant::now();
    let mut needs_redraw = true;
    let mut idle_mode = IdleMode::new();
    let mut video_screensaver = VideoScreensaver::new();
    let (lyrics_tx, lyrics_rx) = std::sync::mpsc::channel::<(String, Result<Lyrics, String>)>();
    let (download_tx, download_rx) = std::sync::mpsc::channel::<DownloadFinished>();
    let mut lyrics_requested_for: Option<String> = None;
    let mut lyrics_requested_at: Option<Instant> = None;

    let mut startup_selected = 0; // 0 = stream+downloaded, 1 = downloaded only

    'home: loop {
    // --- Startup screen state ---
    let mut show_startup = true;
    while show_startup {
        terminal.draw(|f| draw_startup_screen(
            f,
            startup_selected,
            app.lyrics_enabled,
            app.live_sync_enabled,
            app.idle_video_enabled,
            app.idle_video_render_mode,
            app.idle_video_fps,
        ))?;
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up => {
                        startup_selected = startup_selected.checked_sub(1).unwrap_or(6);
                    }
                    KeyCode::Down => {
                        startup_selected = (startup_selected + 1) % 7;
                    }
                    KeyCode::Enter => {
                        match startup_selected {
                            0 | 1 => show_startup = false,
                            2 => {
                                app.lyrics_enabled = !app.lyrics_enabled;
                                if !app.lyrics_enabled {
                                    app.lyrics.clear();
                                    app.lyrics_active = None;
                                    lyrics_requested_for = None;
                                    lyrics_requested_at = None;
                                }
                            }
                            3 if app.lyrics_enabled => {
                                app.live_sync_enabled = !app.live_sync_enabled;
                                app.lyrics_active = None;
                                app.lyrics_scroll = 0;
                            }
                            4 => {
                                app.idle_video_enabled = !app.idle_video_enabled;
                                idle_mode.note_activity();
                            }
                            5 => {
                                app.idle_video_render_mode = app.idle_video_render_mode.next();
                            }
                            6 => {
                                app.idle_video_fps = match app.idle_video_fps {
                                    15 => 30,
                                    30 => 60,
                                    _ => 15,
                                };
                            }
                            _ => {}
                        }
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

    loop {
        let playback_keeps_idle_view = player.status == "Playing"
            || (idle_mode.is_visible()
                && matches!(player.status.as_str(), "Paused" | "Downloading..."));
        if idle_mode.update(app.idle_video_enabled && playback_keeps_idle_view) {
            needs_redraw = true;
        }
        let screen = terminal.size()?;
        video_screensaver.update(
            idle_mode.is_visible(),
            player.video_source(),
            player.position(),
            screen.width,
            screen.height,
            (
                app.idle_video_fps,
                app.idle_video_render_mode.samples_per_cell(),
                player.status == "Playing",
            ),
        );
        if needs_redraw {
            if idle_mode.is_visible() {
                terminal.draw(|f| draw_idle_mode(
                    f,
                    idle_mode.stage(),
                    player.title.as_deref(),
                    player.position(),
                    video_screensaver.frame(),
                    app.idle_video_render_mode,
                ))?;
            } else if downloaded_only_mode {
                terminal.draw(|f| ui_downloaded_only::ui_downloaded_only(f, &app, &player))?;
            } else {
                terminal.draw(|f| ui_with_player(f, &app, &player))?;
            }
            needs_redraw = false;
        }

        let tick_rate = if idle_mode.is_visible() {
            Duration::from_micros(1_000_000 / u64::from(app.idle_video_fps))
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
                            && key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                        player.seek_by(5);
                        video_screensaver.restart();
                        true
                    }
                    Event::Key(key)
                        if key.code == KeyCode::Char('-')
                            && key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                        player.seek_by(-5);
                        video_screensaver.restart();
                        true
                    }
                    Event::Key(key)
                        if key.code == KeyCode::Char('p')
                            && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
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
                            && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        player.stop();
                        app.lyrics_active = None;
                        if !player.queue.is_empty() {
                            let (title, path) = player.queue.remove(0);
                            player.play(&path, &title);
                        }
                        video_screensaver.restart();
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
                        (KeyCode::Home, m) if m.is_empty() => {
                            player.stop();
                            player.queue.clear();
                            app.lyrics.clear();
                            app.lyrics_message = "Play a song to load lyrics.".to_string();
                            app.lyrics_active = None;
                            lyrics_requested_for = None;
                            lyrics_requested_at = None;
                            continue 'home;
                        },
                        (KeyCode::PageDown, m) if m.is_empty() => {
                            app.lyrics_scroll = app.lyrics_scroll.saturating_add(5);
                        },
                        (KeyCode::PageUp, m) if m.is_empty() => {
                            app.lyrics_scroll = app.lyrics_scroll.saturating_sub(5);
                        },
                        (KeyCode::Char('+') | KeyCode::Char('='), m) if m.contains(crossterm::event::KeyModifiers::ALT) => {
                            player.seek_by(5);
                        },
                        (KeyCode::Char('-'), m) if m.contains(crossterm::event::KeyModifiers::ALT) => {
                            player.seek_by(-5);
                        },
                        (KeyCode::Down, m) if m.is_empty() => {
                            if !app.results.is_empty() {
                                app.selected = (app.selected + 1).min(app.results.len() - 1);
                            }
                        },
                        (KeyCode::Up, m) if m.is_empty() => {
                            if !app.results.is_empty() && app.selected > 0 {
                                app.selected -= 1;
                            }
                        },
                        (KeyCode::Enter, m) if m.is_empty() => {
                            if !app.results.is_empty() {
                                let (title, path) = &app.results[app.selected];
                                if player.child.is_some() {
                                    player.queue.push((title.clone(), path.clone()));
                                } else {
                                    player.play(path, title);
                                }
                            }
                        },
                        (KeyCode::Char('a'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if !app.results.is_empty() {
                                let (title, path) = &app.results[app.selected];
                                if player.child.is_some() {
                                    player.queue.push((title.clone(), path.clone()));
                                } else {
                                    player.play(path, title);
                                }
                            }
                        },
                        (KeyCode::Char('n'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            player.stop();
                            app.lyrics_active = None;
                            if !player.queue.is_empty() {
                                let (title, url) = player.queue.remove(0);
                                player.play(&url, &title);
                            }
                        },
                        (KeyCode::Char('q'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            player.stop();
                            player.queue.clear();
                            break 'home;
                        },
                        (KeyCode::Char('p'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if player.status == "Playing" {
                                player.pause();
                            } else if player.status == "Paused" {
                                player.resume();
                            }
                        },
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
                    (KeyCode::Home, m) if m.is_empty() => {
                        player.stop();
                        player.queue.clear();
                        app.lyrics.clear();
                        app.lyrics_message = "Play a song to load lyrics.".to_string();
                        app.lyrics_active = None;
                        lyrics_requested_for = None;
                        lyrics_requested_at = None;
                        continue 'home;
                    },
                    (KeyCode::PageDown, m) if m.is_empty() => {
                        app.lyrics_scroll = app.lyrics_scroll.saturating_add(5);
                    },
                    (KeyCode::PageUp, m) if m.is_empty() => {
                        app.lyrics_scroll = app.lyrics_scroll.saturating_sub(5);
                    },
                    (KeyCode::Backspace, m)
                        if m.is_empty() && app.results.is_empty() && !app.searching => {
                        app.input.pop();
                        app.error = None;
                        needs_redraw = true;
                    },
                    (KeyCode::Char('+') | KeyCode::Char('='), m) if m.contains(crossterm::event::KeyModifiers::ALT) => {
                        player.seek_by(5);
                    },
                    (KeyCode::Char('-'), m) if m.contains(crossterm::event::KeyModifiers::ALT) => {
                        player.seek_by(-5);
                    },
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
                    },
                    (KeyCode::Enter, m) if m.is_empty() => {
                        // If results are empty and input is not empty, trigger a search
                        if app.results.is_empty() && !app.input.trim().is_empty() && !app.searching {
                            app.searching = true;
                            let query = app.input.trim().to_string();
                            match search_youtube(&query) {
                                Ok(results) => {
                                    app.results = results;
                                    app.selected = 0;
                                    app.error = None;
                                },
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
                    let (title, id) = &app.results[app.selected];
                    queue_youtube_download(&mut player, &download_tx, title, id);
                    needs_redraw = true;
                            }
                        }
                    },
                    (KeyCode::Char('n'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        // Ctrl+n: Skip to next song in queue
                        player.stop();
                        app.lyrics_active = None;
                        // Play next in queue if available (FIFO order)
                        if !player.queue.is_empty() {
                            let (title, url) = player.queue.remove(0);
                            player.play(&url, &title);
                        }
                        needs_redraw = true;
                    },
                    (KeyCode::Char('q'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        player.stop();
                        player.queue.clear();
                        break 'home;
                    },
                    (KeyCode::Char('p'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        // Ctrl+p: Toggle pause/resume
                        if player.status == "Playing" {
                            player.pause();
                        } else if player.status == "Paused" {
                            player.resume();
                        }
                        needs_redraw = true;
                    },
                    (KeyCode::Char('a'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        // Ctrl+a: Add selected to queue (works for both search results and library)
                        if app.show_library {
                            if !app.library.is_empty() {
                                let (title, path) = &app.library[app.selected];
                                // Add local file to queue (and play immediately if nothing is playing)
                                if player.child.is_some() {
                                    player.queue.push((title.clone(), path.clone()));
                                } else {
                                    player.play(path, title);
                                }
                                needs_redraw = true;
                            }
                        } else if !app.results.is_empty() {
                            let (title, id) = &app.results[app.selected];
                            queue_youtube_download(&mut player, &download_tx, title, id);
                            needs_redraw = true;
                        }
                    },
                    (KeyCode::Char('l'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        // Ctrl+l: Like/download selected
                        if !app.results.is_empty() {
                            let (title, id) = &app.results[app.selected];
                            let url = format!("https://www.youtube.com/watch?v={}", id);
                            if let Some(path) = download_audio(&url, title) {
                                app.library.push((title.clone(), path.to_str().unwrap().to_string()));
                                save_library(&app.library);
                                needs_redraw = true;
                            }
                        }
                    },
                    (KeyCode::Char('v'), m) if m.is_empty() => {
                        // Toggle library view
                        app.show_library = !app.show_library;
                        needs_redraw = true;
                    },
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
                    },
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
                    },
                    (KeyCode::Char(c), m) if m.is_empty() => {
                        if !app.searching && app.results.is_empty() {
                            app.input.push(c);
                            needs_redraw = true;
                        }
                    },
                    (KeyCode::Esc, m) if m.is_empty() && !app.results.is_empty() => {
                        app.results.clear();
                        app.input.clear();
                        app.selected = 0;
                        needs_redraw = true;
                    },
                    _ => {}
                }
            }
        }
        // Only check playback status and redraw if something changed or on tick
        while let Ok(download) = download_rx.try_recv() {
            if let Some(index) = player.queue.iter().position(|(_, path)| path == &download.path) {
                if download.success
                    && (download.autoplay || player.status == "Downloading...")
                    && player.child.is_none()
                {
                    player.queue.remove(index);
                    player.play(&download.path, &download.title);
                    video_screensaver.restart();
                } else if download.success {
                    player.queue[index].0 = download.title;
                } else {
                    player.queue.remove(index);
                    app.error = Some(format!("Failed to download {}", download.title));
                }
                needs_redraw = true;
            }
        }
        let playing_changed = player.is_playing();
        if playing_changed {
            needs_redraw = true;
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
                std::thread::spawn(move || {
                    let result = std::panic::catch_unwind(|| fetch_lyrics(&clean_title))
                        .unwrap_or_else(|_| Err("Lyrics processing failed unexpectedly.".to_string()));
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
            app.lyrics_message = "Lyrics loading timed out. Start the song again to retry.".to_string();
            lyrics_requested_at = None;
            needs_redraw = true;
        }
        if app.lyrics_enabled && app.live_sync_enabled && app.lyrics_synced {
            let position = player.position();
            let active = app.lyrics.iter().rposition(|line| {
                line.timestamp.map(|timestamp| timestamp <= position).unwrap_or(false)
            });
            if active != app.lyrics_active {
                app.lyrics_active = active;
                if let Some(index) = active {
                    let display_row: usize = app.lyrics[..index]
                        .iter()
                        .map(|line| 1 + usize::from(line.romaji.is_some()))
                        .sum();
                    app.lyrics_scroll = display_row.saturating_sub(2).min(u16::MAX as usize) as u16;
                }
                needs_redraw = true;
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
            if idle_mode.is_visible() {
                needs_redraw = true;
            }
        }
    }
    }
// Save and load library to a file in the Music directory
    disable_raw_mode()?;
    execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen)?;

    // (Performance summary output removed)
    Ok(())
}
