use ratatui::{Frame, layout::{Layout, Constraint, Direction}, widgets::{Block, Borders, List, ListItem, Paragraph}, style::{Style, Color}};
use crate::{App, Player};

pub fn ui_downloaded_only(f: &mut Frame, app: &App, player: &Player) {
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

    // No search bar, just a title
    let input = Paragraph::new("")
        .block(Block::default().borders(Borders::ALL).title("Downloaded Songs (arrows to navigate, Enter/Ctrl+a to play/queue)"));
    f.render_widget(input, vchunks[0]);

    // Results panel is the downloaded songs
    use ratatui::widgets::ListState;
    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, (title, path))| {
            use ratatui::text::{Span, Line};
            use std::path::Path;
            let style = if i == app.selected {
                Style::default().bg(Color::Green).fg(Color::Black)
            } else {
                Style::default()
            };
            let (status, color) = if Path::new(path).exists() && std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) {
                ("●", Color::Green)
            } else {
                ("❌", Color::Red)
            };
            let max_len = 28;
            let short_title = if title.chars().count() > max_len {
                let mut s = title.chars().take(max_len-1).collect::<String>();
                s.push('…');
                s
            } else {
                title.clone()
            };
            ListItem::new(Line::from(vec![Span::raw(short_title + " "), Span::styled(status, Style::default().fg(color))])).style(style)
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.selected));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(
            "Downloaded Songs (arrows, Enter/Ctrl+a to play/queue, Ctrl+n next, Ctrl+p pause/resume, Ctrl+q quit)"
        ))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, vchunks[1], &mut state);

    let help = if app.results.is_empty() {
        "No downloaded songs found."
    } else {
        "Navigate with arrows, Enter/Ctrl+a to play/queue, Ctrl+n next, Ctrl+p pause/resume, Ctrl+q quit."
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

    // Right panel: queue
    let right_title = "Queue (Ctrl+a to add)";
    let right_items: Vec<ListItem> = player.queue.iter().map(|(title, path)| {
        use std::path::Path;
        let status = if Path::new(path).exists() && std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) {
            "✅"
        } else {
            "❌"
        };
        let max_len = 28;
        let short_title = if title.chars().count() > max_len {
            let mut s = title.chars().take(max_len-1).collect::<String>();
            s.push('…');
            s
        } else {
            title.clone()
        };
        ListItem::new(format!("{} {}", short_title, status))
    }).collect();
    let right_list = List::new(right_items)
        .block(Block::default().borders(Borders::ALL).title(right_title))
        .highlight_style(Style::default().bg(Color::Green).fg(Color::Black));
    f.render_widget(right_list, main_chunks[1]);
}
