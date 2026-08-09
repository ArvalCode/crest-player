use crate::idle_mode::{VideoRenderMode, draw_video_frame};
use crate::wallpaper::HomeWallpaper;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub const HOME_OPTION_COUNT: usize = 3;
pub const SETTINGS_OPTION_COUNT: usize = 9;
pub const RESET_WALLPAPER_SETTING: usize = SETTINGS_OPTION_COUNT - 1;

// Draws the startup screen with flamingo C ASCII art and mode selection
pub fn draw_startup_screen(
    f: &mut ratatui::Frame,
    page: (bool, usize),
    lyric_settings: (bool, bool, bool),
    video_settings: (bool, VideoRenderMode, u16, bool),
    autoplay_enabled: bool,
    home_wallpaper: Option<&HomeWallpaper>,
    playback: (Option<&str>, &str),
) {
    let (settings_page, selected) = page;
    let (lyrics_enabled, live_sync_enabled, pronunciations_enabled) = lyric_settings;
    let (idle_video_enabled, idle_video_render_mode, idle_video_fps, hardware_acceleration_enabled) =
        video_settings;
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
                if hardware_acceleration_enabled {
                    "Hardware Acceleration: AUTO"
                } else {
                    "Hardware Acceleration: OFF"
                },
                "Try hardware video decoding, with automatic software fallback.",
            ),
            (
                if autoplay_enabled {
                    "Autoplay: ON"
                } else {
                    "Autoplay: OFF"
                },
                "Prefetch a YouTube Mix recommendation whenever your queue is empty.",
            ),
            (
                if home_wallpaper.is_some() {
                    "Reset Home Wallpaper"
                } else {
                    "Home Wallpaper: DEFAULT"
                },
                if home_wallpaper.is_some() {
                    "Remove the captured video frame and restore the Crest mascot."
                } else {
                    "Press ` while viewing a music video to capture a Home wallpaper."
                },
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

    if !settings_page && let Some(wallpaper) = home_wallpaper {
        draw_wallpaper_home(f, wallpaper, option_lines, hint_text);
        return;
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

    f.render_widget(block, f.size());
    let art_paragraph = Paragraph::new(art).alignment(Alignment::Center);
    f.render_widget(art_paragraph, layout[1]);

    let options_paragraph = Paragraph::new(option_lines).alignment(Alignment::Center);
    f.render_widget(options_paragraph, layout[3]);

    let hint = Paragraph::new(hint_text).alignment(Alignment::Center);
    f.render_widget(hint, layout[4]);
}

fn draw_wallpaper_home(
    frame: &mut ratatui::Frame,
    wallpaper: &HomeWallpaper,
    option_lines: Vec<Line<'static>>,
    hint_text: String,
) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White))
        .title("Crest Player");
    let inner = outer.inner(frame.size());
    frame.render_widget(outer, frame.size());

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(option_lines.len() as u16 + 2),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    draw_video_frame(frame, layout[0], &wallpaper.frame, wallpaper.render_mode);
    for area in [layout[1], layout[2], layout[3], layout[4]] {
        frame.render_widget(Clear, area);
    }

    let menu = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White))
        .title(" Menu ")
        .style(Style::default().bg(Color::Black));
    let menu_inner = menu.inner(layout[2]);
    frame.render_widget(menu, layout[2]);
    frame.render_widget(
        Paragraph::new(option_lines)
            .alignment(Alignment::Center)
            .style(Style::default().bg(Color::Black)),
        menu_inner,
    );
    frame.render_widget(
        Paragraph::new(hint_text)
            .alignment(Alignment::Center)
            .style(Style::default().bg(Color::Black)),
        layout[4],
    );
}
