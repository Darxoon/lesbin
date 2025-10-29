use std::fmt::{Arguments, Write};

use anyhow::{Error, Result};
use ratatui::{Frame, layout::Rect, style::{Color, Modifier, Style}, text::Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineColor {
    Address,
    Regular,
    RegularBold,
    Zero,
}

impl LineColor {
    pub const fn style(self) -> Style {
        match self {
            LineColor::Address => Style::new()
                .fg(Color::Indexed(206)),
            LineColor::Regular => Style::new(),
            LineColor::RegularBold => Style::new()
                .add_modifier(Modifier::BOLD),
            LineColor::Zero => Style::new()
                .fg(Color::DarkGray),
        }
    }
}

pub struct LineWriter<'a, 'b> {
    buffer: String,
    cur_color: LineColor,
    
    original_row: Rect,
    row: Rect,
    
    frame: &'a mut Frame<'b>,
}

impl<'a, 'b> LineWriter<'a, 'b> {
    pub fn new(frame: &'a mut Frame<'b>, row: Rect) -> Self {
        Self {
            buffer: String::new(),
            cur_color: LineColor::Regular,
            original_row: row,
            row,
            frame,
        }
    }
    
    pub fn write_str(&mut self, color: LineColor, content: &str) {
        if self.cur_color != color {
            self.flush();
            self.cur_color = color;
        }
        
        self.buffer.push_str(content);
    }
    
    pub fn write_char(&mut self, color: LineColor, content: char) {
        if self.cur_color != color {
            self.flush();
            self.cur_color = color;
        }
        
        self.buffer.push(content);
    }
    
    pub fn write(&mut self, color: LineColor, content: Arguments<'_>) -> Result<()> {
        if self.cur_color != color {
            self.flush();
            self.cur_color = color;
        }
        
        self.buffer.write_fmt(content).map_err(Error::new)
    }
    
    pub fn write_whitespace(&mut self, content: &str) {
        self.buffer.push_str(content);
    }
    
    pub fn seek(&mut self, x_position: u16) {
        self.flush();
        self.row.x = x_position;
        self.row.width = self.original_row.width - x_position;
    }
    
    pub fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        
        self.frame.render_widget(Span::styled(&*self.buffer, self.cur_color.style()), self.row);
        self.row.x += self.buffer.len() as u16;
        self.row.width -= self.buffer.len() as u16;
        self.buffer.clear();
    }
}
