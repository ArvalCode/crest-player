use crate::app::App;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

pub fn render_download_queue(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .downloads
        .iter()
        .rev()
        .map(|download| {
            let (label, color) = ("Downloading…", Color::Yellow);
            Line::from(vec![
                Span::raw(shorten(&download.title, 22)),
                Span::raw("  "),
                Span::styled(label, Style::default().fg(color)),
            ])
            .into()
        })
        .collect();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Download Queue"),
        ),
        area,
    );
}

fn shorten(title: &str, max_len: usize) -> String {
    if title.chars().count() <= max_len {
        return title.to_string();
    }
    let mut shortened = title
        .chars()
        .take(max_len.saturating_sub(1))
        .collect::<String>();
    shortened.push('…');
    shortened
}
