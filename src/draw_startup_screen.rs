use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::layout::{Alignment, Layout, Constraint, Direction};
use ratatui::text::{Span, Line};
use ratatui::style::{Style, Color};
use crate::idle_mode::VideoRenderMode;

// Draws the startup screen with flamingo C ASCII art and mode selection
pub fn draw_startup_screen(
    f: &mut ratatui::Frame,
    selected: usize,
    lyrics_enabled: bool,
    live_sync_enabled: bool,
    video_settings: (bool, VideoRenderMode, u16),
    autoplay_enabled: bool,
) {
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


    let art: Vec<Line> = flamingo.iter().map(|&l| Line::from(Span::styled(l, Style::default().fg(Color::Red)))).collect();

    let options = [
        ("Stream + Downloaded Music", "Browse and stream from YouTube, plus play your downloaded music."),
        ("Downloaded Music Only", "Play only your downloaded music library."),
        (
            if lyrics_enabled { "Lyrics: ON" } else { "Lyrics: OFF" },
            "Show or completely remove the Lyrics + Romaji panel.",
        ),
        (
            if live_sync_enabled { "Live Lyrics Sync: ON" } else { "Live Lyrics Sync: OFF" },
            if lyrics_enabled {
                "Automatically highlight and scroll timestamped lyrics during playback."
            } else {
                "Enable Lyrics first to use live synchronization."
            },
        ),
        (
            if idle_video_enabled { "YouTube Screensaver: ON" } else { "YouTube Screensaver: OFF" },
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
    ];

    let mut option_lines = vec![];
    for (i, (title, desc)) in options.iter().enumerate() {
        let style = if i == selected {
            Style::default().fg(Color::Black).bg(Color::Red).add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        option_lines.push(Line::from(vec![Span::styled(format!(" {} ", title), style)]));
        if i == selected {
            option_lines.push(Line::from(vec![Span::styled(format!("   {}", desc), Style::default().fg(Color::Gray))]));
        }
    }

    let block = Block::default().borders(Borders::ALL).title("Crest Player");
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

    let hint = Paragraph::new("↑/↓ to select, Enter to start/toggle, Q to quit").alignment(Alignment::Center);
    f.render_widget(hint, layout[4]);
}
