use std::io;
use crossterm::event::{Event, EventStream, KeyCode};
use crossterm::ExecutableCommand;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::{Frame, Terminal};
use tokio_stream::StreamExt;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::signal::BroadcastSignal;
// 🚀 引入原版的本地文件扫描函数和配置项
use crate::local_files::scan_share_dir;
use crate::node_shell::NodeConfig;
use crate::node_runtime::RuntimeCommand;

pub async fn init_tui(
    config: NodeConfig,
    broadcast_tx: broadcast::Sender<BroadcastSignal>,
    command_tx: mpsc::UnboundedSender<RuntimeCommand>,
) {
    let _ = enable_raw_mode();
    let mut stdout = io::stdout();
    let _ = stdout.execute(EnterAlternateScreen); 
    
    let backend = CrosstermBackend::new(stdout);
    
    if let Ok(mut terminal) = Terminal::new(backend) {
        let _ = terminal.clear();
        
        // 🚀 初始化 CLI 终端欢迎语
        let mut initial_history = vec![
            "Resource Node Integrated CLI Terminal v0.1.0".to_string(),
            "Type `help` to list available commands.".to_string(),
            "--------------------------------------------------".to_string(),
        ];

        let mut tui = Tui {
            config,
            broadcast_tx,
            command_tx,
            progress: 0.0,
            file_name: String::from("Waiting for task..."),
            file_size: 0,
            file_hash: String::from("-"),
            input_buffer: String::new(),
            cli_history: initial_history, // 🚀 挂载终端历史纪录
        };
        let _ = tui.run(&mut terminal).await;
    }

    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = stdout.execute(LeaveAlternateScreen);
}

struct Tui {
    pub config: NodeConfig,
    pub broadcast_tx: broadcast::Sender<BroadcastSignal>,
    pub command_tx: mpsc::UnboundedSender<RuntimeCommand>,
    pub progress: f32,
    pub file_name: String,
    pub file_size: u64,
    pub file_hash: String,
    pub input_buffer: String,
    pub cli_history: Vec<String>, // 🚀 存放 CLI 回显历史的缓冲区
}

impl Tui {
    pub async fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {
        let mut events = EventStream::new();
        let mut rx = self.broadcast_tx.subscribe();
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    terminal.draw(|frame| self.render(frame))?;
                }
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
                Some(Ok(event)) = events.next() => {
                    if let Event::Key(key) = &event {
                        if key.kind == crossterm::event::KeyEventKind::Press {
                            match key.code {
                                KeyCode::Enter => {
                                    if !self.input_buffer.is_empty() {
                                        self.handle_command().await;
                                    }
                                }
                                KeyCode::Backspace => {
                                    self.input_buffer.pop();
                                }
                                KeyCode::Char(c) => {
                                    self.input_buffer.push(c);
                                }
                                KeyCode::Esc => {
                                    let _ = self.command_tx.send(RuntimeCommand::Shutdown);
                                }
                                _ => {}
                            }
                        }
                    }
                    terminal.draw(|frame| self.render(frame))?;
                }
            }
        }
        Ok(())
    }

    // 🚀 100% 还原并集成你原版 CLI 的所有核心业务逻辑
    async fn handle_command(&mut self) {
        let raw_cmd = self.input_buffer.clone();
        let command = raw_cmd.trim().to_string();
        self.input_buffer.clear();

        // 在终端历史中回显用户刚刚敲下的命令提示符
        let prompt_prefix = format!("[{}] {}:{}> ", self.config.node_id, self.config.peer_host, self.config.peer_port);
        self.cli_history.push(format!("{}{}", prompt_prefix, command));

        let mut parts = command.split_whitespace();
        match parts.next() {
            // 1. 还原 local 命令
            Some("local") => {
                match scan_share_dir(&self.config.share_dir, self.config.block_size) {
                    Ok(index) => {
                        if index.resources.is_empty() {
                            self.cli_history.push("No local files found.".to_string());
                        } else {
                            self.cli_history.push("file_name | block_count".to_string());
                            for r in index.resources {
                                self.cli_history.push(format!("{} | {}", r.file_name, r.blocks.len()));
                            }
                        }
                    }
                    Err(e) => self.cli_history.push(format!("Error scanning dir: {}", e)),
                }
            }
            // 2. 还原 localCplt 命令
            Some("localCplt") => {
                match scan_share_dir(&self.config.share_dir, self.config.block_size) {
                    Ok(index) => {
                        if let Ok(json) = serde_json::to_string_pretty(&index) {
                            for line in json.lines() {
                                self.cli_history.push(line.to_string());
                            }
                        }
                    }
                    Err(e) => self.cli_history.push(format!("Error: {}", e)),
                }
            }
            // 3. 还原 update 命令
            Some("update") => {
                self.cli_history.push("Publishing local files to tracker...".to_string());
                let (done_tx, done_rx) = oneshot::channel();
                if self.command_tx.send(RuntimeCommand::PublishLocalFiles { done: done_tx }).is_ok() {
                    let _ = done_rx.await;
                    self.cli_history.push("Publish complete.".to_string());
                }
            }
            // 4. 还原 list 命令
            Some("list") => {
                self.cli_history.push("Fetching file list from tracker...".to_string());
                let (done_tx, done_rx) = oneshot::channel();
                if self.command_tx.send(RuntimeCommand::ListTrackerFiles { done: done_tx }).is_ok() {
                    let _ = done_rx.await;
                    self.cli_history.push("List request finished (Check System Log for details).".to_string());
                }
            }
            // 5. 还原 download 命令
            Some("download") => {
                if let Some(target) = parts.next() {
                    self.cli_history.push(format!("Starting download task for target: {}", target));
                    let (done_tx, _done_rx) = oneshot::channel();
                    let _ = self.command_tx.send(RuntimeCommand::Download {
                        target: target.to_string(),
                        done: done_tx,
                    });
                } else {
                    self.cli_history.push("Usage: download <file_hash|file_name>".to_string());
                }
            }
            // 6. 还原 offline 命令
            Some("offline") => {
                self.cli_history.push("Marking node as offline...".to_string());
                let (done_tx, done_rx) = oneshot::channel();
                if self.command_tx.send(RuntimeCommand::MarkOffline { done: done_tx }).is_ok() {
                    let _ = done_rx.await;
                    self.cli_history.push("Offline signal sent to tracker.".to_string());
                }
            }
            // 7. 还原 help 命令
            Some("help") => {
                self.cli_history.push("Available commands:".to_string());
                self.cli_history.push("  local     Scan local files and print summary".to_string());
                self.cli_history.push("  localCplt Scan local files and print full JSON".to_string());
                self.cli_history.push("  update    Update and publish index to tracker".to_string());
                self.cli_history.push("  list      List files known by tracker".to_string());
                self.cli_history.push("  download  Download a file by hash or name".to_string());
                self.cli_history.push("  offline   Tell tracker this node is offline".to_string());
                self.cli_history.push("  help      List available commands".to_string());
                self.cli_history.push("  exit      Stop this node process".to_string());
            }
            Some("exit") => {
                let _ = self.command_tx.send(RuntimeCommand::Shutdown);
            }
            Some(unknown) => {
                self.cli_history.push(format!("Unknown command: {}", unknown));
            }
            None => {}
        }
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

        // 🚀 核心修改 1：调大底部嵌入式 CLI 的空间，切出 11 个像素高的大框给命令行
        let vlayout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Fill(1),      // 上方业务指标看板
                Constraint::Length(4),    // 传输进度条
                Constraint::Length(16)    // 🚀 下方全功能 CLI 终端模拟器
            ])
            .split(frame_size);

        let hlayout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(vlayout[0]);

        // 1. 渲染上方数据
        self.render_tui_log(frame, hlayout[0]);

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

        // 2. 渲染中间进度条
        let progress_block = Block::new().borders(Borders::ALL).title("Transmission Progress").title_style(Style::default().bold());
        let progress_gauge = Gauge::default()
            .gauge_style(Style::new().light_yellow().on_dark_gray())
            .percent(((self.progress * 100.0).round() as u16).clamp(0, 100))
            .label(format!("{:.1}%", self.progress * 100.0));
        let inner_prog = progress_block.inner(vlayout[1]);
        frame.render_widget(progress_block, vlayout[1]);
        let prog_layout = Layout::default().direction(Direction::Vertical).constraints(vec![Constraint::Length(1), Constraint::Length(1)]).split(inner_prog);
        frame.render_widget(progress_gauge, prog_layout[1]);

        // 3. 🚀 核心修改 2：渲染超酷的“嵌入式大 CLI 终端”
        let cli_block = Block::new()
            .borders(Borders::ALL)
            .title(" Integrated Node CLI Terminal ")
            .title_style(Style::default().bold().fg(Color::LightCyan));
        
        let cli_area = vlayout[2];
        let inner_cli = cli_block.inner(cli_area);
        
        // 计算文本终端能容纳的最大行数 (除去边框)
        let max_visible_lines = inner_cli.height as usize;
        
        // 构建当前终端要渲染的所有行（包含历史记录 + 底部正在输入的活动行）
        let mut display_lines: Vec<Line> = self.cli_history.iter()
            .map(|s| Line::from(s.clone().white()))
            .collect();
        
        // 动态拼接当前的 Prompt 提示符：[id] host:port> 你的输入
        let prompt_prefix = format!("[{}] {}:{}> ", self.config.node_id, self.config.peer_host, self.config.peer_port);
        let active_line = Line::from_iter([
            prompt_prefix.clone().cyan(), 
            self.input_buffer.clone().light_yellow()
        ]);
        display_lines.push(active_line);

        // 🚀 实现终端“自动滚屏”机制：如果历史行数过多，只截取最后几行显示，防止溢出
        let scrolled_lines = if display_lines.len() > max_visible_lines {
            &display_lines[display_lines.len() - max_visible_lines..]
        } else {
            &display_lines[..]
        };

        let cli_paragraph = Paragraph::new(scrolled_lines.to_vec());
        frame.render_widget(cli_block, cli_area);
        frame.render_widget(cli_paragraph, inner_cli);

        // 🚀 核心修改 3：动态计算光标应该定位在哪一行哪一列
        // Y 轴总是位于 CLI 框内的最后一行；X 轴等于提示符长度 + 用户当前输入的字符数
        let cursor_y = inner_cli.y + (scrolled_lines.len() as u16) - 1;
        let cursor_x = inner_cli.x + (prompt_prefix.len() as u16) + (self.input_buffer.len() as u16);
        frame.set_cursor(cursor_x, cursor_y);
    }
}