use std::{fmt::{Arguments, Write}, marker::PhantomData};

use anyhow::{Error, Result};
use ratatui::{Frame, layout::Rect, style::{Color, Modifier, Style}, text::Span};

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
}

pub struct LineWriter<'a, 'b> {
    buffer: String,
    cur_color: LineColor,
    
    original_row: Rect,
    row: Rect,
    
    frame: *mut Frame<'b>,
    _marker: PhantomData<&'a mut Frame<'b>>,
}

impl<'a, 'b> LineWriter<'a, 'b> {
    /// SAFETY: `frame` must not be touched while this LineWriter is alive
    /// by anyone except other LineWriters
    pub unsafe fn new(frame: *mut Frame<'b>, row: Rect) -> Self {
        Self {
            buffer: String::new(),
            cur_color: LineColor::Regular,
            original_row: row,
            row,
            frame,
            _marker: PhantomData,
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
        
        let frame = unsafe { &mut *self.frame};
        frame.render_widget(Span::styled(&*self.buffer, self.cur_color.style()), self.row);
        self.row.x += self.buffer.len() as u16;
        self.row.width -= self.buffer.len() as u16;
        self.buffer.clear();
    }
}
