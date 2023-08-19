//! the higher level application
//!
//! this module mostly handles
//! 1. the main TUI loop
//! 1. the rendering
//! 1. the keybindings
//! 1. the internal state of the application
use anyhow::Result;
use ratatui::{prelude::CrosstermBackend, Terminal};

use nu_protocol::{
    ast::{CellPath, PathMember},
    Span, Value,
};

use super::navigation::Direction;
use super::{config::Config, navigation, tui};

/// the mode in which the application is
#[derive(PartialEq)]
pub(super) enum Mode {
    /// the NORMAL mode is the *navigation* mode, where the user can move around in the data
    Normal,
    /// the INSERT mode lets the user edit cells of the structured data
    Insert,
    /// the PEEKING mode lets the user *peek* data out of the application, to be reused later
    Peeking,
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
            Mode::Peeking => "PEEKING",
        };
        write!(f, "{}", repr)
    }
}

/// the complete state of the application
pub(super) struct State {
    /// the full current path in the data
    pub cell_path: CellPath,
    /// tells whether or not the user is at the bottom of the data or not, used for rendering in
    /// [`tui`]
    pub bottom: bool,
    /// the current [`Mode`]
    pub mode: Mode,
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

/// run the application
///
/// this function
/// 1. creates the initial [`State`]
/// 1. runs the main application loop
///
/// the application loop
/// 1. renders the TUI with [`tui`]
/// 1. reads the user's input keys and transition the [`State`] accordingly
pub(super) fn run(
    terminal: &mut Terminal<CrosstermBackend<console::Term>>,
    input: &Value,
    config: &Config,
) -> Result<Value> {
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

        let key = console::Term::stderr().read_key()?;

        if key == config.keybindings.quit {
            break;
        } else if key == config.keybindings.insert {
            if state.mode == Mode::Normal {
                state.mode = Mode::Insert;
            }
        } else if key == config.keybindings.normal {
            if state.mode == Mode::Insert {
                state.mode = Mode::Normal;
            }
        } else if key == config.keybindings.navigation.down {
            if state.mode == Mode::Normal {
                navigation::go_up_or_down_in_data(&mut state, input, Direction::Down);
            }
        } else if key == config.keybindings.navigation.up {
            if state.mode == Mode::Normal {
                navigation::go_up_or_down_in_data(&mut state, input, Direction::Up);
            }
        } else if key == config.keybindings.navigation.right {
            if state.mode == Mode::Normal {
                navigation::go_deeper_in_data(&mut state, input);
            }
        } else if key == config.keybindings.navigation.left {
            if state.mode == Mode::Normal {
                navigation::go_back_in_data(&mut state);
            }
        } else if key == config.keybindings.peek {
            if state.mode == Mode::Normal {
                state.mode = Mode::Peeking;
            }
        }

        if state.mode == Mode::Peeking {
            if key == config.keybindings.peeking.quit {
                state.mode = Mode::Normal;
            } else if key == config.keybindings.peeking.all {
                return Ok(input.clone());
            } else if key == config.keybindings.peeking.current {
                state.cell_path.members.pop();
                return Ok(input
                    .clone()
                    .follow_cell_path(&state.cell_path.members, false)?);
            } else if key == config.keybindings.peeking.under {
                return Ok(input
                    .clone()
                    .follow_cell_path(&state.cell_path.members, false)?);
            }
        }
    }
    Ok(Value::nothing(Span::unknown()))
}
