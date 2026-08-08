use crate::idle_mode::VideoRenderMode;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

// Draws the startup screen with flamingo C ASCII art and mode selection
pub fn draw_startup_screen(
    f: &mut ratatui::Frame,
    page: (bool, usize),
    lyric_settings: (bool, bool, bool),
    video_settings: (bool, VideoRenderMode, u16),
    autoplay_enabled: bool,
    playback: (Option<&str>, &str),
) {
    let (settings_page, selected) = page;
    let (lyrics_enabled, live_sync_enabled, pronunciations_enabled) = lyric_settings;
    let (idle_video_enabled, idle_video_render_mode, idle_video_fps) = video_settings;
    // Flamingo C ASCII art (red)
    let flamingo = vec![
        r"                                            *******,           /#,",
        r"                                            ************,,//######,",
        r"                                            ************,,//######.",
        r"                                            ********           ,#/",
        r"                                             *******                            ",
        r"                                              *******                           ",
        r"                                               *******.                         ",
        r"                                                ******                          ",
        r"                              .************.       ******,                      ",
        r"                         ********************      *******                      ",
        r"                      ************************      .******                     ",
        r"                   ****************************     *******                    ",
        r"                 ////////***********************************                    ",
        r"               *//////******//*****************************,                    ",
        r"              *  (//////////////*******************,                            ",
        r"               //*//////////////*******************                              ",
        r"                   (///,.      //////*      ..,,.                               ",
        r"                           ,//(,      ***                                        ",
        r"                           ,///.      ***                                        ",
        r"                      .(////(/,//***                                             ",
        r"                                ,(/(//////***                                    ",
        r"                                    *** /////,                                  ",
        r"                                    *** /////,                                  ",
        r"                                    ***                                          ",
        r"                                    ***                                          ",
        r"                                    ***                                          ",
        r"                                    ***                                          ",
        r"                                    *********.                                   ",
        r"                                     ******,                                     ",
        r"                                                                                 ",
    ];

    let art: Vec<Line> = flamingo
        .iter()
        .map(|&l| Line::from(Span::styled(l, Style::default().fg(Color::Red))))
        .collect();

    let options = if settings_page {
        vec![
            (
                if lyrics_enabled {
                    "Lyrics: ON"
                } else {
                    "Lyrics: OFF"
                },
                "Show or completely remove the Lyrics + Romaji panel.",
            ),
            (
                if live_sync_enabled {
                    "Live Lyrics Sync: ON"
                } else {
                    "Live Lyrics Sync: OFF"
                },
                if lyrics_enabled {
                    "Automatically highlight and scroll timestamped lyrics during playback."
                } else {
                    "Enable Lyrics first to use live synchronization."
                },
            ),
            (
                if pronunciations_enabled {
                    "English Pronunciations: ON"
                } else {
                    "English Pronunciations: OFF"
                },
                "Show available Latin-letter pronunciations alongside original lyrics (currently Japanese romaji).",
            ),
            (
                if idle_video_enabled {
                    "YouTube Screensaver: ON"
                } else {
                    "YouTube Screensaver: OFF"
                },
                "Show the track's YouTube video after 5 seconds without input.",
            ),
            (
                idle_video_render_mode.label(),
                "Cycle fast ASCII, detailed dithered ASCII, and color pixels.",
            ),
            (
                match idle_video_fps {
                    30 => "Video FPS: 30",
                    60 => "Video FPS: 60",
                    _ => "Video FPS: 15",
                },
                "Cycle the music-video rendering frame rate between 15, 30, and 60 FPS.",
            ),
            (
                if autoplay_enabled {
                    "Autoplay: ON"
                } else {
                    "Autoplay: OFF"
                },
                "Prefetch a YouTube Mix recommendation whenever your queue is empty.",
            ),
        ]
    } else {
        vec![
            (
                "Stream + Downloaded Music",
                "Browse and stream from YouTube, plus play your downloaded music.",
            ),
            (
                "Downloaded Music Only",
                "Play only your downloaded music library.",
            ),
            (
                "Settings",
                "Configure lyrics, pronunciations, video, FPS, and autoplay.",
            ),
        ]
    };

    let mut option_lines = vec![];
    for (i, (title, desc)) in options.iter().enumerate() {
        let style = if i == selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        option_lines.push(Line::from(vec![Span::styled(
            format!(" {} ", title),
            style,
        )]));
        if i == selected {
            option_lines.push(Line::from(vec![Span::styled(
                format!("   {}", desc),
                Style::default().fg(Color::Gray),
            )]));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(if settings_page {
            "Crest Player · Settings"
        } else {
            "Crest Player"
        });
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(4)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(art.len() as u16),
            Constraint::Length(2),
            Constraint::Length(option_lines.len() as u16),
            Constraint::Min(1),
        ])
        .split(f.size());

    let art_paragraph = Paragraph::new(art).alignment(Alignment::Center);
    f.render_widget(block, f.size());
    f.render_widget(art_paragraph, layout[1]);

    let options_paragraph = Paragraph::new(option_lines).alignment(Alignment::Center);
    f.render_widget(options_paragraph, layout[3]);

    let navigation_hint = if settings_page {
        "↑/↓ select, Enter change, Esc/Home back"
    } else {
        "↑/↓ select, Enter open, Q quit"
    };
    let hint_text = match playback.0 {
        Some(title) => format!(
            "Now playing: {title} [{}]  ·  {navigation_hint}",
            playback.1
        ),
        None => navigation_hint.to_string(),
    };
    let hint = Paragraph::new(hint_text).alignment(Alignment::Center);
    f.render_widget(hint, layout[4]);
}
