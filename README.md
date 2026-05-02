# Hermes Agent Wizard

An interactive TUI launcher for the Hermes Agent, built with Rust and Ratatui.

## Features
- **Dashboard**: View current Hermes configuration, default model, and active toolsets.
- **Launcher**: Quickly start the interactive chat (`tirith`), edit the configuration file, or tail logs.
- **Logs**: View the last 100 lines of the Hermes agent log.

## Usage

### Shortcuts
- `Tab`: Switch between Dashboard, Launcher, and Logs.
- `↑ / ↓`: Navigate the Launcher menu.
- `Enter`: Execute the selected action in the Launcher.
- `q`: Quit the wizard.

### Running
To run the wizard:
```bash
cd hermes-agent-wizard
cargo run
```

## Architecture & Flow

### System Diagram
```dot
digraph G {
    node [shape=box, style=filled, color=lightblue];
    User -> "Hermes Agent Wizard" [label="CLI Invocation"];
    "Hermes Agent Wizard" -> Dashboard [label="Tab 1"];
    "Hermes Agent Wizard" -> Launcher [label="Tab 2"];
    "Hermes Agent Wizard" -> Logs [label="Tab 3"];
    
    Dashboard -> "~/.hermes/config.yaml" [label="Parses"];
    Launcher -> "Hermes CLI (cli.py)" [label="Executes"];
    Launcher -> "Tirith (doctor)" [label="Executes"];
    Launcher -> Nano [label="Edits Config"];
    Logs -> "agent.log" [label="Reads"];
}
```

### Wizard Logic Flow
```dot
digraph Flow {
    rankdir=LR;
    Start -> "TUI Init" -> "Main Loop";
    "Main Loop" -> "Event Handling" [label="Key Press"];
    "Event Handling" -> "State Update";
    "Event Handling" -> "External Process" [label="Enter (Launcher)"];
    "External Process" -> "TUI Suspend" -> "Run Command" -> "TUI Resume";
    "State Update" -> "Render Frame" -> "Main Loop";
}
```

