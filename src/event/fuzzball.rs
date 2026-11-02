//! Fuzzball keyboard protocol (opt-in `fuzzball` feature).
//!
//! [Fuzzball](https://github.com/WeirdBrains/fuzzball/blob/main/docs/keyboard.md) is a
//! small family of terminal extensions first implemented in the
//! [Mouser](https://github.com/WeirdBrains/mouser) terminal. Its keyboard subsystem has
//! exactly one mode: while it is on, every key press, repeat and release is reported as
//! one DCS message
//!
//! ```text
//! ESC P = F z k e ; t=<p|r|u>,code=<phys>,key=<logical>[,mods=<n>][,locks=<n>][,text=<hex>][,ime=<phase>] ESC \
//! ```
//!
//! Framing and field parsing come from the [`fuzzball`](https://docs.rs/fuzzball) crate
//! (re-exported as [`protocol`]); this module maps its events onto crossterm's types.
//!
//! With this feature enabled, crossterm's input parser decodes these messages into
//! ordinary [`Event::Key`](crate::event::Event::Key) events, so applications keep using
//! [`read`](crate::event::read) / [`poll`](crate::event::poll) unchanged. A committed IME
//! composition of more than one character is delivered as
//! [`Event::Paste`](crate::event::Event::Paste) when the `bracketed-paste` feature is on.
//! IME pre-edit messages (`start`, `update`, `cancel`) are consumed and not reported.
//!
//! The extra fields the protocol carries (physical key, logical key name, AltGr, lock
//! state, inserted text, IME phase) are available through [`FuzzballKeyEvent`] (the
//! crate's [`protocol::keyboard::KeyEvent`]) and [`to_input`] for applications that read
//! raw input themselves.
//!
//! # Example
//!
//! ```no_run
//! use std::io::stdout;
//! use crossterm::{execute, event::fuzzball::{EnableFuzzballKeyboard, DisableFuzzballKeyboard}};
//!
//! # fn main() -> std::io::Result<()> {
//! #[cfg(unix)]
//! let supported = crossterm::terminal::supports_fuzzball_keyboard()?;
//! # #[cfg(not(unix))] let supported = false;
//! if supported {
//!     execute!(stdout(), EnableFuzzballKeyboard)?;
//!     // ... crossterm::event::read() now returns keys decoded from Fuzzball events ...
//!     execute!(stdout(), DisableFuzzballKeyboard)?;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Through tmux or GNU screen the enable / disable commands must be wrapped in the
//! multiplexer's passthrough by the application (see [`protocol::passthrough`]).

use std::fmt;

use crate::Command;
use crate::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MediaKeyCode, ModifierKeyCode,
};

/// The [`fuzzball`](https://docs.rs/fuzzball) protocol crate.
pub use ::fuzzball as protocol;
pub use ::fuzzball::keyboard::{ImePhase, Key, KeyKind, Locks, Modifiers};

/// One Fuzzball key event with every field the protocol defines.
pub type FuzzballKeyEvent = ::fuzzball::keyboard::KeyEvent;

/// Longest Fuzzball message the input parser buffers before giving up.
pub(crate) const MAX_MESSAGE_LEN: usize = 4096;

/// A command that turns on the Fuzzball keyboard mode (structured key events) for the
/// current screen: `ESC P = F z k s ; on=1,v=0 ESC \`.
///
/// The main and alternate screens keep their own state, and leaving the alternate screen
/// or a terminal reset also turns the mode off. Use [`DisableFuzzballKeyboard`] to turn it
/// off explicitly (for example before exiting, and in a panic hook).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnableFuzzballKeyboard;

impl Command for EnableFuzzballKeyboard {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(::fuzzball::keyboard::ENABLE_STR)
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "The Fuzzball keyboard protocol is not available through the legacy Windows API.",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false
    }
}

/// A command that turns off the Fuzzball keyboard mode: `ESC P = F z k s ; on=0 ESC \`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisableFuzzballKeyboard;

impl Command for DisableFuzzballKeyboard {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(::fuzzball::keyboard::DISABLE_STR)
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "The Fuzzball keyboard protocol is not available through the legacy Windows API.",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false
    }
}

/// What a Fuzzball key event means for a crossterm application.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FuzzballInput {
    /// A key event.
    Key(KeyEvent),
    /// Committed composed text of more than one character.
    Text(String),
    /// Nothing to report (IME pre-edit, unknown key).
    None,
}

/// crossterm modifiers for a Fuzzball modifier mask (AltGr has no crossterm equivalent
/// and is dropped; Mouser reports the Windows / Command key as Super).
pub fn modifiers(mods: Modifiers) -> KeyModifiers {
    let mut m = KeyModifiers::empty();
    for (bit, flag) in [
        (Modifiers::SHIFT, KeyModifiers::SHIFT),
        (Modifiers::ALT, KeyModifiers::ALT),
        (Modifiers::CTRL, KeyModifiers::CONTROL),
        (Modifiers::SUPER, KeyModifiers::SUPER),
        (Modifiers::HYPER, KeyModifiers::HYPER),
        (Modifiers::META, KeyModifiers::META),
    ] {
        if mods.contains(bit) {
            m |= flag;
        }
    }
    m
}

/// crossterm key state for an event's locks and keypad keys.
pub fn state(ev: &FuzzballKeyEvent) -> KeyEventState {
    let mut s = KeyEventState::empty();
    if ev.locks.contains(Locks::CAPS_LOCK) {
        s |= KeyEventState::CAPS_LOCK;
    }
    if ev.locks.contains(Locks::NUM_LOCK) {
        s |= KeyEventState::NUM_LOCK;
    }
    if ev.code.as_deref().is_some_and(|c| c.starts_with("Numpad")) {
        s |= KeyEventState::KEYPAD;
    }
    s
}

fn kind(k: KeyKind) -> KeyEventKind {
    match k {
        KeyKind::Repeat => KeyEventKind::Repeat,
        KeyKind::Release => KeyEventKind::Release,
        _ => KeyEventKind::Press,
    }
}

/// Map a Fuzzball key event to what crossterm reports. Letters follow crossterm's usual
/// convention: `Shift+a` is `Char('A')` with `SHIFT`, `Ctrl+c` is `Char('c')` with
/// `CONTROL`, and `Shift+Tab` is `BackTab`.
pub fn to_input(ev: &FuzzballKeyEvent) -> FuzzballInput {
    let st = state(ev);
    match ev.ime {
        Some(ImePhase::Commit) => {
            let Some(text) = ev.text.as_deref().filter(|t| !t.is_empty()) else {
                return FuzzballInput::None;
            };
            let mut chars = text.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) => FuzzballInput::Key(KeyEvent::new_with_kind_and_state(
                    KeyCode::Char(c),
                    KeyModifiers::empty(),
                    KeyEventKind::Press,
                    st,
                )),
                _ => FuzzballInput::Text(text.to_string()),
            };
        }
        Some(_) => return FuzzballInput::None,
        None => {}
    }
    let mut mods = modifiers(ev.mods);
    let code_name = ev.code.as_deref().unwrap_or("");
    let code = match &ev.key {
        Some(Key::Char(c)) => {
            // Prefer the inserted character (it carries Shift / layout effects). Mouser
            // sends no text with Ctrl/Alt/Super held, so fall back to uppercasing.
            let typed = ev.text.as_deref().and_then(|t| {
                let mut it = t.chars();
                match (it.next(), it.next()) {
                    (Some(t), None) if !t.is_control() => Some(t),
                    _ => None,
                }
            });
            let c = typed.unwrap_or(if mods.contains(KeyModifiers::SHIFT) {
                let mut up = c.to_uppercase();
                match (up.next(), up.next()) {
                    (Some(u), None) => u,
                    _ => *c,
                }
            } else {
                *c
            });
            KeyCode::Char(c)
        }
        Some(Key::Named(n)) => match named_key(n, code_name) {
            Some(KeyCode::Tab) if mods.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
            Some(k) => k,
            None => return FuzzballInput::None,
        },
        _ => return FuzzballInput::None,
    };
    if let KeyCode::Modifier(m) = code {
        // `mods` reflects the state after the event; crossterm (like kitty) reports a
        // modifier key's own bit as held while it is pressed.
        let own = match m {
            ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => KeyModifiers::SHIFT,
            ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => KeyModifiers::ALT,
            ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => KeyModifiers::CONTROL,
            ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper => KeyModifiers::SUPER,
            ModifierKeyCode::LeftHyper | ModifierKeyCode::RightHyper => KeyModifiers::HYPER,
            ModifierKeyCode::LeftMeta | ModifierKeyCode::RightMeta => KeyModifiers::META,
            _ => KeyModifiers::empty(),
        };
        if ev.kind != KeyKind::Release {
            mods |= own;
        }
    }
    FuzzballInput::Key(KeyEvent::new_with_kind_and_state(
        code,
        mods,
        kind(ev.kind),
        st,
    ))
}

fn named_key(key: &str, code: &str) -> Option<KeyCode> {
    let right = code.ends_with("Right");
    let side =
        |l: ModifierKeyCode, r: ModifierKeyCode| KeyCode::Modifier(if right { r } else { l });
    Some(match key {
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Esc,
        "Backspace" => KeyCode::Backspace,
        "Tab" => KeyCode::Tab,
        "Delete" => KeyCode::Delete,
        "Insert" => KeyCode::Insert,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "CapsLock" => KeyCode::CapsLock,
        "ScrollLock" => KeyCode::ScrollLock,
        "NumLock" => KeyCode::NumLock,
        "PrintScreen" => KeyCode::PrintScreen,
        "Pause" => KeyCode::Pause,
        "ContextMenu" => KeyCode::Menu,
        "Clear" => KeyCode::KeypadBegin,
        "Shift" => side(ModifierKeyCode::LeftShift, ModifierKeyCode::RightShift),
        "Control" => side(ModifierKeyCode::LeftControl, ModifierKeyCode::RightControl),
        "Alt" => side(ModifierKeyCode::LeftAlt, ModifierKeyCode::RightAlt),
        "Meta" => side(ModifierKeyCode::LeftMeta, ModifierKeyCode::RightMeta),
        "Super" | "OS" => side(ModifierKeyCode::LeftSuper, ModifierKeyCode::RightSuper),
        "Hyper" => side(ModifierKeyCode::LeftHyper, ModifierKeyCode::RightHyper),
        "AltGraph" => KeyCode::Modifier(ModifierKeyCode::IsoLevel3Shift),
        "MediaPlay" => KeyCode::Media(MediaKeyCode::Play),
        "MediaPause" => KeyCode::Media(MediaKeyCode::Pause),
        "MediaPlayPause" => KeyCode::Media(MediaKeyCode::PlayPause),
        "MediaStop" => KeyCode::Media(MediaKeyCode::Stop),
        "MediaFastForward" => KeyCode::Media(MediaKeyCode::FastForward),
        "MediaRewind" => KeyCode::Media(MediaKeyCode::Rewind),
        "MediaTrackNext" => KeyCode::Media(MediaKeyCode::TrackNext),
        "MediaTrackPrevious" => KeyCode::Media(MediaKeyCode::TrackPrevious),
        "MediaRecord" => KeyCode::Media(MediaKeyCode::Record),
        "AudioVolumeDown" => KeyCode::Media(MediaKeyCode::LowerVolume),
        "AudioVolumeUp" => KeyCode::Media(MediaKeyCode::RaiseVolume),
        "AudioVolumeMute" => KeyCode::Media(MediaKeyCode::MuteVolume),
        f if f.len() > 1 && f.starts_with('F') => KeyCode::F(f[1..].parse().ok()?),
        _ => return None,
    })
}

/// What the input parser does with a buffer that starts with `ESC P = F z`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Parsed {
    /// Not complete yet (or a prefix of `ESC P = F z`).
    Incomplete,
    /// Not a Fuzzball message.
    NotFuzzball,
    /// A keyboard event.
    Input(FuzzballInput),
    /// A keyboard-mode reply (`k r`): whether the mode is on.
    ModeReply(bool),
    /// A capability reply (`c r`): whether the keyboard subsystem is available.
    Capabilities(bool),
    /// Some other / malformed Fuzzball message: drop it.
    Ignored,
}

/// Parse a buffer that may start with `ESC P = F z`.
pub(crate) fn parse_dcs(buffer: &[u8]) -> Parsed {
    use ::fuzzball::{Parse, common::Capabilities, keyboard::ModeReply};
    match ::fuzzball::parse_with_limit(buffer, MAX_MESSAGE_LEN) {
        Parse::Incomplete => Parsed::Incomplete,
        Parse::NotFuzzball => Parsed::NotFuzzball,
        Parse::Complete { value: m, .. } => {
            if let Some(ev) = FuzzballKeyEvent::from_message(&m) {
                Parsed::Input(to_input(&ev))
            } else if let Some(r) = ModeReply::from_message(&m) {
                Parsed::ModeReply(r.on)
            } else if let Some(c) = Capabilities::from_message(&m) {
                Parsed::Capabilities(c.keyboard())
            } else {
                Parsed::Ignored
            }
        }
        _ => Parsed::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(msg: &str) -> KeyEvent {
        match parse_dcs(format!("\x1BP=Fz{msg}\x1B\\").as_bytes()) {
            Parsed::Input(FuzzballInput::Key(k)) => k,
            other => panic!("{msg}: {other:?}"),
        }
    }

    #[test]
    fn spec_examples() {
        let k = key("ke;t=p,code=KeyA,key=U+0061,text=61");
        assert_eq!(
            (k.code, k.modifiers, k.kind),
            (KeyCode::Char('a'), KeyModifiers::NONE, KeyEventKind::Press)
        );
        let k = key("ke;t=p,code=KeyA,key=U+0061,mods=1,text=41");
        assert_eq!(
            (k.code, k.modifiers),
            (KeyCode::Char('A'), KeyModifiers::SHIFT)
        );
        let k = key("ke;t=u,code=KeyA,key=U+0061");
        assert_eq!(
            (k.code, k.kind),
            (KeyCode::Char('a'), KeyEventKind::Release)
        );
        let k = key("ke;t=p,code=KeyC,key=U+0063,mods=4");
        assert_eq!(
            (k.code, k.modifiers),
            (KeyCode::Char('c'), KeyModifiers::CONTROL)
        );
        let k = key("ke;t=p,code=KeyA,key=U+0444,mods=4");
        assert_eq!(
            (k.code, k.modifiers),
            (KeyCode::Char('ф'), KeyModifiers::CONTROL)
        );
        let k = key("ke;t=p,code=Enter,key=Enter,text=0d");
        assert_eq!(k.code, KeyCode::Enter);
        let k = key("ke;t=r,code=ArrowUp,key=ArrowUp");
        assert_eq!((k.code, k.kind), (KeyCode::Up, KeyEventKind::Repeat));
        let k = key("ke;t=p,code=ShiftLeft,key=Shift,mods=1");
        assert_eq!(
            (k.code, k.modifiers),
            (
                KeyCode::Modifier(ModifierKeyCode::LeftShift),
                KeyModifiers::SHIFT
            )
        );
    }

    #[test]
    fn mouser_wire_format() {
        // Space is U+0020; uppercase hex keys; Super for Win/Cmd; no text with Ctrl.
        assert_eq!(
            key("ke;t=p,code=Space,key=U+0020,text=20").code,
            KeyCode::Char(' ')
        );
        assert_eq!(
            key("ke;t=p,code=Digit1,key=U+0031,text=31").code,
            KeyCode::Char('1')
        );
        let k = key("ke;t=p,code=KeyE,key=U+00E9,text=c3a9");
        assert_eq!(k.code, KeyCode::Char('é'));
        let k = key("ke;t=p,code=MetaLeft,key=Super,mods=8");
        assert_eq!(
            (k.code, k.modifiers),
            (
                KeyCode::Modifier(ModifierKeyCode::LeftSuper),
                KeyModifiers::SUPER
            )
        );
        let k = key("ke;t=p,code=KeyA,key=U+0061,mods=5");
        assert_eq!(
            (k.code, k.modifiers),
            (
                KeyCode::Char('A'),
                KeyModifiers::SHIFT | KeyModifiers::CONTROL
            )
        );
    }

    #[test]
    fn modifiers_locks_and_keypad() {
        let k = key("ke;t=p,code=Numpad7,key=U+0037,mods=58,locks=3,text=37");
        assert_eq!(k.code, KeyCode::Char('7'));
        assert_eq!(
            k.modifiers,
            KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META
        );
        assert_eq!(
            k.state,
            KeyEventState::KEYPAD | KeyEventState::CAPS_LOCK | KeyEventState::NUM_LOCK
        );
        assert_eq!(key("ke;t=p,code=Tab,key=Tab,mods=1").code, KeyCode::BackTab);
        assert_eq!(key("ke;t=p,code=F13,key=F13").code, KeyCode::F(13));
        assert_eq!(
            key("ke;t=p,code=MediaPlayPause,key=MediaPlayPause").code,
            KeyCode::Media(MediaKeyCode::PlayPause)
        );
        assert_eq!(
            key("ke;t=p,code=ControlRight,key=Control,mods=4").code,
            KeyCode::Modifier(ModifierKeyCode::RightControl)
        );
        assert_eq!(key("ke;t=p,code=Escape,key=Escape").code, KeyCode::Esc);
        assert_eq!(
            key("ke;t=p,code=KeyA,key=U+0061,future=9").code,
            KeyCode::Char('a')
        );
    }

    #[test]
    fn full_fields() {
        let e =
            FuzzballKeyEvent::parse(b"\x1BP=Fzke;t=p,code=KeyQ,key=U+0071,mods=64,text=40\x1B\\")
                .unwrap();
        assert_eq!(e.code.as_deref(), Some("KeyQ"));
        assert!(e.mods.contains(Modifiers::ALT_GR));
        assert_eq!(e.text.as_deref(), Some("@"));
        assert_eq!(e.ime, None);
    }

    #[test]
    fn ime() {
        let p = |m: &str| parse_dcs(format!("\x1BP=Fz{m}\x1B\\").as_bytes());
        assert_eq!(p("ke;t=p,ime=start"), Parsed::Input(FuzzballInput::None));
        assert_eq!(
            p("ke;t=p,ime=update,text=e3818b"),
            Parsed::Input(FuzzballInput::None)
        );
        assert_eq!(
            p("ke;t=p,ime=commit,text=e6bca2e5ad97"),
            Parsed::Input(FuzzballInput::Text("漢字".into()))
        );
        assert_eq!(key("ke;t=p,ime=commit,text=c3a9").code, KeyCode::Char('é'));
    }

    #[test]
    fn framing_replies_and_garbage() {
        assert_eq!(parse_dcs(b"\x1BP=Fzke;t=p,code=KeyA"), Parsed::Incomplete);
        assert_eq!(parse_dcs(b"\x1BP="), Parsed::Incomplete);
        assert_eq!(parse_dcs(b"\x1BPx"), Parsed::NotFuzzball);
        assert_eq!(
            parse_dcs(b"\x1BP=Fzkr;id=1,ok=1,on=1\x1B\\"),
            Parsed::ModeReply(true)
        );
        assert_eq!(
            parse_dcs(b"\x1BP=Fzcr;id=1,v=0,sub=gk,g_fmt=rgba:rgb:png,k=1,k_ime=0\x1B\\"),
            Parsed::Capabilities(true)
        );
        assert_eq!(
            parse_dcs(b"\x1BP=Fzcr;id=1,v=0,sub=g\x1B\\"),
            Parsed::Capabilities(false)
        );
        assert_eq!(parse_dcs(b"\x1BP=Fzgr;id=2\x1B\\"), Parsed::Ignored);
        assert_eq!(
            parse_dcs(b"\x1BP=Fzke;t=x,key=Enter\x1B\\"),
            Parsed::Ignored
        );
        assert_eq!(
            parse_dcs(b"\x1BP=Fzke;t=p,key=U+0061,text=6\x1B\\"),
            Parsed::Ignored
        );
        let mut long = b"\x1BP=Fzke;".to_vec();
        long.resize(MAX_MESSAGE_LEN + 1, b'a');
        assert_eq!(parse_dcs(&long), Parsed::Ignored);
    }

    #[test]
    fn commands() {
        let mut s = String::new();
        EnableFuzzballKeyboard.write_ansi(&mut s).unwrap();
        assert_eq!(s, "\x1BP=Fzks;on=1,v=0\x1B\\");
        s.clear();
        DisableFuzzballKeyboard.write_ansi(&mut s).unwrap();
        assert_eq!(s, "\x1BP=Fzks;on=0\x1B\\");
    }
}
