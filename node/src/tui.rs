use std::sync::Arc;

use crossterm::event::{Event, EventStream, KeyCode};
use crossterm::terminal::disable_raw_mode;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Gauge};
use ratatui::layout::{Direction, Layout, Constraint};
use ratatui::Frame;
use ratatui::DefaultTerminal;
use tokio_stream::StreamExt;

use crate::download::DownloadContext;
use crate::signal::BroadcastSignal;

pub async fn init_tui(context: Arc<DownloadContext>) {
    disable_raw_mode().expect("Failed to disable raw mode");
    let terminal = ratatui::init();
    Tui {context} .run(terminal).await;

    ratatui::restore();
}

struct Tui {
    context: Arc<DownloadContext>
}

impl Tui {
    const FRAMES_PER_SECOND: f32 = 60.0;

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> anyhow::Result<()> {
        let period = std::time::Duration::from_secs_f32(1.0 / Self::FRAMES_PER_SECOND);
        let mut interval = tokio::time::interval(period);
        let mut events = EventStream::new();
        let mut rx = self.context.broadcast.subscribe();

        loop {
            tokio::select! {
                _ = rx.recv() => { break; },
                _ = interval.tick() => { terminal.draw(|frame| self.render(frame))?; },
                Some(Ok(event)) = events.next() => self.handle_event(&event),
            }
        }

        Ok(())
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

        let prog_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(prog_inner_area);
        let prog_up = prog_layout[0];
        let prog_down = prog_layout[1];
        
        let progress_gauge = Gauge::default()
            .gauge_style(Style::new().light_yellow().on_dark_gray())
            .percent(80);

        let log_widget = tui_logger::TuiLoggerWidget::default()
            .block(Block::new().borders(Borders::ALL).title("System Log").title_style(Style::default().bold()))
            .style_error(Style::default().fg(Color::LightRed))
            .style_warn(Style::default().fg(Color::LightYellow))
            .style_info(Style::default().fg(Color::LightGreen))
            .style_debug(Style::default().fg(Color::LightCyan))
            .style_trace(Style::default().fg(Color::LightBlue))
            .output_file(false)
            .output_line(false);
        

        frame.render_widget(log_widget, log_area);
        frame.render_widget(info_block, info_area);
        frame.render_widget(progress_block, prog_area);
        frame.render_widget(progress_gauge, prog_down);
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