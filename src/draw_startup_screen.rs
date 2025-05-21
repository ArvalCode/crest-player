use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::layout::{Alignment, Layout, Constraint, Direction};
use ratatui::text::{Span, Line};
use ratatui::style::{Style, Color};

// Draws the startup screen with flamingo C ASCII art and mode selection
pub fn draw_startup_screen(f: &mut ratatui::Frame, selected: usize) {
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

    let options = vec![
        ("Stream + Downloaded Music", "Browse and stream from YouTube, plus play your downloaded music."),
        ("Downloaded Music Only", "Play only your downloaded music library."),
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

    let hint = Paragraph::new("↑/↓ to select, Enter to start, Ctrl+Q to quit").alignment(Alignment::Center);
    f.render_widget(hint, layout[4]);
}
