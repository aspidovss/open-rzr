//! Macro documents: what the macro editor works with, and the compiler that turns them into
//! on-board macro steps.
//!
//! A document has two parts:
//! * `items` — **hardware steps**, compiled into the mouse: delays, keys, mouse buttons, wheel,
//!   media keys, loops (unrolled), other macros (inlined) and ASCII text (typed as keystrokes,
//!   US layout). They work on any computer, without open-rzr running.
//! * `software` — **program actions**: launch a program or a website, run a command, type any
//!   text (Unicode). The mouse cannot do these; when present, the compiled macro ends with a press
//!   of a free F13–F24 key (`trigger`), and open-rzr, while running, performs the actions when it
//!   sees that key.

use crate::hidkeys;
use crate::macros::{self, MacroStep};

/// One step of the editor.
///
/// | kind            | value                    | extra                | text        |
/// |-----------------|--------------------------|----------------------|-------------|
/// | `delay`         | ms                       |                      |             |
/// | `key_press`     | HID key code             | hold ms (0 = 20)     |             |
/// | `key_down`/`up` | HID key code             |                      |             |
/// | `mouse_click`   | button bit (1,2,4,8,16)  | hold ms (0 = 20)     |             |
/// | `mouse_down`/`up` | button bit             |                      |             |
/// | `mouse`         | raw button bitmask       |                      |             |
/// | `wheel`         | +up / -down              |                      |             |
/// | `consumer`      | raw usage (0 = release)  |                      |             |
/// | `media`         | consumer usage (pressed and released) |         |             |
/// | `loop_start`    |                          | repeat count         |             |
/// | `loop_end`      |                          |                      |             |
/// | `macro`         |                          |                      | macro name  |
/// | `text`          |                          | ms between keys      | ASCII text  |
///
/// `on: false` keeps the step in the document but leaves it out of the macro.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(default))]
pub struct Item {
    pub kind: String,
    pub value: i64,
    pub extra: i64,
    pub text: String,
    pub on: bool,
}

impl Item {
    pub fn new(kind: &str, value: i64) -> Item {
        Item { kind: kind.into(), value, on: true, ..Default::default() }
    }
    pub fn with_extra(mut self, extra: i64) -> Item {
        self.extra = extra;
        self
    }
    pub fn with_text(mut self, text: &str) -> Item {
        self.text = text.into();
        self
    }
    pub fn from_step(s: &MacroStep) -> Item {
        Item::new(&s.kind, s.value)
    }
}

/// Program action. kind: `launch` (text = program, file or web address), `run` (text = command
/// line), `type` (text = any text), `delay` (value = ms).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(default))]
pub struct SoftAction {
    pub kind: String,
    pub value: i64,
    pub text: String,
    pub on: bool,
}

impl SoftAction {
    pub fn new(kind: &str, text: &str) -> SoftAction {
        SoftAction { kind: kind.into(), text: text.into(), on: true, value: 0 }
    }

    pub fn describe(&self) -> String {
        match self.kind.as_str() {
            "launch" => format!("Launch: {}", self.text),
            "run" => format!("Run: {}", self.text),
            "type" => format!("Type: \"{}\"", self.text),
            "delay" => format!("Delay {} ms", self.value),
            k => format!("{k}: {}", self.text),
        }
    }
}

/// Trigger keys for program actions: F13..F24 (HID codes 0x68..0x73).
pub const TRIGGER_KEYS: std::ops::RangeInclusive<u8> = 0x68..=0x73;

pub const MAX_LOOP: i64 = 1000;
pub const MAX_STEPS: usize = 20_000;
/// Limit for one macro's encoded size (the mouse stores macros in a few KB of flash).
pub const MAX_BYTES: usize = 4096;
const DEFAULT_HOLD: i64 = 20;
const MAX_DEPTH: usize = 8;

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Compiled {
    pub steps: Vec<MacroStep>,
    /// encoded size in bytes
    pub size: usize,
    /// total time in ms (sum of delays)
    pub ms: i64,
    /// the trigger key was appended
    pub trigger: Option<u8>,
}

/// US layout: character -> (needs Shift, HID key code).
pub fn ascii_key(c: char) -> Option<(bool, u8)> {
    Some(match c {
        'a'..='z' => (false, 0x04 + (c as u8 - b'a')),
        'A'..='Z' => (true, 0x04 + (c as u8 - b'A')),
        '1'..='9' => (false, 0x1E + (c as u8 - b'1')),
        '0' => (false, 0x27),
        '\n' => (false, 0x28),
        '\t' => (false, 0x2B),
        ' ' => (false, 0x2C),
        '!' => (true, 0x1E),
        '@' => (true, 0x1F),
        '#' => (true, 0x20),
        '$' => (true, 0x21),
        '%' => (true, 0x22),
        '^' => (true, 0x23),
        '&' => (true, 0x24),
        '*' => (true, 0x25),
        '(' => (true, 0x26),
        ')' => (true, 0x27),
        '-' => (false, 0x2D),
        '_' => (true, 0x2D),
        '=' => (false, 0x2E),
        '+' => (true, 0x2E),
        '[' => (false, 0x2F),
        '{' => (true, 0x2F),
        ']' => (false, 0x30),
        '}' => (true, 0x30),
        '\\' => (false, 0x31),
        '|' => (true, 0x31),
        ';' => (false, 0x33),
        ':' => (true, 0x33),
        '\'' => (false, 0x34),
        '"' => (true, 0x34),
        '`' => (false, 0x35),
        '~' => (true, 0x35),
        ',' => (false, 0x36),
        '<' => (true, 0x36),
        '.' => (false, 0x37),
        '>' => (true, 0x37),
        '/' => (false, 0x38),
        '?' => (true, 0x38),
        _ => return None,
    })
}

/// Characters of `text` the mouse cannot type (not in the US layout table).
pub fn untypable(text: &str) -> Vec<char> {
    let mut v: Vec<char> = text.chars().filter(|c| *c != '\r' && ascii_key(*c).is_none()).collect();
    v.dedup();
    v
}

pub fn describe(it: &Item) -> String {
    let key = |v: i64| hidkeys::key_name(v as u8);
    let btn = |v: i64| {
        macros::MOUSE_BITS.iter().find(|(b, _)| *b as i64 == v).map(|(_, n)| n.to_string()).unwrap_or(format!("0x{v:02x}"))
    };
    match it.kind.as_str() {
        "key_press" => format!("Key {}", key(it.value)),
        "mouse_click" => format!("Click {}", btn(it.value)),
        "mouse_down" => format!("↓ Mouse {}", btn(it.value)),
        "mouse_up" => format!("↑ Mouse {}", btn(it.value)),
        "media" => format!("Media: {}", hidkeys::consumer_name(it.value as u16)),
        "loop_start" => format!("Loop ×{}", it.extra),
        "loop_end" => "End of loop".into(),
        "macro" => format!("Macro \"{}\"", it.text),
        "text" => format!("Text \"{}\"", it.text),
        _ => MacroStep::new(&it.kind, it.value).describe(),
    }
}

/// Errors carry a code and one parameter for translation: "[code|param] English message".
fn err(code: &str, param: impl std::fmt::Display, msg: impl std::fmt::Display) -> String {
    format!("[{code}|{param}] {msg}")
}

struct Ctx<'a> {
    lookup: &'a dyn Fn(&str) -> Option<Vec<Item>>,
    mouse: i64,
    out: Vec<MacroStep>,
    stack: Vec<String>,
}

impl Ctx<'_> {
    fn push(&mut self, kind: &str, value: i64) -> Result<(), String> {
        if self.out.len() >= MAX_STEPS {
            return Err(err("too_long", MAX_STEPS, format!("the macro is too long (over {MAX_STEPS} steps)")));
        }
        self.out.push(MacroStep::new(kind, value));
        Ok(())
    }

    fn hold(&mut self, ms: i64) -> Result<(), String> {
        self.push("delay", if ms > 0 { ms } else { DEFAULT_HOLD })
    }

    fn block(&mut self, items: &[Item]) -> Result<(), String> {
        let mut i = 0;
        while i < items.len() {
            let it = &items[i];
            i += 1;
            if !it.on {
                if it.kind == "loop_start" {
                    // a switched-off loop: its body still runs once
                    continue;
                }
                if it.kind != "loop_end" {
                    continue;
                }
            }
            match it.kind.as_str() {
                "loop_start" => {
                    let end = matching_end(items, i - 1).ok_or_else(|| err("loop_open", "", "a loop has no end"))?;
                    let n = it.extra.clamp(1, MAX_LOOP);
                    for _ in 0..n {
                        self.block(&items[i..end])?;
                    }
                    i = end + 1;
                }
                "loop_end" => {}
                _ => self.one(it)?,
            }
        }
        Ok(())
    }

    fn one(&mut self, it: &Item) -> Result<(), String> {
        let v = it.value;
        match it.kind.as_str() {
            "delay" => {
                if v > 0 {
                    self.push("delay", v)?
                }
            }
            "key_down" | "key_up" | "wheel" | "consumer" | "system" => self.push(&it.kind, v)?,
            "key_press" => {
                self.push("key_down", v)?;
                self.hold(it.extra)?;
                self.push("key_up", v)?;
            }
            "mouse" => {
                self.mouse = v & 0x1F;
                self.push("mouse", self.mouse)?;
            }
            "mouse_down" => {
                self.mouse |= v & 0x1F;
                self.push("mouse", self.mouse)?;
            }
            "mouse_up" => {
                self.mouse &= !v;
                self.push("mouse", self.mouse)?;
            }
            "mouse_click" => {
                self.mouse |= v & 0x1F;
                self.push("mouse", self.mouse)?;
                self.hold(it.extra)?;
                self.mouse &= !v;
                self.push("mouse", self.mouse)?;
            }
            "media" => {
                self.push("consumer", v)?;
                self.hold(it.extra)?;
                self.push("consumer", 0)?;
            }
            "text" => {
                let bad = untypable(&it.text);
                if !bad.is_empty() {
                    let s: String = bad.iter().take(5).collect();
                    return Err(err(
                        "text",
                        &s,
                        format!(
                            "the mouse can type only Latin letters, digits and ASCII symbols (US layout); \
                             not \"{s}\" — use a \"Type text\" program action for other text"
                        ),
                    ));
                }
                let gap = it.extra.max(0);
                for c in it.text.chars().filter(|c| *c != '\r') {
                    let (shift, k) = ascii_key(c).unwrap();
                    if shift {
                        self.push("key_down", 0xE1)?;
                    }
                    self.push("key_down", k as i64)?;
                    self.push("delay", 10)?;
                    self.push("key_up", k as i64)?;
                    if shift {
                        self.push("key_up", 0xE1)?;
                    }
                    if gap > 0 {
                        self.push("delay", gap)?;
                    }
                }
            }
            "macro" => {
                let name = it.text.clone();
                if self.stack.contains(&name) {
                    return Err(err("self", &name, format!("macro \"{name}\" contains itself")));
                }
                if self.stack.len() >= MAX_DEPTH {
                    return Err(err("deep", MAX_DEPTH, "macros are nested too deep"));
                }
                let items = (self.lookup)(&name).ok_or_else(|| err("missing", &name, format!("macro \"{name}\" not found")))?;
                self.stack.push(name);
                self.block(&items)?;
                self.stack.pop();
            }
            k => return Err(err("unknown", k, format!("unknown step {k}"))),
        }
        Ok(())
    }
}

/// Index of the `loop_end` that closes the `loop_start` at `start` (switched-off loops count too).
fn matching_end(items: &[Item], start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, it) in items.iter().enumerate().skip(start) {
        match it.kind.as_str() {
            "loop_start" if it.on => depth += 1,
            "loop_end" if it.on => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Check that loops are balanced.
pub fn check_loops(items: &[Item]) -> Result<(), String> {
    let mut depth = 0i32;
    for it in items.iter().filter(|i| i.on) {
        match it.kind.as_str() {
            "loop_start" => depth += 1,
            "loop_end" => {
                depth -= 1;
                if depth < 0 {
                    return Err(err("loop_end", "", "\"End of loop\" without a loop"));
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(err("loop_open", "", "a loop has no end"));
    }
    Ok(())
}

/// Compile hardware items (and the trigger key, if program actions exist) into on-board steps.
/// `lookup` returns the items of another macro by name (for `macro` steps).
pub fn compile(
    name: &str,
    items: &[Item],
    trigger: Option<u8>,
    lookup: &dyn Fn(&str) -> Option<Vec<Item>>,
) -> Result<Compiled, String> {
    check_loops(items)?;
    let mut cx = Ctx { lookup, mouse: 0, out: Vec::new(), stack: vec![name.to_string()] };
    cx.block(items)?;
    // nothing must stay pressed after the macro
    if cx.mouse != 0 {
        cx.push("mouse", 0)?;
    }
    if let Some(k) = trigger {
        if !TRIGGER_KEYS.contains(&k) {
            return Err(err("trigger", k, format!("trigger key 0x{k:02x} is not F13–F24")));
        }
        cx.push("key_down", k as i64)?;
        cx.push("delay", DEFAULT_HOLD)?;
        cx.push("key_up", k as i64)?;
    }
    let steps = cx.out;
    let size = macros::encode(&steps)?.len();
    if size > MAX_BYTES {
        return Err(err("too_big", size, format!("the macro takes {size} bytes; the mouse holds at most {MAX_BYTES} — fewer loop repeats or less text")));
    }
    let ms = steps.iter().filter(|s| s.kind == "delay").map(|s| s.value).sum();
    Ok(Compiled { steps, size, ms, trigger })
}

/// A free trigger key, not in `used`.
pub fn free_trigger(used: &[u8]) -> Option<u8> {
    TRIGGER_KEYS.clone().find(|k| !used.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<Vec<Item>> {
        None
    }

    #[test]
    fn press_and_click() {
        let c = compile("m", &[Item::new("key_press", 0x04), Item::new("mouse_click", 2).with_extra(50)], None, &none).unwrap();
        let k: Vec<(&str, i64)> = c.steps.iter().map(|s| (s.kind.as_str(), s.value)).collect();
        assert_eq!(k, [("key_down", 4), ("delay", 20), ("key_up", 4), ("mouse", 2), ("delay", 50), ("mouse", 0)]);
        assert_eq!(c.ms, 70);
    }

    #[test]
    fn loops_unrolled_and_nested() {
        let items = [
            Item::new("loop_start", 0).with_extra(3),
            Item::new("key_press", 0x05),
            Item::new("loop_start", 0).with_extra(2),
            Item::new("wheel", 1),
            Item::new("loop_end", 0),
            Item::new("loop_end", 0),
        ];
        let c = compile("m", &items, None, &none).unwrap();
        assert_eq!(c.steps.iter().filter(|s| s.kind == "wheel").count(), 6);
        assert_eq!(c.steps.iter().filter(|s| s.kind == "key_down").count(), 3);
        assert!(compile("m", &items[..2], None, &none).is_err());
        assert!(compile("m", &items[4..], None, &none).is_err());
    }

    #[test]
    fn mouse_state_and_off_steps() {
        let mut off = Item::new("key_press", 6);
        off.on = false;
        let items = [Item::new("mouse_down", 1), Item::new("mouse_down", 2), off, Item::new("mouse_up", 1)];
        let c = compile("m", &items, None, &none).unwrap();
        let v: Vec<i64> = c.steps.iter().map(|s| s.value).collect();
        assert_eq!(v, [1, 3, 2, 0]); // released at the end
    }

    #[test]
    fn text_and_nested_macros() {
        let lookup = |n: &str| match n {
            "hi" => Some(vec![Item::new("text", 0).with_text("Hi!")]),
            "self" => Some(vec![Item::new("macro", 0).with_text("self")]),
            _ => None,
        };
        let c = compile("m", &[Item::new("macro", 0).with_text("hi")], None, &lookup).unwrap();
        let downs: Vec<i64> = c.steps.iter().filter(|s| s.kind == "key_down").map(|s| s.value).collect();
        assert_eq!(downs, [0xE1, 0x0B, 0x0C, 0xE1, 0x1E]);
        assert!(compile("m", &[Item::new("macro", 0).with_text("self")], None, &lookup).unwrap_err().contains("itself"));
        assert!(compile("m", &[Item::new("macro", 0).with_text("x")], None, &lookup).is_err());
        assert!(compile("m", &[Item::new("text", 0).with_text("Привет")], None, &none).is_err());
        assert_eq!(untypable("aПbП"), vec!['П']);
    }

    #[test]
    fn trigger_and_limits() {
        let c = compile("m", &[], Some(0x68), &none).unwrap();
        assert_eq!(c.steps.len(), 3);
        assert_eq!(c.trigger, Some(0x68));
        assert!(compile("m", &[], Some(0x04), &none).is_err());
        assert_eq!(free_trigger(&[0x68, 0x69]), Some(0x6A));
        let big = [Item::new("loop_start", 0).with_extra(1000), Item::new("key_press", 4), Item::new("loop_end", 0)];
        assert!(compile("m", &big, None, &none).unwrap_err().contains("bytes"));
    }
}
