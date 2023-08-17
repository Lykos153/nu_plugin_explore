mod config;
mod terminal;
mod tui;

use anyhow::{Context, Result};
use ratatui::{prelude::CrosstermBackend, style::Color, Terminal};

use nu_plugin::{EvaluatedCall, LabeledError, Plugin};
use nu_protocol::{
    ast::{CellPath, PathMember},
    Category, PluginExample, PluginSignature, Span, Type, Value,
};

use config::{Config, KeyBindingsMap, NavigationBindingsMap, StatusBarConfig};
use terminal::setup as setup_terminal;
use terminal::restore as restore_terminal;

pub struct Explore;

impl Plugin for Explore {
    fn signature(&self) -> Vec<PluginSignature> {
        vec![PluginSignature::build("explore")
            .usage("TODO")
            .input_output_type(Type::Any, Type::Nothing)
            .plugin_examples(vec![PluginExample {
                example: "open Cargo.toml | explore".into(),
                description: "TODO".into(),
                result: None,
            }])
            .category(Category::Experimental)]
    }

    fn run(
        &mut self,
        name: &str,
        call: &EvaluatedCall,
        input: &Value,
    ) -> Result<Value, LabeledError> {
        match name {
            "explore" => explore(call, input),
            _ => Err(LabeledError {
                label: "Plugin call with wrong name signature".into(),
                msg: "the signature used to call the plugin does not match any name in the plugin signature vector".into(),
                span: Some(call.head),
            }),
        }
    }
}

fn explore(call: &EvaluatedCall, input: &Value) -> Result<Value, LabeledError> {
    let config = Config {
        show_cell_path: true,
        status_bar: StatusBarConfig {
            background: Color::White,
            foreground: Color::Black,
        },
        keybindings: KeyBindingsMap {
            quit: 'q',
            insert: 'i',
            normal: 'n',
            navigation: NavigationBindingsMap {
                left: 'h',
                down: 'j',
                up: 'k',
                right: 'l',
            },
        },
    };

    let mut terminal = setup_terminal().context("setup failed").unwrap();
    run(&mut terminal, input, &config)
        .context("app loop failed")
        .unwrap();
    restore_terminal(&mut terminal)
        .context("restore terminal failed")
        .unwrap();

    Ok(Value::nothing(call.head))
}

#[derive(PartialEq)]
enum Mode {
    Normal,
    Insert,
}

impl Mode {
    fn default() -> Mode {
        Mode::Normal
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let repr = match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
        };
        write!(f, "{}", repr)
    }
}

struct State {
    cell_path: CellPath,
    bottom: bool,
    mode: Mode,
}

impl State {
    fn default() -> State {
        State {
            cell_path: CellPath { members: vec![] },
            bottom: false,
            mode: Mode::default(),
        }
    }
}

enum Direction {
    Down,
    Up,
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<console::Term>>,
    input: &Value,
    config: &Config,
) -> Result<()> {
    let mut state = State::default();
    match input {
        Value::List { vals, .. } => state.cell_path.members.push(PathMember::Int {
            val: 0,
            span: Span::unknown(),
            optional: vals.is_empty(),
        }),
        Value::Record { cols, .. } => state.cell_path.members.push(PathMember::String {
            val: cols.get(0).unwrap_or(&"".to_string()).into(),
            span: Span::unknown(),
            optional: cols.is_empty(),
        }),
        _ => {}
    };

    loop {
        terminal.draw(|frame| tui::render_ui(frame, input, &state, config))?;

        let char = console::Term::stderr().read_char()?;
        if char == config.keybindings.quit {
            break;
        } else if char == config.keybindings.insert {
            state.mode = Mode::Insert;
        } else if char == config.keybindings.normal {
            state.mode = Mode::Normal;
        } else if char == config.keybindings.navigation.down {
            if state.mode == Mode::Normal {
                go_up_or_down_in_data(&mut state, input, Direction::Down);
            }
        } else if char == config.keybindings.navigation.up {
            if state.mode == Mode::Normal {
                go_up_or_down_in_data(&mut state, input, Direction::Up);
            }
        } else if char == config.keybindings.navigation.right {
            if state.mode == Mode::Normal {
                go_deeper_in_data(&mut state, input);
            }
        } else if char == config.keybindings.navigation.left {
            if state.mode == Mode::Normal {
                go_back_in_data(&mut state);
            }
        }
    }
    Ok(())
}

fn go_up_or_down_in_data(state: &mut State, input: &Value, direction: Direction) {
    if state.bottom {
        return ();
    }

    let direction = match direction {
        Direction::Up => usize::MAX,
        Direction::Down => 1,
    };

    let current = state.cell_path.members.pop();

    match input
        .clone()
        .follow_cell_path(&state.cell_path.members, false)
    {
        Ok(Value::List { vals, .. }) => {
            let new = match current {
                Some(PathMember::Int {
                    val,
                    span,
                    optional,
                }) => PathMember::Int {
                    val: if vals.is_empty() {
                        val
                    } else {
                        (val + direction + vals.len()) % vals.len()
                    },
                    span,
                    optional,
                },
                None => panic!("unexpected error when unpacking current cell path"),
                _ => panic!("current should be an integer path member"),
            };
            state.cell_path.members.push(new);
        }
        Ok(Value::Record { cols, .. }) => {
            let new = match current {
                Some(PathMember::String {
                    val,
                    span,
                    optional,
                }) => PathMember::String {
                    val: if cols.is_empty() {
                        "".into()
                    } else {
                        let index = cols.iter().position(|x| x == &val).unwrap();
                        cols[(index + direction + cols.len()) % cols.len()].clone()
                    },
                    span,
                    optional,
                },
                None => panic!("unexpected error when unpacking current cell path"),
                _ => panic!("current should be an string path member"),
            };
            state.cell_path.members.push(new);
        }
        Err(_) => panic!("unexpected error when following cell path"),
        _ => {}
    }
}

fn go_deeper_in_data(state: &mut State, input: &Value) {
    match input
        .clone()
        .follow_cell_path(&state.cell_path.members, false)
    {
        Ok(Value::List { vals, .. }) => state.cell_path.members.push(PathMember::Int {
            val: 0,
            span: Span::unknown(),
            optional: vals.is_empty(),
        }),
        Ok(Value::Record { cols, .. }) => state.cell_path.members.push(PathMember::String {
            val: cols.get(0).unwrap_or(&"".to_string()).into(),
            span: Span::unknown(),
            optional: cols.is_empty(),
        }),
        Err(_) => panic!("unexpected error when following cell path"),
        _ => state.bottom = true,
    }
}

fn go_back_in_data(state: &mut State) {
    if !state.bottom & (state.cell_path.members.len() > 1) {
        state.cell_path.members.pop();
    }
    state.bottom = false;
}
