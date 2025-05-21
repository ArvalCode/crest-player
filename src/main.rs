
mod app;
mod player;
mod ui_with_player;
mod ui_downloaded_only;
mod draw_startup_screen;
mod search;


use std::io;
use std::time::{Duration, Instant};
use std::process::{Command, Stdio};
use crossterm::{event::{self, Event, KeyCode}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use app::{App, save_library};
use player::Player;
use ui_with_player::ui_with_player;
use ui_downloaded_only::ui_downloaded_only;
use draw_startup_screen::draw_startup_screen;
use search::{search_youtube, download_audio};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    let mut player = Player::new();
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(400);
    let mut needs_redraw = true;

    // --- Startup screen state ---
    let mut show_startup = true;
    let mut startup_selected = 0; // 0 = stream+downloaded, 1 = downloaded only

    while show_startup {
        terminal.draw(|f| draw_startup_screen(f, startup_selected))?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up | KeyCode::Down => {
                        startup_selected = 1 - startup_selected;
                    }
                    KeyCode::Enter => {
                        show_startup = false;
                    }
                    KeyCode::Char('q') => {
                        disable_raw_mode()?;
                        execute!(io::stdout(), LeaveAlternateScreen)?;
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }

    let downloaded_only_mode = startup_selected == 1;

    // If "downloaded only" mode, set up the UI for library-only navigation
    if downloaded_only_mode {
        app.results = app.library.clone();
        app.input.clear();
        app.show_library = false; // results panel is now the library
    }

    loop {
        if needs_redraw {
            if downloaded_only_mode {
                terminal.draw(|f| ui_downloaded_only::ui_downloaded_only(f, &app, &player))?;
            } else {
                terminal.draw(|f| ui_with_player(f, &app, &player))?;
            }
            needs_redraw = false;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                needs_redraw = true;
                if downloaded_only_mode {
                    // Only allow navigation and playback in the downloaded songs list (results panel)
                    match (key.code, key.modifiers) {
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
                                    player.play(&path, title);
                                }
                            }
                        },
                        (KeyCode::Char('a'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if !app.results.is_empty() {
                                let (title, path) = &app.results[app.selected];
                                if player.child.is_some() {
                                    player.queue.push((title.clone(), path.clone()));
                                } else {
                                    player.play(&path, title);
                                }
                            }
                        },
                        (KeyCode::Char('n'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if let Some(child) = &mut player.child {
                                let _ = child.kill();
                            }
                            player.child = None;
                            player.status = "Stopped".to_string();
                            player.title = None;
                            if !player.queue.is_empty() {
                                let (title, url) = player.queue.remove(0);
                                player.play(&url, &title);
                            }
                        },
                        (KeyCode::Char('q'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            player.stop();
                            player.queue.clear();
                            break;
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
                                        player.play(&path, title);
                                    }
                                    needs_redraw = true;
                                }
                            } else if !app.results.is_empty() {
                    let (title, id) = &app.results[app.selected];
                    let url = format!("https://www.youtube.com/watch?v={}", id);
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
                    let tmp_path = std::env::temp_dir().join(format!("ytmusic_play_{}.mp3", unique));
                    let temp_path_str = tmp_path.to_str().unwrap().to_string();
                    // Add to queue immediately with Downloading... status
                    player.queue.push((format!("{} (Downloading...)", title), temp_path_str.clone()));
                    needs_redraw = true;
                    // Spawn download in background
                    let title_clone = title.clone();
                    let tmp_path_clone = tmp_path.clone();
                    std::thread::spawn(move || {
                        let output = Command::new("yt-dlp")
                            .args(["-f", "bestaudio", "-x", "--audio-format", "mp3", "-o", tmp_path_clone.to_str().unwrap(), &url])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .output();
                        // After download, update queue entry (notifies main thread on next tick)
                        // This is a simple approach; for a more robust solution, use a channel or shared state
                    });
                    // If nothing is playing, poll for file and play when ready
                    if player.child.is_none() {
                        let title = title.clone();
                        let temp_path_str = temp_path_str.clone();
                        std::thread::spawn(move || {
                            use std::{thread, time};
                            let wait_path = std::path::Path::new(&temp_path_str);
                            for _ in 0..120 { // Wait up to ~60s
                                if wait_path.exists() && wait_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                                    break;
                                }
                                thread::sleep(time::Duration::from_millis(500));
                            }
                            // The main thread will pick up the file on next tick and play it
                        });
                    }
                            }
                        }
                    },
                    (KeyCode::Char('n'), m) if m.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        // Ctrl+n: Skip to next song in queue
                        if let Some(child) = &mut player.child {
                            let _ = child.kill();
                        }
                        player.child = None;
                        player.status = "Stopped".to_string();
                        player.title = None;
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
                        needs_redraw = true;
                        break;
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
                                    player.play(&path, title);
                                }
                                needs_redraw = true;
                            }
                        } else if !app.results.is_empty() {
                            let (title, id) = &app.results[app.selected];
                            let url = format!("https://www.youtube.com/watch?v={}", id);
                            use std::time::{SystemTime, UNIX_EPOCH};
                            let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
                            let tmp_path = std::env::temp_dir().join(format!("ytmusic_play_{}.mp3", unique));
                            let temp_path_str = tmp_path.to_str().unwrap().to_string();
                            // Add to queue immediately
                            player.queue.push((title.clone(), temp_path_str.clone()));
                            // Spawn download in background
                            let url_clone = url.clone();
                            let tmp_path_clone = tmp_path.clone();
                            std::thread::spawn(move || {
                                let _ = Command::new("yt-dlp")
                                    .args(["-f", "bestaudio", "-x", "--audio-format", "mp3", "-o", tmp_path_clone.to_str().unwrap(), &url_clone])
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .status();
                            });
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
                            if !app.library.is_empty() {
                                if app.selected > 0 {
                                    app.selected -= 1;
                                    needs_redraw = true;
                                }
                            }
                        } else if !app.results.is_empty() {
                            if app.selected > 0 {
                                app.selected -= 1;
                                needs_redraw = true;
                            }
                        }
                    },
                    (KeyCode::Char(c), m) if m.is_empty() => {
                        if !app.searching && app.results.is_empty() {
                            app.input.push(c);
                            needs_redraw = true;
                        }
                    },
                    (KeyCode::Backspace, m) if m.is_empty() => {
                        if !app.searching && app.results.is_empty() {
                            app.input.pop();
                            needs_redraw = true;
                        }
                    },
                    (KeyCode::Esc, m) if m.is_empty() => {
                        if !app.results.is_empty() {
                            app.results.clear();
                            app.input.clear();
                            app.selected = 0;
                            needs_redraw = true;
                        }
                    },
                    _ => {}
                }
            }
        }
        // Only check playback status and redraw if something changed or on tick
        let playing_changed = player.is_playing();
        if playing_changed {
            needs_redraw = true;
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
            // Optionally, force redraw every N ticks for safety (not strictly needed)
        }
    }
// Save and load library to a file in the Music directory
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    // (Performance summary output removed)
    Ok(())
}

