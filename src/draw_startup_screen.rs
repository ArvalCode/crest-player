use crate::idle_mode::{ColorPrecision, VideoRenderMode, draw_video_frame};
use crate::wallpaper::HomeWallpaper;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub const HOME_OPTION_COUNT: usize = 4;
pub const SETTINGS_OPTION_COUNT: usize = 14;
pub const DELETE_MEDIA_SETTING: usize = SETTINGS_OPTION_COUNT - 3;
pub const RESET_WALLPAPER_SETTING: usize = SETTINGS_OPTION_COUNT - 2;
pub const REMOVE_APPLICATION_SETTING: usize = SETTINGS_OPTION_COUNT - 1;

// Draws the startup screen with flamingo C ASCII art and mode selection
pub struct StartupScreenState<'a> {
    pub page: (bool, usize),
    pub lyric_settings: (bool, bool, bool),
    pub video_settings: (bool, VideoRenderMode, ColorPrecision, u16, bool),
    pub autoplay_enabled: bool,
    pub discord_presence_enabled: bool,
    pub discord_presence_configured: bool,
    pub library_track_count: usize,
    pub home_wallpaper: Option<&'a HomeWallpaper>,
    pub playback: (Option<&'a str>, &'a str),
    pub party_notice: Option<&'a str>,
}

pub fn draw_startup_screen(f: &mut ratatui::Frame, state: StartupScreenState<'_>) {
    let StartupScreenState {
        page,
        lyric_settings,
        video_settings,
        autoplay_enabled,
        discord_presence_enabled,
        discord_presence_configured,
        library_track_count,
        home_wallpaper,
        playback,
        party_notice,
    } = state;
    let (settings_page, selected) = page;
    let (lyrics_enabled, live_sync_enabled, pronunciations_enabled) = lyric_settings;
    let (
        idle_video_enabled,
        idle_video_render_mode,
        color_precision,
        idle_video_fps,
        hardware_acceleration_enabled,
    ) = video_settings;
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
                color_precision.label(),
                "Cycle low, medium, and high RGB precision. Lower precision reduces terminal output.",
            ),
            (
                match idle_video_fps {
                    0 => "Video FPS: AUTO",
                    24 => "Video FPS: 24",
                    30 => "Video FPS: 30",
                    60 => "Video FPS: 60",
                    _ => "Video FPS: 15",
                },
                "Cycle between adaptive AUTO mode and fixed 15, 30, or 60 FPS.",
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
                if !discord_presence_configured {
                    "Discord Rich Presence: NOT CONFIGURED"
                } else if discord_presence_enabled {
                    "Discord Rich Presence: ON"
                } else {
                    "Discord Rich Presence: OFF"
                },
                if discord_presence_configured {
                    "Share the current track and playback state with the Discord desktop app."
                } else {
                    "Set CREST_DISCORD_CLIENT_ID to a Discord Application ID before launching Crest Player."
                },
            ),
            (
                "Speakers...",
                "Find AirPlay, Sonos, and Bluetooth speakers, ordered by connection type.",
            ),
            (
                "Delete All Known Songs/Videos",
                if library_track_count == 0 {
                    "No downloaded library media is currently tracked by Crest Player."
                } else {
                    "Delete every song and cached video recorded in Crest Player's library index."
                },
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
            (
                "Remove Crest Player...",
                "Choose application only, downloaded media only, or everything, then confirm removal.",
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
                "Party Mode",
                "Instantly host a private music queue for phones on this Wi-Fi network.",
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
        "↑/↓ select, Enter change, Esc/Ctrl+← back"
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
    let hint_text = party_notice
        .map(|notice| format!("{notice}  ·  {hint_text}"))
        .unwrap_or(hint_text);

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
    let layout = if settings_page {
        Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(0),
                Constraint::Length(0),
                Constraint::Min(option_lines.len() as u16),
                Constraint::Length(if party_notice.is_some() { 2 } else { 1 }),
            ])
            .split(f.area())
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .margin(4)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(art.len() as u16),
                Constraint::Length(2),
                Constraint::Length(option_lines.len() as u16),
                Constraint::Min(1),
            ])
            .split(f.area())
    };

    f.render_widget(block, f.area());
    let art_paragraph = Paragraph::new(art).alignment(Alignment::Center);
    f.render_widget(art_paragraph, layout[1]);

    let available_lines = usize::from(layout[3].height);
    let selected_line = selected + 1;
    let scroll = if settings_page && selected_line >= available_lines {
        (selected_line + 1 - available_lines) as u16
    } else {
        0
    };
    let options_paragraph = Paragraph::new(option_lines)
        .alignment(Alignment::Center)
        .scroll((scroll, 0));
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
    let inner = outer.inner(frame.area());
    frame.render_widget(outer, frame.area());

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
    draw_video_frame(
        frame,
        layout[0],
        &wallpaper.frame,
        wallpaper.render_mode,
        wallpaper.color_precision,
    );
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
