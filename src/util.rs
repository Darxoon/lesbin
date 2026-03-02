use std::{fmt::{Arguments}, io::{Write, stdout}};

use anyhow::Result;
use crossterm::{QueueableCommand, cursor::MoveTo, queue, style::{Attribute, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineColor {
    Regular,
    Emphasis,
    Highlighted,
    TextCursor,
    Modified,
    Address,
    Zero,
}

impl LineColor {
    fn encode(self, buffer: &mut Vec<u8>) -> Result<()> {
        match self {
            LineColor::Regular => queue!(buffer, ResetColor),
            LineColor::Emphasis => queue!(
                buffer,
                ResetColor,
                SetForegroundColor(crossterm::style::Color::AnsiValue(39)),
                SetAttribute(Attribute::Bold),
            ),
            LineColor::Highlighted => queue!(
                buffer,
                ResetColor,
                SetForegroundColor(crossterm::style::Color::Black),
                SetBackgroundColor(crossterm::style::Color::Grey),
            ),
            LineColor::TextCursor => queue!(
                buffer,
                ResetColor,
                SetAttribute(Attribute::Reverse),
            ),
            LineColor::Modified => queue!(
                buffer,
                ResetColor,
                SetForegroundColor(crossterm::style::Color::AnsiValue(215)),
            ),
            LineColor::Address => queue!(
                buffer,
                ResetColor,
                SetForegroundColor(crossterm::style::Color::AnsiValue(206)),
            ),
            LineColor::Zero => queue!(
                buffer,
                ResetColor,
                SetForegroundColor(crossterm::style::Color::DarkGrey),
            ),
        }.map_err(Into::into)
    }
}

pub struct LineWriter {
    buffer: Vec<u8>,
    cur_color: Option<LineColor>,
    
    x: u16,
    y: u16,
}

impl LineWriter {
    pub fn new(x: u16, y: u16) -> Self {
        Self {
            buffer: Vec::new(),
            cur_color: None,
            x,
            y,
        }
    }
    
    pub fn write_str(&mut self, color: LineColor, content: &str) -> Result<()> {
        if self.cur_color.is_none_or(|cur_color| cur_color != color) {
            color.encode(&mut self.buffer)?;
            self.cur_color = Some(color);
        }
        
        self.buffer.extend_from_slice(content.as_bytes());
        Ok(())
    }
    
    pub fn write_char(&mut self, color: LineColor, content: char) -> Result<()> {
        if self.cur_color.is_none_or(|cur_color| cur_color != color) {
            color.encode(&mut self.buffer)?;
            self.cur_color = Some(color);
        }
        
        let mut buffer: [u8; 4] = [0; 4];
        content.encode_utf8(&mut buffer);
        self.buffer.extend_from_slice(&buffer[..content.len_utf8()]);
        Ok(())
    }
    
    pub fn write(&mut self, color: LineColor, content: Arguments<'_>) -> Result<()> {
        if self.cur_color.is_none_or(|cur_color| cur_color != color) {
            color.encode(&mut self.buffer)?;
            self.cur_color = Some(color);
        }
        
        self.buffer.write_fmt(content)?;
        Ok(())
    }
    
    pub fn write_whitespace(&mut self, content: &str) {
        self.buffer.extend_from_slice(content.as_bytes());
    }
    
    pub fn seek(&mut self, x: u16) -> Result<()> {
        self.flush()?;
        self.x = x;
        Ok(())
    }
    
    pub fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        
        let mut stdout = stdout();
        stdout.queue(MoveTo(self.x, self.y))?;
        stdout.write(&self.buffer)?;
        stdout.flush()?;
        
        self.cur_color = None;
        self.buffer.clear();
        Ok(())
    }
}
