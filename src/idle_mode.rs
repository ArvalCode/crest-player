use std::time::{Duration, Instant};

use crate::video_screensaver::VideoFrame;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IdleStage {
    #[default]
    Active,
    Ambient,
    Cinema,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VideoRenderMode {
    #[default]
    AsciiFast,
    AsciiDetailed,
    ColorPixels,
}

impl VideoRenderMode {
    pub fn next(self) -> Self {
        match self {
            Self::AsciiFast => Self::AsciiDetailed,
            Self::AsciiDetailed => Self::ColorPixels,
            Self::ColorPixels => Self::AsciiFast,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AsciiFast => "Video Style: ASCII FAST",
            Self::AsciiDetailed => "Video Style: ASCII DETAILED",
            Self::ColorPixels => "Video Style: COLOR PIXELS",
        }
    }

    pub fn samples_per_cell(self) -> (u16, u16) {
        (1, 2)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorPrecision {
    Low,
    Medium,
    #[default]
    High,
}

impl ColorPrecision {
    pub fn next(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Low,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Color Precision: LOW",
            Self::Medium => "Color Precision: MEDIUM",
            Self::High => "Color Precision: HIGH",
        }
    }

    fn quantize(self, value: u8) -> u8 {
        match self {
            Self::Low => value & 0b1110_0000,
            Self::Medium => value & 0b1111_0000,
            Self::High => value,
        }
    }

    fn apply(self, color: Color) -> Color {
        match color {
            Color::Rgb(red, green, blue) => Color::Rgb(
                self.quantize(red),
                self.quantize(green),
                self.quantize(blue),
            ),
            color => color,
        }
    }
}

pub struct IdleMode {
    last_activity: Instant,
    stage: IdleStage,
}

impl IdleMode {
    pub fn new() -> Self {
        Self {
            last_activity: Instant::now(),
            stage: IdleStage::Active,
        }
    }

    pub fn note_activity(&mut self) {
        self.last_activity = Instant::now();
        self.stage = IdleStage::Active;
    }

    pub fn update(&mut self, playback_active: bool) -> bool {
        let previous = self.stage;
        self.stage = if !playback_active {
            // Do not turn an empty or paused player into a screensaver.
            self.last_activity = Instant::now();
            IdleStage::Active
        } else {
            match self.last_activity.elapsed().as_secs() {
                0..=4 => IdleStage::Active,
                5..=7 => IdleStage::Ambient,
                _ => IdleStage::Cinema,
            }
        };
        previous != self.stage
    }

    pub fn stage(&self) -> IdleStage {
        self.stage
    }

    pub fn is_visible(&self) -> bool {
        self.stage != IdleStage::Active
    }

    pub fn should_preload_video(&self, playback_active: bool) -> bool {
        playback_active
    }
}

/// Render a clock-driven true-color half-block scene. Each terminal cell contains
/// two independently colored vertical pixels (foreground on top, background below).
/// This is also the rendering contract that decoded video frames can use later.
pub struct IdleRenderState<'a> {
    pub stage: IdleStage,
    pub title: Option<&'a str>,
    pub position: Duration,
    pub video_frame: Option<&'a VideoFrame>,
    pub render_mode: VideoRenderMode,
    pub color_precision: ColorPrecision,
    pub synced_lyrics: Option<(String, Option<String>)>,
}

pub fn draw_idle_mode(frame: &mut Frame, state: IdleRenderState<'_>) {
    let IdleRenderState {
        stage,
        title,
        position,
        video_frame,
        render_mode,
        color_precision,
        synced_lyrics,
    } = state;
    let has_synced_lyrics = synced_lyrics.is_some();
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    let margin = match stage {
        IdleStage::Ambient => 1,
        IdleStage::Cinema | IdleStage::Active => 0,
    };
    let visual_area = inset(area, margin);
    let bordered = stage != IdleStage::Cinema;
    let inner = if bordered {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(match stage {
                IdleStage::Ambient => " AMBIENT · ` capture wallpaper · any other input to return ",
                _ => "",
            })
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(visual_area);
        frame.render_widget(block, visual_area);
        inner
    } else {
        visual_area
    };

    let seconds = position.as_secs_f32();
    for y in 0..inner.height {
        for x in 0..inner.width {
            let top = video_frame
                .and_then(|video| video_color(video, inner, x, y.saturating_mul(2)))
                .unwrap_or_else(|| pixel_color(x, y.saturating_mul(2), inner, seconds));
            let bottom = video_frame
                .and_then(|video| {
                    video_color(video, inner, x, y.saturating_mul(2).saturating_add(1))
                })
                .unwrap_or_else(|| {
                    pixel_color(x, y.saturating_mul(2).saturating_add(1), inner, seconds)
                });
            let top = color_precision.apply(top);
            let bottom = color_precision.apply(bottom);
            let (symbol, style) = match (render_mode, video_frame) {
                (VideoRenderMode::AsciiFast, Some(_)) => ascii_cell(top, bottom, x, y, false),
                (VideoRenderMode::AsciiDetailed, Some(_)) => ascii_cell(top, bottom, x, y, true),
                _ => ("▀", Style::default().fg(top).bg(bottom)),
            };
            if let Some(cell) = frame.buffer_mut().cell_mut((inner.x + x, inner.y + y)) {
                cell.set_symbol(symbol).set_style(style);
            }
        }
    }

    // Lyrics are overlaid instead of taking layout space, preserving the video's
    // terminal-cell resolution and aspect ratio.
    if stage != IdleStage::Active
        && inner.height >= 3
        && let Some((current, next)) = synced_lyrics
    {
        let current_line = format!("♪ {current}");
        let next_line = next.map(|line| format!("  {line}"));
        let content_width = current_line
            .chars()
            .count()
            .max(
                next_line
                    .as_deref()
                    .map(|line| line.chars().count())
                    .unwrap_or(0),
            )
            .saturating_add(4)
            .min(inner.width as usize) as u16;
        let height = if next_line.is_some() { 2 } else { 1 };
        let lyrics_area = Rect::new(
            inner.x + inner.width.saturating_sub(content_width) / 2,
            inner.bottom().saturating_sub(height + 2),
            content_width,
            height,
        );
        let mut lyric_lines = vec![Line::styled(
            current_line,
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Black)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )];
        if let Some(next_line) = next_line {
            lyric_lines.push(Line::styled(
                next_line,
                Style::default().fg(Color::Gray).bg(Color::Black),
            ));
        }
        frame.render_widget(
            Paragraph::new(lyric_lines)
                .alignment(Alignment::Center)
                .style(Style::default().bg(Color::Black)),
            lyrics_area,
        );
    }

    // Keep metadata during the first two stages; cinema intentionally leaves only
    // a small clock so the scene can occupy the terminal.
    if !has_synced_lyrics && stage != IdleStage::Cinema && inner.height >= 5 {
        let label = format!(
            "  {}  ·  {}:{:02}  ",
            title.unwrap_or("Unknown track"),
            position.as_secs() / 60,
            position.as_secs() % 60
        );
        let width = label.chars().count().min(inner.width as usize) as u16;
        let overlay = Rect::new(
            inner.x + inner.width.saturating_sub(width) / 2,
            inner.y + inner.height.saturating_sub(3),
            width,
            1,
        );
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::White).bg(Color::Black)),
            overlay,
        );
    } else if !has_synced_lyrics && inner.width >= 8 && inner.height > 0 {
        let clock = format!("{}:{:02}", position.as_secs() / 60, position.as_secs() % 60);
        let clock_area = Rect::new(
            inner.right().saturating_sub(7),
            inner.bottom().saturating_sub(1),
            6,
            1,
        );
        frame.render_widget(
            Paragraph::new(clock).style(Style::default().fg(Color::Gray).bg(Color::Black)),
            clock_area,
        );
    }
}

pub fn draw_video_frame(
    frame: &mut Frame,
    area: Rect,
    video: &VideoFrame,
    render_mode: VideoRenderMode,
    color_precision: ColorPrecision,
) {
    for y in 0..area.height {
        for x in 0..area.width {
            let top = video_color(video, area, x, y.saturating_mul(2)).unwrap_or(Color::Black);
            let bottom = video_color(video, area, x, y.saturating_mul(2).saturating_add(1))
                .unwrap_or(Color::Black);
            let top = color_precision.apply(top);
            let bottom = color_precision.apply(bottom);
            let (symbol, style) = match render_mode {
                VideoRenderMode::AsciiFast => ascii_cell(top, bottom, x, y, false),
                VideoRenderMode::AsciiDetailed => ascii_cell(top, bottom, x, y, true),
                VideoRenderMode::ColorPixels => ("▀", Style::default().fg(top).bg(bottom)),
            };
            if let Some(cell) = frame.buffer_mut().cell_mut((area.x + x, area.y + y)) {
                cell.set_symbol(symbol).set_style(style);
            }
        }
    }
}

fn blend(top: Color, bottom: Color) -> Color {
    match (top, bottom) {
        (Color::Rgb(tr, tg, tb), Color::Rgb(br, bg, bb)) => Color::Rgb(
            ((tr as u16 + br as u16) / 2) as u8,
            ((tg as u16 + bg as u16) / 2) as u8,
            ((tb as u16 + bb as u16) / 2) as u8,
        ),
        _ => top,
    }
}

fn ascii_cell(top: Color, bottom: Color, x: u16, y: u16, detailed: bool) -> (&'static str, Style) {
    const FAST_RAMP: &[u8] = b" .,:;irsXA253hMHGS#9B&@";
    const DETAILED_RAMP: &[u8] =
        br#" .`^\",:;Il!i~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"#;
    const BAYER: [[i16; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    let color = blend(top, bottom);
    let mut luminance = luminance(color) as i16;
    let ramp = if detailed {
        luminance = (luminance + (BAYER[y as usize % 4][x as usize % 4] - 8) * 2).clamp(0, 255);
        DETAILED_RAMP
    } else {
        FAST_RAMP
    };
    let index = luminance as usize * (ramp.len() - 1) / 255;
    let symbol = std::str::from_utf8(&ramp[index..index + 1]).unwrap_or(" ");
    (symbol, Style::default().fg(color).bg(Color::Black))
}

fn luminance(color: Color) -> u8 {
    let luminance = match color {
        Color::Rgb(r, g, b) => (r as usize * 2126 + g as usize * 7152 + b as usize * 722) / 10_000,
        _ => 0,
    };
    luminance as u8
}

fn video_color(frame: &VideoFrame, area: Rect, x: u16, y: u16) -> Option<Color> {
    let source_x = x as usize * frame.width as usize / area.width.max(1) as usize;
    let target_height = area.height.saturating_mul(2).max(1) as usize;
    let source_y = y as usize * frame.height as usize / target_height;
    let index = (source_y * frame.width as usize + source_x) * 3;
    Some(Color::Rgb(
        *frame.pixels.get(index)?,
        *frame.pixels.get(index + 1)?,
        *frame.pixels.get(index + 2)?,
    ))
}

fn inset(area: Rect, margin: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(margin),
        area.y.saturating_add(margin),
        area.width.saturating_sub(margin.saturating_mul(2)),
        area.height.saturating_sub(margin.saturating_mul(2)),
    )
}

fn pixel_color(x: u16, y: u16, area: Rect, time: f32) -> Color {
    let width = area.width.max(1) as f32;
    let height = area.height.max(1).saturating_mul(2) as f32;
    let nx = x as f32 / width;
    let ny = y as f32 / height;
    let wave = ((nx * 9.0 + time * 0.7).sin() + (ny * 12.0 - time * 0.45).cos()) * 0.5;
    let radial = (((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt() * 8.0 - time * 0.8).sin();
    let energy = ((wave + radial) * 0.25 + 0.5).clamp(0.0, 1.0);
    Color::Rgb(
        (18.0 + energy * 45.0) as u8,
        (20.0 + energy * 105.0) as u8,
        (45.0 + energy * 190.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::{ColorPrecision, VideoRenderMode};
    use ratatui::style::Color;

    #[test]
    fn cycles_through_all_video_render_modes() {
        let mode = VideoRenderMode::AsciiFast;
        assert_eq!(mode.next(), VideoRenderMode::AsciiDetailed);
        assert_eq!(mode.next().next(), VideoRenderMode::ColorPixels);
        assert_eq!(mode.next().next().next(), mode);
    }

    #[test]
    fn render_modes_request_half_block_resolution() {
        assert_eq!(VideoRenderMode::AsciiDetailed.samples_per_cell(), (1, 2));
        assert_eq!(VideoRenderMode::ColorPixels.samples_per_cell(), (1, 2));
    }

    #[test]
    fn color_precision_quantizes_rgb_channels() {
        let color = Color::Rgb(255, 127, 31);
        assert_eq!(ColorPrecision::High.apply(color), color);
        assert_eq!(
            ColorPrecision::Medium.apply(color),
            Color::Rgb(240, 112, 16)
        );
        assert_eq!(ColorPrecision::Low.apply(color), Color::Rgb(224, 96, 0));
    }
}
