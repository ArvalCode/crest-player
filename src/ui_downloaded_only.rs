use crate::{App, Player};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn ui_downloaded_only(f: &mut Frame, app: &App, player: &Player) {
    let size = f.size();
    let column_widths = if app.lyrics_enabled {
        vec![
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
        ]
    } else {
        vec![Constraint::Percentage(70), Constraint::Percentage(30)]
    };
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(2)
        .constraints(column_widths)
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
    let input = Paragraph::new("").block(
        Block::default()
            .borders(Borders::ALL)
            .title("Downloaded Songs (arrows navigate, Enter/Ctrl+a play/queue, Home returns)"),
    );
    f.render_widget(input, vchunks[0]);

    // Results panel is the downloaded songs
    use ratatui::widgets::ListState;
    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, (title, path))| {
            use ratatui::text::{Line, Span};
            let style = if i == app.selected {
                Style::default().bg(Color::Green).fg(Color::Black)
            } else {
                Style::default()
            };
            let (status, color) = if app.is_library_file_available(path) {
                ("●", Color::Green)
            } else {
                ("❌", Color::Red)
            };
            let max_len = 28;
            let short_title = if title.chars().count() > max_len {
                let mut s = title.chars().take(max_len - 1).collect::<String>();
                s.push('…');
                s
            } else {
                title.clone()
            };
            ListItem::new(Line::from(vec![
                Span::raw(short_title + " "),
                Span::styled(status, Style::default().fg(color)),
            ]))
            .style(style)
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.selected));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(
            "Downloaded Songs (arrows, Enter/Ctrl+a play/queue, Ctrl+n next, Home return, Ctrl+q quit)"
        ))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, vchunks[1], &mut state);

    let help = if app.results.is_empty() {
        "No downloaded songs found."
    } else {
        "Arrows navigate, Enter/Ctrl+a plays or queues, Ctrl+n skips, Home returns to the menu, Ctrl+q quits."
    };
    let help = Paragraph::new(help).block(Block::default().borders(Borders::ALL));
    f.render_widget(help, vchunks[2]);

    // Player bar
    let player_text = if let Some(title) = &player.title {
        format!(
            "▶ {} [{}] (Alt+± seek 5s, Ctrl+p pause, Ctrl+n next, Home returns)",
            title, player.status
        )
    } else {
        format!(
            "▶ [No song playing] [{}] (Alt+± seek 5s, Ctrl+p pause, Ctrl+n next, Home returns)",
            player.status
        )
    };
    let player_bar =
        Paragraph::new(player_text).block(Block::default().borders(Borders::ALL).title("Player"));
    f.render_widget(player_bar, vchunks[3]);

    // Right panel: queue
    let right_title = "Queue (Ctrl+a to add)";
    let right_items: Vec<ListItem> = player
        .queue
        .iter()
        .map(|(title, path)| {
            let status = if app.is_library_path(path) || !title.ends_with("(Downloading...)") {
                "✅"
            } else {
                "❌"
            };
            let max_len = 28;
            let short_title = if title.chars().count() > max_len {
                let mut s = title.chars().take(max_len - 1).collect::<String>();
                s.push('…');
                s
            } else {
                title.clone()
            };
            ListItem::new(format!("{} {}", short_title, status))
        })
        .collect();
    let right_list = List::new(right_items)
        .block(Block::default().borders(Borders::ALL).title(right_title))
        .highlight_style(Style::default().bg(Color::Green).fg(Color::Black));
    f.render_widget(right_list, main_chunks[1]);

    if app.lyrics_enabled {
        let lyric_lines: Vec<ratatui::text::Line> = if app.lyrics.is_empty() {
            vec![ratatui::text::Line::from(app.lyrics_message.as_str())]
        } else {
            app.lyrics
                .iter()
                .enumerate()
                .flat_map(|(index, line)| {
                    let active = app.lyrics_active == Some(index);
                    let lyric_style = if active {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(ratatui::style::Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let mut rows = vec![ratatui::text::Line::styled(
                        if active {
                            format!("▶ {}", line.text)
                        } else {
                            format!("  {}", line.text)
                        },
                        lyric_style,
                    )];
                    if app.pronunciations_enabled
                        && let Some(romaji) = &line.romaji
                    {
                        rows.push(ratatui::text::Line::styled(
                            format!("  {}", romaji),
                            Style::default().fg(if active {
                                Color::LightYellow
                            } else {
                                Color::DarkGray
                            }),
                        ));
                    }
                    rows
                })
                .collect()
        };
        let sync_label = if app.lyrics_synced && app.live_sync_enabled {
            "SYNCED"
        } else {
            "STATIC"
        };
        let lyrics = Paragraph::new(lyric_lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((app.lyrics_scroll, 0))
            .block(Block::default().borders(Borders::ALL).title(format!(
                "Lyrics{} · {} · PgUp/PgDn",
                if app.pronunciations_enabled {
                    " + Pronunciation"
                } else {
                    ""
                },
                sync_label
            )));
        f.render_widget(lyrics, main_chunks[2]);
    }
}
