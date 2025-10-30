use std::fmt::{self, Display, Write};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, de};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub keybinds: Keybinds,
}

#[derive(Debug, Deserialize)]
pub struct Keybinds {
    pub quit: Keybind,
    pub save: Keybind,
    pub left: Keybind,
    pub down: Keybind,
    pub up: Keybind,
    pub right: Keybind,
    pub toggle_cursor: Keybind,
    pub go_to: Keybind,
    pub find: Keybind,
    pub find_binary: Keybind,
    pub find_text: Keybind,
}

#[derive(Debug, Clone, Copy)]
pub struct Keybind {
    pub control: bool,
    pub key: char,
}

impl Keybind {
    pub fn matches(self, event: KeyEvent) -> bool {
        let KeyCode::Char(c) = event.code else {
            return false;
        };
        
        let control = event.modifiers.contains(KeyModifiers::CONTROL);
        
        let char_matches = self.key.to_ascii_lowercase() == c || self.key.to_ascii_uppercase() == c;
        self.control == control && char_matches
    }
}

impl Display for Keybind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.control {
            write!(f, "^")?;
        }
        
        f.write_char(self.key)
    }
}

impl<'de> Deserialize<'de> for Keybind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>
    {
        let string = String::deserialize(deserializer)?;
        
        let mut control = false;
        let mut key = None;
        for c in string.chars() {
            if key.is_some() {
                return Err(de::Error::invalid_value(de::Unexpected::Str(&string), &"a valid keybind definition"));
            }
            
            if c == '^' {
                control = true;
            } else {
                key = Some(c);
            }
        }
        
        let Some(key) = key else {
            return Err(de::Error::invalid_value(de::Unexpected::Str(&string), &"a valid keybind definition"));
        };
        
        Ok(Self {
            control,
            key,
        })
    }
}
