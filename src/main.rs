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
    provider: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct LoggingConfig {
    level: Option<String>,
}

enum Tab {
    Dashboard,
    Launcher,
    Logs,
}

struct App {
    current_tab: Tab,
    config: Option<HermesConfig>,
    launcher_state: ListState,
    log_content: Vec<String>,
    hermes_path: PathBuf,
}

impl App {
    fn new() -> Self {
        let home = directories::UserDirs::new()
            .map(|u| u.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/home/jeb"));
        let hermes_path = home.join(".hermes");
        
        let config = fs::read_to_string(hermes_path.join("config.yaml"))
            .ok()
            .and_then(|content| serde_yaml::from_str(&content).ok());

        let mut launcher_state = ListState::default();
        launcher_state.select(Some(0));

        Self {
            current_tab: Tab::Dashboard,
            config,
            launcher_state,
            log_content: Vec::new(),
            hermes_path,
        }
    }

    fn next_tab(&mut self) {
        self.current_tab = match self.current_tab {
            Tab::Dashboard => Tab::Launcher,
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
            // Keep last 100 lines
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

fn main() -> Result<()> {
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
    let res = run_app(&mut terminal, &mut app);

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
    [Tab]           Cycle through views: Dashboard -> Launcher -> Logs.
    [↑/↓ Arrows]    Navigate through items in the Launcher menu.
    [Enter]         Execute the highlighted action in the Launcher.
    [q]             Quit the wizard and restore terminal state.

DETAILED VIEW EXPLANATIONS:

  1. DASHBOARD VIEW
     - Status Overview: Confirms the presence of the ~/.hermes directory.
     - Model Info: Displays the default LLM (e.g., qwen3.5:4b) and the provider.
     - Toolsets: Lists all active toolsets currently enabled in your agent.

  2. LAUNCHER VIEW (Action Center)
     - Launch Hermes CLI: Initiates the primary Python-based Interactive CLI (cli.py).
       This is the main interface for chatting with the agent.
     - Tirith Doctor: Runs a diagnostic check on the Tirith security layer to
       verify installation health and shell hook status.
     - Edit Config: Spawns a 'nano' session directly to ~/.hermes/config.yaml.
     - View Live Logs: Executes 'tail -f' on the agent.log for real-time monitoring.

  3. LOGS VIEW
     - Displays a static snapshot of the last 100 lines of your agent.log.
     - Useful for quick verification without leaving the wizard.

SYSTEM REQUIREMENTS:
    - Hermes Agent: Must be initialized at ~/.hermes.
    - Python Venv: Expects a virtual environment in the agent source directory.
    - Terminal: Supports most modern terminal emulators (Xterm, Alacritty, iTerm2).

"#);
}

fn run_app<B: Backend + io::Write>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app)).map_err(|e| eyre!(e.to_string()))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Tab => app.next_tab(),
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
                                        // Launch Hermes CLI
                                        let hermes_dir = "/home/jeb/programs/hermes-agent-commissioning-20260401_175500/hermes-agent";
                                        let venv_python = format!("{}/venv/bin/python3", hermes_dir);
                                        let cli_script = format!("{}/cli.py", hermes_dir);
                                        execute_command(terminal, &venv_python, &[&cli_script])?;
                                    }
                                    1 => {
                                        // Tirith Doctor
                                        let tirith_path = app.hermes_path.join("bin/tirith");
                                        let tirith_str = tirith_path.to_str().unwrap_or("tirith");
                                        execute_command(terminal, tirith_str, &["doctor"])?;
                                    }
                                    2 => {
                                        // Edit config
                                        let config_path = app.hermes_path.join("config.yaml");
                                        execute_command(terminal, "nano", &[config_path.to_str().unwrap()])?;
                                    }
                                    3 => {
                                        // Tail logs
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

fn execute_command<B: Backend + io::Write>(terminal: &mut Terminal<B>, cmd: &str, args: &[&str]) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor().map_err(|e| eyre!(e.to_string()))?;

    println!("Executing: {} {}", cmd, args.join(" "));
    let status = Command::new(cmd)
        .args(args)
        .status();

    match status {
        Ok(s) if s.success() => {
            // Success
        }
        Ok(s) => {
            println!("\nCommand '{}' exited with status: {}", cmd, s);
            println!("Press Enter to return to the wizard...");
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
        }
        Err(e) => {
            println!("\nFailed to execute '{}': {}", cmd, e);
            println!("Press Enter to return to the wizard...");
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
        }
    }

    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    terminal.clear().map_err(|e| eyre!(e.to_string()))?;
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    let titles = vec![" Dashboard ", " Launcher ", " Logs "];
    let index = match app.current_tab {
        Tab::Dashboard => 0,
        Tab::Launcher => 1,
        Tab::Logs => 2,
    };
    let tabs = Tabs::new(titles.iter().cloned().map(Line::from).collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL).title(" Hermes Agent Wizard "))
        .select(index)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Black)
                .fg(Color::Yellow),
        );
    f.render_widget(tabs, chunks[0]);

    match app.current_tab {
        Tab::Dashboard => render_dashboard(f, app, chunks[1]),
        Tab::Launcher => render_launcher(f, app, chunks[1]),
        Tab::Logs => render_logs(f, app, chunks[1]),
    };

    let help_text = match app.current_tab {
        Tab::Dashboard => "Tab: Switch | q: Quit",
        Tab::Launcher => "↑/↓: Select | Enter: Execute | Tab: Switch | q: Quit",
        Tab::Logs => "Tab: Switch | q: Quit",
    };
    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Help "));
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
        if let Some(toolsets) = &config.toolsets {
            text.push(Line::from(""));
            text.push(Line::from("Active Toolsets:"));
            for ts in toolsets {
                text.push(Line::from(format!(" - {}", ts)));
            }
        }
    } else {
        text.push(Line::from(Span::styled("Failed to load config.yaml", Style::default().fg(Color::Red))));
    }

    let dashboard = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Dashboard "))
        .wrap(Wrap { trim: true });
    f.render_widget(dashboard, area);
}

fn render_launcher(f: &mut Frame, app: &mut App, area: Rect) {
    let items = vec![
        ListItem::new("🚀 Launch Hermes Interactive CLI (cli.py)"),
        ListItem::new("⚕️  Run Tirith Security Diagnosis (doctor)"),
        ListItem::new("⚙️  Edit Configuration (nano ~/.hermes/config.yaml)"),
        ListItem::new("📋 View Live Logs (tail -f ~/.hermes/logs/agent.log)"),
    ];
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Launcher "))
        .highlight_style(
            Style::default()
                .bg(Color::LightBlue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    f.render_stateful_widget(list, area, &mut app.launcher_state);
}

fn render_logs(f: &mut Frame, app: &App, area: Rect) {
    let logs: Vec<ListItem> = app
        .log_content
        .iter()
        .map(|l| ListItem::new(l.as_str()))
        .collect();
    let list = List::new(logs)
        .block(Block::default().borders(Borders::ALL).title(" Recent Logs (Last 100 lines) "));
    f.render_widget(list, area);
}
