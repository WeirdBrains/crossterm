//! Fuzzball keyboard protocol (opt-in `fuzzball` feature).
//!
//! [Fuzzball](https://github.com/WeirdBrains/mouser/blob/main/docs/protocol/keyboard.md)
//! is a small family of terminal extensions first implemented in the Mouser terminal. Its
//! keyboard subsystem has exactly one mode: while it is on, every key press, repeat and
//! release is reported as one DCS message
//!
//! ```text
//! ESC P = F z k e ; t=<p|r|u>,code=<phys>,key=<logical>[,mods=<n>][,locks=<n>][,text=<hex>][,ime=<phase>] ESC \
//! ```
//!
//! With this feature enabled, crossterm's input parser decodes these messages into
//! ordinary [`Event::Key`](crate::event::Event::Key) events, so applications keep using
//! [`read`](crate::event::read) / [`poll`](crate::event::poll) unchanged. A committed IME
//! composition of more than one character is delivered as
//! [`Event::Paste`](crate::event::Event::Paste) when the `bracketed-paste` feature is on.
//! IME pre-edit messages (`start`, `update`, `cancel`) are consumed and not reported.
//!
//! The extra fields the protocol carries (physical key, logical key name, AltGr, Hyper,
//! lock state, inserted text, IME phase) are available through [`FuzzballKeyEvent`] for
//! applications that read raw input themselves.
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
//! multiplexer's passthrough by the application; see the protocol README.

use std::fmt;

use crate::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MediaKeyCode, ModifierKeyCode,
};
use crate::Command;

/// Prefix of every Fuzzball message: `ESC P = F z`.
pub(crate) const DCS_PREFIX: &[u8] = b"\x1BP=Fz";
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
        f.write_str("\x1BP=Fzks;on=1,v=0\x1B\\")
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
        f.write_str("\x1BP=Fzks;on=0\x1B\\")
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

/// Fuzzball modifier bits (`mods=`).
pub mod mods {
    /// Shift.
    pub const SHIFT: u8 = 1;
    /// Alt.
    pub const ALT: u8 = 2;
    /// Control.
    pub const CONTROL: u8 = 4;
    /// Super.
    pub const SUPER: u8 = 8;
    /// Hyper.
    pub const HYPER: u8 = 16;
    /// Meta.
    pub const META: u8 = 32;
    /// AltGr.
    pub const ALT_GR: u8 = 64;
}

/// Fuzzball lock bits (`locks=`).
pub mod locks {
    /// Caps Lock.
    pub const CAPS_LOCK: u8 = 1;
    /// Num Lock.
    pub const NUM_LOCK: u8 = 2;
    /// Scroll Lock.
    pub const SCROLL_LOCK: u8 = 4;
}

/// IME composition phase (`ime=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImePhase {
    /// Composition started.
    Start,
    /// Pre-edit text changed (not inserted).
    Update,
    /// Composition finished; `text` is inserted.
    Commit,
    /// Composition cancelled.
    Cancel,
}

/// One Fuzzball key event (`ESC P = F z k e ; … ESC \`) with every field the protocol
/// defines.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuzzballKeyEvent {
    /// Press, repeat or release (`t=`).
    pub kind: KeyEventKind,
    /// Physical key as a W3C UI Events `code` name, e.g. `KeyA` (`code=`). Empty for IME
    /// messages.
    pub code: String,
    /// Logical key: a W3C UI Events `key` name, or `U+XXXX` (`key=`). Empty for IME
    /// messages.
    pub key: String,
    /// Held modifiers, see [`mods`] (`mods=`).
    pub mods: u8,
    /// Active locks, see [`locks`] (`locks=`).
    pub locks: u8,
    /// Text this event inserts, decoded from hex (`text=`).
    pub text: Option<String>,
    /// IME composition phase (`ime=`).
    pub ime: Option<ImePhase>,
}

/// What a Fuzzball key event means for a crossterm application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FuzzballInput {
    /// A key event.
    Key(KeyEvent),
    /// Committed composed text of more than one character.
    Text(String),
    /// Nothing to report (IME pre-edit, unknown key).
    None,
}

impl FuzzballKeyEvent {
    /// Parse a complete message (`ESC P = F z k e ; … ESC \`, with or without the framing).
    /// Unknown parameters are ignored, as the spec requires.
    pub fn parse(message: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(message).ok()?;
        let s = s.strip_prefix("\x1BP=Fz").unwrap_or(s);
        let s = s.strip_suffix("\x1B\\").unwrap_or(s);
        let params = s.strip_prefix("ke;").or_else(|| s.strip_prefix("ke"))?;
        let mut ev = FuzzballKeyEvent {
            kind: KeyEventKind::Press,
            code: String::new(),
            key: String::new(),
            mods: 0,
            locks: 0,
            text: None,
            ime: None,
        };
        let mut have_t = false;
        for pair in params.split(',').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=')?;
            match k {
                "t" => {
                    have_t = true;
                    ev.kind = match v {
                        "p" => KeyEventKind::Press,
                        "r" => KeyEventKind::Repeat,
                        "u" => KeyEventKind::Release,
                        _ => return None,
                    }
                }
                "code" => ev.code = v.to_string(),
                "key" => ev.key = v.to_string(),
                "mods" => ev.mods = v.parse().ok()?,
                "locks" => ev.locks = v.parse().ok()?,
                "text" => ev.text = Some(decode_hex_utf8(v)?),
                "ime" => {
                    ev.ime = Some(match v {
                        "start" => ImePhase::Start,
                        "update" => ImePhase::Update,
                        "commit" => ImePhase::Commit,
                        "cancel" => ImePhase::Cancel,
                        _ => return None,
                    })
                }
                _ => {}
            }
        }
        if !have_t || (ev.ime.is_none() && ev.key.is_empty()) {
            return None;
        }
        Some(ev)
    }

    /// crossterm modifiers for `mods` (AltGr has no crossterm equivalent and is dropped).
    pub fn modifiers(&self) -> KeyModifiers {
        let mut m = KeyModifiers::empty();
        for (bit, flag) in [
            (mods::SHIFT, KeyModifiers::SHIFT),
            (mods::ALT, KeyModifiers::ALT),
            (mods::CONTROL, KeyModifiers::CONTROL),
            (mods::SUPER, KeyModifiers::SUPER),
            (mods::HYPER, KeyModifiers::HYPER),
            (mods::META, KeyModifiers::META),
        ] {
            if self.mods & bit != 0 {
                m |= flag;
            }
        }
        m
    }

    /// crossterm key state for `locks` and keypad keys.
    pub fn state(&self) -> KeyEventState {
        let mut s = KeyEventState::empty();
        if self.locks & locks::CAPS_LOCK != 0 {
            s |= KeyEventState::CAPS_LOCK;
        }
        if self.locks & locks::NUM_LOCK != 0 {
            s |= KeyEventState::NUM_LOCK;
        }
        if self.code.starts_with("Numpad") {
            s |= KeyEventState::KEYPAD;
        }
        s
    }

    /// Map to what crossterm reports. Letters follow crossterm's usual convention:
    /// `Shift+a` is `Char('A')` with `SHIFT`, `Ctrl+c` is `Char('c')` with `CONTROL`, and
    /// `Shift+Tab` is `BackTab`.
    pub fn to_input(&self) -> FuzzballInput {
        match self.ime {
            Some(ImePhase::Commit) => {
                let Some(text) = self.text.as_deref().filter(|t| !t.is_empty()) else {
                    return FuzzballInput::None;
                };
                let mut chars = text.chars();
                return match (chars.next(), chars.next()) {
                    (Some(c), None) => FuzzballInput::Key(KeyEvent::new_with_kind_and_state(
                        KeyCode::Char(c),
                        KeyModifiers::empty(),
                        KeyEventKind::Press,
                        self.state(),
                    )),
                    _ => FuzzballInput::Text(text.to_string()),
                };
            }
            Some(_) => return FuzzballInput::None,
            None => {}
        }
        let mut modifiers = self.modifiers();
        let code = match self.key.strip_prefix("U+") {
            Some(hex) => {
                let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) else {
                    return FuzzballInput::None;
                };
                // Prefer the inserted character (it carries Shift / layout effects).
                let typed = self.text.as_deref().and_then(|t| {
                    let mut it = t.chars();
                    match (it.next(), it.next()) {
                        (Some(t), None) if !t.is_control() => Some(t),
                        _ => None,
                    }
                });
                let c = typed.unwrap_or(if modifiers.contains(KeyModifiers::SHIFT) {
                    let mut up = c.to_uppercase();
                    match (up.next(), up.next()) {
                        (Some(u), None) => u,
                        _ => c,
                    }
                } else {
                    c
                });
                KeyCode::Char(c)
            }
            None => match named_key(&self.key, &self.code) {
                Some(KeyCode::Tab) if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
                Some(k) => k,
                None => return FuzzballInput::None,
            },
        };
        if let KeyCode::Modifier(m) = code {
            // Spec: `mods` reflects the state after the event; crossterm (like kitty)
            // reports a modifier key's own bit as held while it is pressed.
            let own = match m {
                ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => KeyModifiers::SHIFT,
                ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => KeyModifiers::ALT,
                ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => {
                    KeyModifiers::CONTROL
                }
                ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper => KeyModifiers::SUPER,
                ModifierKeyCode::LeftHyper | ModifierKeyCode::RightHyper => KeyModifiers::HYPER,
                ModifierKeyCode::LeftMeta | ModifierKeyCode::RightMeta => KeyModifiers::META,
                _ => KeyModifiers::empty(),
            };
            if self.kind != KeyEventKind::Release {
                modifiers |= own;
            }
        }
        FuzzballInput::Key(KeyEvent::new_with_kind_and_state(
            code,
            modifiers,
            self.kind,
            self.state(),
        ))
    }
}

fn decode_hex_utf8(v: &str) -> Option<String> {
    if v.len() % 2 != 0 {
        return None;
    }
    let bytes = (0..v.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(v.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
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

/// What the input parser does with a buffer that starts with [`DCS_PREFIX`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Parsed {
    /// Not complete yet.
    Incomplete,
    /// A keyboard event.
    Input(FuzzballInput),
    /// A keyboard-mode query reply (`k r`): whether the mode is on.
    ModeReply(bool),
    /// A capability reply (`c r`): whether the keyboard subsystem is available.
    Capabilities(bool),
    /// Some other / malformed Fuzzball message: drop it.
    Ignored,
}

/// Parse a buffer starting with `ESC P = F z`.
pub(crate) fn parse_dcs(buffer: &[u8]) -> Parsed {
    let Some(end) = buffer
        .windows(2)
        .position(|w| w == b"\x1B\\")
        .filter(|&i| i >= DCS_PREFIX.len())
    else {
        return if buffer.len() > MAX_MESSAGE_LEN {
            Parsed::Ignored
        } else {
            Parsed::Incomplete
        };
    };
    let body = &buffer[DCS_PREFIX.len()..end];
    let params = |b: &[u8]| -> Vec<(String, String)> {
        std::str::from_utf8(b.get(3..).unwrap_or_default())
            .unwrap_or_default()
            .split(',')
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let has = |ps: &[(String, String)], k: &str, v: &str| ps.iter().any(|(a, b)| a == k && b == v);
    match body.get(..2) {
        Some(b"ke") => FuzzballKeyEvent::parse(body)
            .map(|e| Parsed::Input(e.to_input()))
            .unwrap_or(Parsed::Ignored),
        Some(b"kr") => Parsed::ModeReply(has(&params(body), "on", "1")),
        Some(b"cr") => {
            let ps = params(body);
            let sub_k = ps.iter().any(|(k, v)| k == "sub" && v.contains('k'));
            Parsed::Capabilities(sub_k && has(&ps, "k", "1"))
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
        // Unknown params are ignored.
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
        assert_eq!(e.code, "KeyQ");
        assert_eq!(e.mods & mods::ALT_GR, mods::ALT_GR);
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
        assert_eq!(
            parse_dcs(b"\x1BP=Fzkr;id=1,ok=1,on=1\x1B\\"),
            Parsed::ModeReply(true)
        );
        assert_eq!(
            parse_dcs(b"\x1BP=Fzcr;id=1,v=0,sub=gk,k=1,k_ime=0\x1B\\"),
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
