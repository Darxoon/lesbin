use std::{fmt::{Arguments}, io::{Write, stdout}};

use anyhow::{Error, Result};
use crossterm::{QueueableCommand, cursor::MoveTo, queue, style::ResetColor};
use ratatui::style::{Color, Modifier, Style};

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
    const fn style(self) -> Style {
        match self {
            LineColor::Regular => Style::new(),
            LineColor::Emphasis => Style::new()
                .fg(Color::Indexed(39))
                .add_modifier(Modifier::BOLD),
            LineColor::Highlighted => Style::new()
                .fg(Color::Black)
                .bg(Color::Gray),
            LineColor::TextCursor => Style::new()
                .add_modifier(Modifier::REVERSED),
            LineColor::Modified => Style::new()
                .fg(Color::Indexed(215)),
            LineColor::Address => Style::new()
                .fg(Color::Indexed(206)),
            LineColor::Zero => Style::new()
                .fg(Color::DarkGray),
        }
    }
    
    fn encode(self, buffer: &mut Vec<u8>) -> Result<()> {
        queue!(buffer, ResetColor).map_err(Into::into)
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
        
        self.buffer.clear();
        Ok(())
    }
}
