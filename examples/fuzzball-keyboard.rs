//! Demonstrates the Fuzzball keyboard protocol (`--features fuzzball`).
//!
//! cargo run --example fuzzball-keyboard --features fuzzball
//!
//! Prints every key event (press, repeat and release) until Esc is pressed.

use std::io::{self, stdout};

use crossterm::{
    event::{
        fuzzball::{DisableFuzzballKeyboard, EnableFuzzballKeyboard},
        read, Event, KeyCode, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let supported = crossterm::terminal::supports_fuzzball_keyboard().unwrap_or(false);
    print!("Fuzzball keyboard supported: {supported}\r\n");
    if supported {
        execute!(stdout(), EnableFuzzballKeyboard)?;
    }
    print!("Press keys; Esc quits.\r\n");
    let result = (|| loop {
        match read()? {
            Event::Key(k) if k.code == KeyCode::Esc && k.kind == KeyEventKind::Press => {
                return Ok(())
            }
            event => print!("{event:?}\r\n"),
        }
    })();
    if supported {
        execute!(stdout(), DisableFuzzballKeyboard)?;
    }
    disable_raw_mode()?;
    result
}
