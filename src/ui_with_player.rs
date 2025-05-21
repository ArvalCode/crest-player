use ratatui::{Frame, layout::{Layout, Constraint, Direction}, widgets::{Block, Borders, List, ListItem, Paragraph}, style::{Style, Color}};
use crate::{App, Player};

pub fn ui_with_player(f: &mut Frame, app: &App, player: &Player) {
    let size = f.size();
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(2)
        .constraints([
            Constraint::Min(60),
            Constraint::Length(40),
        ])
        .split(size);

    let vchunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(main_chunks[0]);

    // Highlight the search query text (not the whole input box) yellow if searching, green if not
    use ratatui::text::{Span, Line};
    let input_line = if !app.input.is_empty() {
        let color = if app.searching {
            Color::Yellow
        } else if !app.results.is_empty() || app.error.is_some() {
            Color::Green
        } else {
            Color::White
        };
        Line::from(vec![Span::styled(app.input.as_str(), Style::default().fg(color))])
    } else {
        Line::from("")
    };
    let input = Paragraph::new(input_line)
        .block(Block::default().borders(Borders::ALL).title("Search YouTube Music (type and press Enter)"));
    f.render_widget(input, vchunks[0]);

    let query = app.input.trim();
    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, (title, _))| {
            if !query.is_empty() && title.to_lowercase().contains(&query.to_lowercase()) {
                let style = if i == app.selected {
                    Style::default().bg(Color::Green).fg(Color::Black)
                } else {
                    Style::default().fg(Color::Yellow)
                };
                ListItem::new(title.clone()).style(style)
            } else if i == app.selected {
                ListItem::new(title.clone()).style(Style::default().bg(Color::Blue).fg(Color::White))
            } else {
                ListItem::new(title.clone())
            }
        })
        .collect();
    use ratatui::widgets::ListState;
    let mut state = ListState::default();
    state.select(Some(app.selected));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(
            "Results (arrows, Ctrl+a to queue, Ctrl+l to like, v to view library, Esc to search again, Ctrl+q to quit, Ctrl+p to pause/resume, Ctrl+n to next)"
        ))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, vchunks[1], &mut state);

    let help = if app.results.is_empty() {
        "Type your search and press Enter. Ctrl+q to quit."
    } else {
        "Navigate with arrows, Enter to play, Ctrl+a to queue, Ctrl+l to like, v to view library, Esc to search again, Ctrl+q to quit. Ctrl+p to pause/resume. Ctrl+n to next."
    };
    let help = if let Some(err) = &app.error {
        err.as_str()
    } else {
        help
    };
    let help = Paragraph::new(help).block(Block::default().borders(Borders::ALL));
    f.render_widget(help, vchunks[2]);

    // Player bar
    let player_text = if let Some(title) = &player.title {
        format!("▶ {} [{}] (Ctrl+p to pause/resume, Ctrl+n to next, Ctrl+q to quit)", title, player.status)
    } else {
        format!("▶ [No song playing] [{}] (Ctrl+p to pause/resume, Ctrl+n to next, Ctrl+q to quit)", player.status)
    };
    let player_bar = Paragraph::new(player_text).block(Block::default().borders(Borders::ALL).title("Player"));
    f.render_widget(player_bar, vchunks[3]);

    // Right panel: queue or library
    let right_title = if app.show_library { "Library (v to close)" } else { "Queue (Ctrl+a to add)" };
    let right_items: Vec<ListItem> = if app.show_library {
        app.library.iter().map(|(title, path)| {
            use ratatui::text::{Span, Line};
            use std::path::Path;
            let max_len = 28;
            let short_title = if title.chars().count() > max_len {
                let mut s = title.chars().take(max_len-1).collect::<String>();
                s.push('…');
                s
            } else {
                title.clone()
            };
            let (status, color) = if Path::new(path).exists() && std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) {
                ("●", Color::Green)
            } else {
                ("❌", Color::Red)
            };
            ListItem::new(Line::from(vec![Span::raw(short_title + " "), Span::styled(status, Style::default().fg(color))]))
        }).collect()
    } else {
        player.queue.iter().map(|(title, path)| {
            use std::path::Path;
            use ratatui::text::{Span, Line};
            // Check if this song is in the library (by path)
            let is_library = app.library.iter().any(|(_, lib_path)| lib_path == path);
            let (status, style): (&str, Style) = if is_library {
                ("●", Style::default().fg(Color::Green))
            } else if Path::new(path).exists() && std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) {
                if path.ends_with(".mp3") && (path.contains("ytmusic_play_") || path.contains("ytmusic_play-")) {
                    ("☑", Style::default().fg(Color::Yellow))
                } else {
                    ("■", Style::default().fg(Color::Green))
                }
            } else {
                ("❌", Style::default().fg(Color::Red))
            };
            let max_len = 28;
            let short_title = if title.chars().count() > max_len {
                let mut s = title.chars().take(max_len-1).collect::<String>();
                s.push('…');
                s
            } else {
                title.clone()
            };
            ListItem::new(Line::from(vec![Span::raw(short_title + " "), Span::styled(status, style)]))
        }).collect()
    };
    let right_list = List::new(right_items)
        .block(Block::default().borders(Borders::ALL).title(right_title))
        .highlight_style(Style::default().bg(Color::Green).fg(Color::Black));
    f.render_widget(right_list, main_chunks[1]);
}
