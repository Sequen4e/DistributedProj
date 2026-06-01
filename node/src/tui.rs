use std::io;
use crossterm::event::{Event, EventStream, KeyCode};
// 🚀 核心修复 1：引入 ExecutableCommand 和 备用屏幕切换指令
use crossterm::ExecutableCommand;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::{Frame, Terminal};
use tokio_stream::StreamExt;

use crate::signal::BroadcastSignal;

pub async fn init_tui(broadcast_tx: tokio::sync::broadcast::Sender<BroadcastSignal>) {
    let _ = enable_raw_mode();
    let mut stdout = io::stdout();
    
    // 🚀 核心修复 1：强制终端进入“全屏画布”模式，而不是普通的流式打印！
    let _ = stdout.execute(EnterAlternateScreen); 
    
    let backend = CrosstermBackend::new(stdout);
    
    if let Ok(mut terminal) = Terminal::new(backend) {
        let _ = terminal.clear();
        let mut tui = Tui {
            broadcast_tx,
            progress: 0.0,
            file_name: String::from("Waiting for task..."),
            file_size: 0,
            file_hash: String::from("-"),
        };
        let _ = tui.run(&mut terminal).await;
    }

    // 退出时恢复终端原来的样子
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = stdout.execute(LeaveAlternateScreen);
}

struct Tui {
    pub broadcast_tx: tokio::sync::broadcast::Sender<BroadcastSignal>,
    pub progress: f32,
    pub file_name: String,
    pub file_size: u64,
    pub file_hash: String,
}

impl Tui {
    pub async fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {
        let mut events = EventStream::new();
        let mut rx = self.broadcast_tx.subscribe();

        // 🚀 核心修复 2：恢复 10Hz 的心跳（每 100 毫秒醒来一次）
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

        loop {
            tokio::select! {
                // 🚀 心跳任务：即使没有任何业务网络事件，每 100 毫秒也会强制把 tui-logger 内存里的最新日志画到屏幕上
                _ = interval.tick() => {
                    terminal.draw(|frame| self.render(frame))?;
                }
                // 业务数据更新
                Ok(signal) = rx.recv() => {
                    match signal {
                        BroadcastSignal::Shutdown => break,
                        BroadcastSignal::TaskInfo { file_name, file_size, file_hash } => {
                            self.file_name = file_name;
                            self.file_size = file_size;
                            self.file_hash = file_hash;
                        }
                        BroadcastSignal::ProgressUpdate { downloaded_blocks, total_blocks } => {
                            if total_blocks > 0 {
                                self.progress = downloaded_blocks as f32 / total_blocks as f32;
                            }
                        }
                    }
                    terminal.draw(|frame| self.render(frame))?;
                },
                // 用户按键捕获
                Some(Ok(event)) = events.next() => {
                    if let Event::Key(key) = &event {
                        if key.kind == crossterm::event::KeyEventKind::Press {
                            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                                let _ = self.broadcast_tx.send(BroadcastSignal::Shutdown);
                            }
                        }
                    }
                }
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
            .output_file(false)
            .output_line(false);
        frame.render_widget(log_widget, area);
    }

    fn render(&self, frame: &mut Frame) {
        let frame_size = frame.size();

        let vlayout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Fill(1), Constraint::Length(4)])
            .split(frame_size);
        let hlayout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(vlayout[0]);

        self.render_tui_log(frame, hlayout[0]);

        let progress_block = Block::new().borders(Borders::ALL).title("Transmission Progress").title_style(Style::default().bold());
        let progress_gauge = Gauge::default()
            .gauge_style(Style::new().light_yellow().on_dark_gray())
            .percent(((self.progress * 100.0).round() as u16).clamp(0, 100))
            .label(format!("{:.1}%", self.progress * 100.0));
        let inner_prog = progress_block.inner(vlayout[1]);
        frame.render_widget(progress_block, vlayout[1]);
        
        let prog_layout = Layout::default().direction(Direction::Vertical).constraints(vec![Constraint::Length(1), Constraint::Length(1)]).split(inner_prog);
        frame.render_widget(progress_gauge, prog_layout[1]);

        let info_block = Block::new().borders(Borders::ALL).title("Node Task Info");
        let lines = vec![
            "File Name:".bold().into(),
            self.file_name.clone().green().into(),
            Line::from_iter(["Hash: ".yellow(), self.file_hash.clone().into()]),
            Line::from_iter(["Size: ".yellow(), format!("{} bytes", self.file_size).into()]),
        ];
        let paragraph = Paragraph::new(lines).style(Color::White);
        let inner_info = info_block.inner(hlayout[1]);
        frame.render_widget(info_block, hlayout[1]);
        frame.render_widget(paragraph, inner_info);
    }
}