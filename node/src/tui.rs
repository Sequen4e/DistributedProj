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
        
        //   初始化控制台终端欢迎语（现在它会在左侧大面板回显）
        let initial_history = vec![
            "Welcome to Resource Node Console Hub".to_string(),
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
            cli_history: initial_history, 
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
    pub cli_history: Vec<String>, 
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
                        BroadcastSignal::ConsoleLog(msg) => {
                            self.cli_history.push(msg);
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

    async fn handle_command(&mut self) {
        let raw_cmd = self.input_buffer.clone();
        let command = raw_cmd.trim().to_string();
        self.input_buffer.clear();

        let prompt_prefix = format!("[{}] {}:{}> ", self.config.node_id, self.config.peer_host, self.config.peer_port);
        self.cli_history.push(format!("{}{}", prompt_prefix, command));

        let mut parts = command.split_whitespace();
        match parts.next() {
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
            Some("update") => {
                self.cli_history.push("Publishing local files to tracker...".to_string());
                let (done_tx, done_rx) = oneshot::channel();
                if self.command_tx.send(RuntimeCommand::PublishLocalFiles { done: done_tx }).is_ok() {
                    let _ = done_rx.await;
                    self.cli_history.push("Publish complete.".to_string());
                }
            }
            Some("list") => {
                self.cli_history.push("Fetching file list from tracker...".to_string());
                let (done_tx, done_rx) = oneshot::channel();
                if self.command_tx.send(RuntimeCommand::ListTrackerFiles { done: done_tx }).is_ok() {
                    let _ = done_rx.await;
                    self.cli_history.push("List request finished.".to_string());
                }
            }
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
            Some("offline") => {
                self.cli_history.push("Marking node as offline...".to_string());
                let (done_tx, done_rx) = oneshot::channel();
                if self.command_tx.send(RuntimeCommand::MarkOffline { done: done_tx }).is_ok() {
                    let _ = done_rx.await;
                    self.cli_history.push("Offline signal sent to tracker.".to_string());
                }
            }
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

    fn render(&self, frame: &mut Frame) {
        let frame_size = frame.size();

        //   布局修改 1：底部只切出 3 个像素高度（除去上下边框，正好剩下 1 行用来打字输入）
        let vlayout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Fill(1),      // 上方核心大看板（包含左侧重定向区域 + 右侧详情）
                Constraint::Length(4),    // 中间传输进度条
                Constraint::Length(3)     //   底部单行命令输入框
            ])
            .split(frame_size);

        let hlayout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(vlayout[0]);

        //   布局修改 2：把左侧改造成接收“控制台输出历史”的滚动视图
        let console_block = Block::new()
            .borders(Borders::ALL)
            .title(" Console Command Output ")
            .title_style(Style::default().bold().fg(Color::LightCyan));
        
        let console_area = hlayout[0];
        let inner_console = console_block.inner(console_area);
        
        // 动态计算左侧巨大的可视行数
        let max_console_lines = inner_console.height as usize;
        let console_lines: Vec<Line> = self.cli_history.iter()
            .map(|s| Line::from(s.clone().white()))
            .collect();

        // 超长文本自动滚屏切片
        let scrolled_console_lines = if console_lines.len() > max_console_lines {
            &console_lines[console_lines.len() - max_console_lines..]
        } else {
            &console_lines[..]
        };

        let console_paragraph = Paragraph::new(scrolled_console_lines.to_vec());
        frame.render_widget(console_block, console_area);
        frame.render_widget(console_paragraph, inner_console);

        // 2. 右侧任务信息（保持不变）
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

        // 3. 中间进度条（保持不变）
        let progress_block = Block::new().borders(Borders::ALL).title("Transmission Progress").title_style(Style::default().bold());
        let progress_gauge = Gauge::default()
            .gauge_style(Style::new().light_yellow().on_dark_gray())
            .percent(((self.progress * 100.0).round() as u16).clamp(0, 100))
            .label(format!("{:.1}%", self.progress * 100.0));
        let inner_prog = progress_block.inner(vlayout[1]);
        frame.render_widget(progress_block, vlayout[1]);
        let prog_layout = Layout::default().direction(Direction::Vertical).constraints(vec![Constraint::Length(1), Constraint::Length(1)]).split(inner_prog);
        frame.render_widget(progress_gauge, prog_layout[1]);

        //   布局修改 3：渲染极简的底部单行输入框
        let input_block = Block::new()
            .borders(Borders::ALL)
            .title(" Command Input (e.g., `local`, `localCplt`, `download <hash>`) ")
            .title_style(Style::default().bold().fg(Color::LightYellow));
        
        let input_area = vlayout[2];
        let inner_input = input_block.inner(input_area);
        
        // 渲染单行的活动输入：[id] host:port> 你的打字内容
        let prompt_prefix = format!("[{}] {}:{}> ", self.config.node_id, self.config.peer_host, self.config.peer_port);
        let active_line = Line::from_iter([
            prompt_prefix.clone().cyan(), 
            self.input_buffer.clone().light_yellow()
        ]);

        let input_paragraph = Paragraph::new(active_line);
        frame.render_widget(input_block, input_area);
        frame.render_widget(input_paragraph, inner_input);

        //   布局修改 4：由于输入框只有一行，光标的 Y 轴永远固定，X 轴跟随打字长度变化即可
        let cursor_x = inner_input.x + (prompt_prefix.len() as u16) + (self.input_buffer.len() as u16);
        let cursor_y = inner_input.y;
        frame.set_cursor(cursor_x, cursor_y);
    }
}