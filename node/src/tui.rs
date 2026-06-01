use std::sync::Arc;

use bytesize::ByteSize;
use thousands::Separable;
use crossterm::event::{Event, EventStream, KeyCode};
use crossterm::terminal::disable_raw_mode;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;
use ratatui::DefaultTerminal;
use tokio_stream::StreamExt;

use crate::download::{BlockStatus, DownloadContext};
use crate::signal::BroadcastSignal;

pub async fn init_tui(context: Arc<DownloadContext>) {
    disable_raw_mode().expect("Failed to disable raw mode");
    let terminal = ratatui::init();
    let _ = Tui {context, progress: 0.0} .run(terminal).await;

    ratatui::restore();
}

struct Tui {
    pub context: Arc<DownloadContext>,
    pub progress: f32,
}

impl Tui {
    const FRAMES_PER_SECOND: f32 = 10.0;

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> anyhow::Result<()> {
        let period = std::time::Duration::from_secs_f32(1.0 / Self::FRAMES_PER_SECOND);
        let mut interval = tokio::time::interval(period);
        let mut events = EventStream::new();
        let mut rx = self.context.broadcast.subscribe();

        loop {
            tokio::select! {
                _ = rx.recv() => { break; },
                _ = interval.tick() => {

                    let block_count : u64 = self.context.manifest.file_size.div_ceil(self.context.manifest.block_size);
                    let block_downloaded : usize = self.context.blocks.read().await.iter().filter(|value| **value == BlockStatus::Complete).count();
                    self.progress = block_downloaded as f32 / block_count as f32;

                    terminal.draw(|frame| self.render(frame))?;
                },
                Some(Ok(event)) = events.next() => self.handle_event(&event),
            }
        }

        Ok(())
    }

    fn render_tui_log(&self, frame: &mut Frame, area: Rect) {
        let log_widget = tui_logger::TuiLoggerWidget::default()
            .block(Block::new().borders(Borders::ALL).title("System Log").title_style(Style::default().bold()))
            .style_error(Style::default().fg(Color::LightRed))
            .style_warn(Style::default().fg(Color::LightYellow))
            .style_info(Style::default().fg(Color::LightGreen))
            .style_debug(Style::default().fg(Color::LightCyan))
            .style_trace(Style::default().fg(Color::LightBlue))
            .output_file(false)
            .output_line(false);
        frame.render_widget(log_widget, area);
    }

    fn render_progress(&self, frame: &mut Frame, inner_area: Rect) {
        let prog_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner_area);
        let prog_up = prog_layout[0];
        let prog_down = prog_layout[1];

        let progress_gauge = Gauge::default()
            .gauge_style(Style::new().light_yellow().on_dark_gray())
            .percent(((self.progress * 100.0).round() as u16).clamp(0, 100))
            .label(format!("{:.1}%", self.progress * 100.0));
        frame.render_widget(progress_gauge, prog_down);
    }

    fn render_info(&self, frame: &mut Frame, inner_area: Rect){
        let info_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(10),
                Constraint::Length(1),
            ])
            .split(inner_area);

        let lines = vec![
            "File Info:".bold().into(),
            self.context.manifest.file_name.clone().bold().green().into(),
            Line::from_iter([
                "  Hash: ".yellow(),
                self.context.manifest.file_hash.clone().into()
            ]),
            Line::from_iter([
                "  Size: ".yellow(),
                ByteSize::b(self.context.manifest.file_size).display().iec().to_string().into(),
                " (".into(),
                self.context.manifest.file_size.separate_with_commas().to_string().into(),
                " bytes)".into()
            ]),
            Line::from_iter([
                "  Block Size: ".yellow(),
                ByteSize::b(self.context.manifest.block_size).display().iec().to_string().into(),
                " (".into(),
                self.context.manifest.file_size.div_ceil(self.context.manifest.block_size).separate_with_commas().to_string().into(),
                " blocks)".into()
            ])
        ];

        let paragraph = Paragraph::new(lines)
            .style(Color::White);

        frame.render_widget(paragraph, info_layout[0]);
    }

    fn render(&self, frame: &mut Frame) {
        let vlayout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Fill(1),
                Constraint::Length(4),
            ])
            .split(frame.area());
        let hlayout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(vlayout[0]);

        let prog_area = vlayout[1];
        let log_area = hlayout[0];
        let info_area = hlayout[1];

        let progress_block = Block::new().borders(Borders::ALL).title("Download Progress").title_style(Style::default().bold());
        let info_block = Block::new().borders(Borders::ALL).title("Task Info");
        let prog_inner_area = progress_block.inner(prog_area);
        let info_inner_area = info_block.inner(info_area);

        // frame.render_widget(log_widget, log_area);
        self.render_tui_log(frame, log_area);
        frame.render_widget(progress_block, prog_area);
        self.render_progress(frame, prog_inner_area);

        frame.render_widget(info_block, info_area);
        self.render_info(frame, info_inner_area);
    }

    fn handle_event(&mut self, event: &Event) {
        if let Some(key) = event.as_key_press_event() {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => { let _ = self.context.broadcast.send(BroadcastSignal::Shutdown); },
                _ => {}
            }
        }
    }
}