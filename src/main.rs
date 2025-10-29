use std::{env, fs, io::{ErrorKind}, process::exit};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{DefaultTerminal, Frame, layout::{Margin, Rect}, style::{Color, Style}, text::Span};

use crate::util::{LineColor, LineWriter};

mod util;

fn main() -> Result<()> {
    // Parse args
    let mut input_file = None;
    for arg in env::args().skip(1) {
        if input_file.is_some() {
            eprintln!("Error: Cannot define more than one input file");
            exit(1);
        }
        
        input_file = Some(arg);
    }
    
    let Some(input_file) = input_file else {
        eprintln!("Error: No input file has been passed");
        exit(1);
    };
    
    // Read input file
    // TODO: large files
    let input_bytes = match fs::read(&input_file) {
        Ok(input_bytes) => input_bytes,
        Err(err) => {
            match err.kind() {
                ErrorKind::NotFound | ErrorKind::IsADirectory => {
                    eprintln!("Error: Could not find file '{input_file}'");
                    exit(1);
                },
                _ => return Err(err.into()),
            }
        },
    };
    
    // Run TUI
    let terminal = ratatui::init();
    let result = run(terminal, State::new(&input_file, &input_bytes));
    ratatui::restore();
    result
}

struct State<'a> {
    scroll_pos: usize,
    max_rows: usize,
    
    file_name: &'a str,
    input_bytes: &'a [u8],
    
    bottom_text: Option<String>,
}

impl<'a> State<'a> {
    fn new(file_name: &'a str, input_bytes: &'a [u8]) -> Self {
        Self {
            scroll_pos: 0,
            max_rows: input_bytes.len().div_ceil(16),
            file_name,
            input_bytes,
            bottom_text: None,
        }
    }
}

fn run(mut terminal: DefaultTerminal, mut state: State<'_>) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, &state).unwrap())?;
        
        match event::read()? {
            Event::Key(key_event) => {
                // special case for Ctrl C
                if let KeyCode::Char('c') = key_event.code && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
                
                match key_event.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.scroll_pos = state.scroll_pos.saturating_sub(1);
                    },
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.scroll_pos < state.max_rows {
                            state.scroll_pos += 1;
                        }
                    },
                    KeyCode::Esc | KeyCode::Char('q') => {
                        return Ok(());
                    },
                    _ => {},
                }
            },
            _ => {},
        }
    }
}

const TITLE_STYLE: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Rgb(220, 220, 220));

fn draw(frame: &mut Frame, state: &State<'_>) -> Result<()> {
    frame.render_widget(Span::styled(state.file_name, TITLE_STYLE), frame.area());
    draw_bottom(frame, state, frame.area().rows().last().unwrap())?;
    
    let area = frame.area().inner(Margin::new(2, 2));
    
    for (i, row) in area.rows().enumerate() {
        if i + state.scroll_pos >= state.max_rows {
            break;
        }
        
        draw_line(frame, state, row, (i + state.scroll_pos) * 0x10)?;
    }
    
    Ok(())
}

fn draw_bottom(frame: &mut Frame, state: &State<'_>, row: Rect) -> Result<()> {
    let mut writer = LineWriter::new(frame, row);
    
    if let Some(bottom_text) = state.bottom_text.as_deref() {
        writer.write_str(LineColor::Regular, bottom_text);
    } else {
        writer.write_str(LineColor::RegularBold, "Q");
        writer.write_str(LineColor::Regular, " exit, ");
        writer.write_str(LineColor::RegularBold, "J/Down");
        writer.write_str(LineColor::Regular, " scroll down, ");
        writer.write_str(LineColor::RegularBold, "K/Up");
        writer.write_str(LineColor::Regular, " scroll up");
    }
    
    writer.flush();
    Ok(())
}

fn draw_line(frame: &mut Frame, state: &State<'_>, row: Rect, offset: usize) -> Result<()> {
    let mut writer = LineWriter::new(frame, row);
    
    // Write offset
    writer.write(LineColor::Address, format_args!("{:04x} {:04x}", offset >> 16, offset & 0xFFFF))?;
    writer.write_str(LineColor::Regular, ":  ");
    
    let first_half = &state.input_bytes[offset..usize::min(
        offset + 0x8, 
        state.input_bytes.len(),
    )];
    let second_half = &state.input_bytes[usize::min(
        offset + 0x8, 
        state.input_bytes.len(),
    )..usize::min(
        offset + 0x10, 
        state.input_bytes.len(),
    )];
    
    let color_of = |x: u8| {
        if x == 0 {
            LineColor::Zero
        } else {
            LineColor::Regular
        }
    };
    
    // Write byte values
    for x in first_half.iter().copied() {
        writer.write(color_of(x), format_args!("{x:02x} "))?;
    }
    
    writer.write_whitespace(" ");
    
    for x in second_half.iter().copied() {
        writer.write(color_of(x), format_args!("{x:02x} "))?;
    }
    
    // Write ascii text
    writer.seek(64);
    
    for x in first_half.iter().copied() {
        let mut ascii = x as char;
        if x & 0x80 == 1 || !ascii.is_ascii_graphic() {
            ascii = '.';
        }
        
        writer.write_char(LineColor::Regular, ascii);
    }
    
    writer.write_whitespace(" ");
    
    for x in second_half.iter().copied() {
        let mut ascii = x as char;
        if x & 0x80 == 1 || !ascii.is_ascii_graphic() {
            ascii = '.';
        }
        
        writer.write_char(LineColor::Regular, ascii);
    }
    
    writer.flush();
    Ok(())
}
