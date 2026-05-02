mod agent;

use agent::{Agent, Message};
use color_eyre::eyre::{eyre, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, BufRead, BufReader},
    path::PathBuf,
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
};

#[derive(Serialize, Deserialize, Debug)]
struct HermesConfig {
    model: Option<ModelConfig>,
    logging: Option<LoggingConfig>,
    toolsets: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ModelConfig {
    default: Option<String>,
    base_url: Option<String>,
    provider: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct LoggingConfig {
    level: Option<String>,
}

enum Tab {
    Dashboard,
    Chat,
    Launcher,
    Logs,
}

struct App {
    current_tab: Tab,
    config: Option<HermesConfig>,
    launcher_state: ListState,
    log_content: Vec<String>,
    hermes_path: PathBuf,
    // Agent state
    agent: Agent,
    input: String,
    is_loading: bool,
    tx: Sender<Result<String>>,
    rx: Receiver<Result<String>>,
}

impl App {
    fn new() -> Self {
        let home = directories::UserDirs::new()
            .map(|u| u.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/home/jeb"));
        let hermes_path = home.join(".hermes");
        
        let config_str = fs::read_to_string(hermes_path.join("config.yaml")).unwrap_or_default();
        let config: Option<HermesConfig> = serde_yaml::from_str(&config_str).ok();

        let base_url = config.as_ref()
            .and_then(|c| c.model.as_ref())
            .and_then(|m| m.base_url.as_ref())
            .cloned()
            .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
        
        let model = config.as_ref()
            .and_then(|c| c.model.as_ref())
            .and_then(|m| m.default.as_ref())
            .cloned()
            .unwrap_or_else(|| "qwen3.5:4b".to_string());

        let mut launcher_state = ListState::default();
        launcher_state.select(Some(0));

        let (tx, rx) = mpsc::channel();

        Self {
            current_tab: Tab::Dashboard,
            config,
            launcher_state,
            log_content: Vec::new(),
            hermes_path,
            agent: Agent::new(&base_url, &model),
            input: String::new(),
            is_loading: false,
            tx,
            rx,
        }
    }

    fn next_tab(&mut self) {
        self.current_tab = match self.current_tab {
            Tab::Dashboard => Tab::Chat,
            Tab::Chat => Tab::Launcher,
            Tab::Launcher => Tab::Logs,
            Tab::Logs => Tab::Dashboard,
        };
        if let Tab::Logs = self.current_tab {
            self.refresh_logs();
        }
    }

    fn refresh_logs(&mut self) {
        let log_path = self.hermes_path.join("logs/agent.log");
        if let Ok(file) = fs::File::open(log_path) {
            let reader = BufReader::new(file);
            self.log_content = reader
                .lines()
                .filter_map(|line| line.ok())
                .collect();
            if self.log_content.len() > 100 {
                self.log_content = self.log_content.split_off(self.log_content.len() - 100);
            }
        }
    }

    fn launcher_next(&mut self) {
        let i = match self.launcher_state.selected() {
            Some(i) => if i >= 3 { 0 } else { i + 1 },
            None => 0,
        };
        self.launcher_state.select(Some(i));
    }

    fn launcher_prev(&mut self) {
        let i = match self.launcher_state.selected() {
            Some(i) => if i == 0 { 3 } else { i - 1 },
            None => 0,
        };
        self.launcher_state.select(Some(i));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    color_eyre::install()?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn print_help() {
    println!(r#"
Hermes Agent Wizard - Verbose Help ⚕️

The Hermes Agent Wizard is a Terminal User Interface (TUI) designed to simplify 
management and interaction with your local Hermes Agent.

USAGE:
    hermes-agent-wizard [FLAGS]

FLAGS:
    -h, --help      Prints this verbose help information and exits.

TUI NAVIGATION & SHORTCUTS:
    [Tab]           Cycle through views: Dashboard -> Chat -> Launcher -> Logs.
    [↑/↓ Arrows]    Navigate through items in the Launcher menu.
    [Enter]         Execute the highlighted action in the Launcher.
    [Chars]         Type messages in the Chat tab.
    [q]             Quit the wizard and restore terminal state.

DETAILED VIEW EXPLANATIONS:

  1. DASHBOARD VIEW
     - Status Overview: Confirms the presence of the ~/.hermes directory.
     - Model Info: Displays the default LLM (e.g., qwen3.5:4b) and the provider.

  2. CHAT VIEW (BETA - Native Rust)
     - Chat directly with your model without spawning external Python processes.
     - Note: This is a pure LLM chat and does not yet support the full Python toolset.

  3. LAUNCHER VIEW (Action Center)
     - Launch Hermes CLI: Spawns the full Python-based CLI (cli.py).
     - Tirith Doctor: Runs a diagnostic check on the Tirith security layer.
     - Edit Config: Spawns a 'nano' session directly to ~/.hermes/config.yaml.
     - View Live Logs: Executes 'tail -f' on the agent.log.

  4. LOGS VIEW
     - Displays a static snapshot of the last 100 lines of your agent.log.

"#);
}

async fn run_app<B: Backend + io::Write>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app)).map_err(|e| eyre!(e.to_string()))?;

        // Check for async chat response
        if let Ok(result) = app.rx.try_recv() {
            app.is_loading = false;
            if let Err(e) = result {
                app.agent.history.push(Message {
                    role: "assistant".to_string(),
                    content: format!("Error: {}", e),
                });
            }
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if app.is_loading {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Tab => app.next_tab(),
                    KeyCode::Char(c) if matches!(app.current_tab, Tab::Chat) => {
                        app.input.push(c);
                    }
                    KeyCode::Backspace if matches!(app.current_tab, Tab::Chat) => {
                        app.input.pop();
                    }
                    KeyCode::Enter if matches!(app.current_tab, Tab::Chat) => {
                        if !app.input.is_empty() {
                            let input = std::mem::take(&mut app.input);
                            app.is_loading = true;
                            
                            // Need a clones for the async task
                            let mut agent = app.agent.clone(); 
                            // Actually we want to keep history synced, so we need to update app.agent.history manually
                            app.agent.history.push(Message {
                                role: "user".to_string(),
                                content: input.clone(),
                            });
                            
                            let tx = app.tx.clone();
                            let base_url = app.agent.base_url.clone();
                            let model = app.agent.model.clone();
                            let history = app.agent.history.clone();

                            tokio::spawn(async move {
                                let mut temp_agent = Agent::new(&base_url, &model);
                                temp_agent.history = history;
                                // We don't want the chat method to push 'user' again, so we'll implement a custom call
                                let url = format!("{}/chat/completions", base_url);
                                let client = reqwest::Client::new();
                                let request = serde_json::json!({
                                    "model": model,
                                    "messages": temp_agent.history
                                });
                                
                                let res = client.post(&url).json(&request).send().await;
                                match res {
                                    Ok(resp) => {
                                        let json = resp.json::<serde_json::Value>().await;
                                        match json {
                                            Ok(val) => {
                                                if let Some(content) = val["choices"][0]["message"]["content"].as_str() {
                                                    tx.send(Ok(content.to_string())).ok();
                                                } else {
                                                    tx.send(Err(eyre!("No content"))).ok();
                                                }
                                            }
                                            Err(e) => { tx.send(Err(eyre!(e.to_string()))).ok(); }
                                        }
                                    }
                                    Err(e) => { tx.send(Err(eyre!(e.to_string()))).ok(); }
                                }
                            });
                        }
                    }
                    KeyCode::Down => {
                        if let Tab::Launcher = app.current_tab {
                            app.launcher_next();
                        }
                    }
                    KeyCode::Up => {
                        if let Tab::Launcher = app.current_tab {
                            app.launcher_prev();
                        }
                    }
                    KeyCode::Enter => {
                        if let Tab::Launcher = app.current_tab {
                            if let Some(selected) = app.launcher_state.selected() {
                                match selected {
                                    0 => {
                                        // Still launch the old CLI as a fallback or for the 'Full' experience
                                        let hermes_dir = "/home/jeb/programs/hermes-agent-commissioning-20260401_175500/hermes-agent";
                                        let venv_python = format!("{}/venv/bin/python3", hermes_dir);
                                        let cli_script = format!("{}/cli.py", hermes_dir);
                                        execute_command(terminal, &venv_python, &[&cli_script])?;
                                    }
                                    1 => {
                                        let tirith_path = app.hermes_path.join("bin/tirith");
                                        let tirith_str = tirith_path.to_str().unwrap_or("tirith");
                                        execute_command(terminal, tirith_str, &["doctor"])?;
                                    }
                                    2 => {
                                        let config_path = app.hermes_path.join("config.yaml");
                                        execute_command(terminal, "nano", &[config_path.to_str().unwrap()])?;
                                    }
                                    3 => {
                                        let log_path = app.hermes_path.join("logs/agent.log");
                                        execute_command(terminal, "tail", &["-f", log_path.to_str().unwrap()])?;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

// Implement clone for Agent to pass to tasks
impl Clone for Agent {
    fn clone(&self) -> Self {
        Agent {
            client: reqwest::Client::new(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            history: self.history.clone(),
        }
    }
}

fn execute_command<B: Backend + io::Write>(terminal: &mut Terminal<B>, cmd: &str, args: &[&str]) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor().map_err(|e| eyre!(e.to_string()))?;

    Command::new(cmd).args(args).status()?;

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
    terminal.clear().map_err(|e| eyre!(e.to_string()))?;
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)].as_ref())
        .split(size);

    let titles = vec![" Dashboard ", " Chat (BETA) ", " Launcher ", " Logs "];
    let index = match app.current_tab {
        Tab::Dashboard => 0,
        Tab::Chat => 1,
        Tab::Launcher => 2,
        Tab::Logs => 3,
    };
    let tabs = Tabs::new(titles.iter().cloned().map(Line::from).collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL).title(" Hermes Agent Wizard "))
        .select(index)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::Black).fg(Color::Yellow));
    f.render_widget(tabs, chunks[0]);

    match app.current_tab {
        Tab::Dashboard => render_dashboard(f, app, chunks[1]),
        Tab::Chat => render_chat(f, app, chunks[1]),
        Tab::Launcher => render_launcher(f, app, chunks[1]),
        Tab::Logs => render_logs(f, app, chunks[1]),
    };

    let help_text = match app.current_tab {
        Tab::Dashboard => "Tab: Switch | q: Quit",
        Tab::Chat => "Enter: Send | Backspace: Delete | Tab: Switch | q: Quit",
        Tab::Launcher => "↑/↓: Select | Enter: Execute | Tab: Switch | q: Quit",
        Tab::Logs => "Tab: Switch | q: Quit",
    };
    let help = Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title(" Help "));
    f.render_widget(help, chunks[2]);
}

fn render_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let mut text = vec![
        Line::from(vec![
            Span::raw("Hermes Path: "),
            Span::styled(app.hermes_path.to_str().unwrap_or("Unknown"), Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
    ];

    if let Some(config) = &app.config {
        if let Some(model) = &config.model {
            text.push(Line::from(vec![
                Span::raw("Default Model: "),
                Span::styled(model.default.as_deref().unwrap_or("N/A"), Style::default().fg(Color::Yellow)),
            ]));
            text.push(Line::from(vec![
                Span::raw("Provider:      "),
                Span::styled(model.provider.as_deref().unwrap_or("N/A"), Style::default().fg(Color::Yellow)),
            ]));
        }
        if let Some(logging) = &config.logging {
            text.push(Line::from(vec![
                Span::raw("Log Level:      "),
                Span::styled(logging.level.as_deref().unwrap_or("INFO"), Style::default().fg(Color::Blue)),
            ]));
        }
    } else {
        text.push(Line::from(Span::styled("Failed to load config.yaml", Style::default().fg(Color::Red))));
    }

    let dashboard = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Dashboard "))
        .wrap(Wrap { trim: true });
    f.render_widget(dashboard, area);
}

fn render_chat(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
        .split(area);

    let mut messages = Vec::new();
    for msg in &app.agent.history {
        let color = if msg.role == "user" { Color::Green } else { Color::Yellow };
        messages.push(ListItem::new(vec![
            Line::from(Span::styled(format!("{}: ", msg.role.to_uppercase()), Style::default().fg(color).add_modifier(Modifier::BOLD))),
            Line::from(msg.content.clone()),
            Line::from(""),
        ]));
    }

    let history_list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title(" Conversation History "));
    f.render_widget(history_list, chunks[0]);

    let input_text = if app.is_loading { "Sending..." } else { &app.input };
    let input = Paragraph::new(input_text)
        .block(Block::default().borders(Borders::ALL).title(" Send Message "));
    f.render_widget(input, chunks[1]);
}

fn render_launcher(f: &mut Frame, app: &mut App, area: Rect) {
    let items = vec![
        ListItem::new("🚀 Launch Hermes Interactive CLI (Python Fallback)"),
        ListItem::new("⚕️  Run Tirith Security Diagnosis (doctor)"),
        ListItem::new("⚙️  Edit Configuration (nano ~/.hermes/config.yaml)"),
        ListItem::new("📋 View Live Logs (tail -f ~/.hermes/logs/agent.log)"),
    ];
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Launcher "))
        .highlight_style(Style::default().bg(Color::LightBlue).fg(Color::Black).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");
    f.render_stateful_widget(list, area, &mut app.launcher_state);
}

fn render_logs(f: &mut Frame, app: &App, area: Rect) {
    let logs: Vec<ListItem> = app.log_content.iter().map(|l| ListItem::new(l.as_str())).collect();
    let list = List::new(logs).block(Block::default().borders(Borders::ALL).title(" Recent Logs (Last 100 lines) "));
    f.render_widget(list, area);
}
