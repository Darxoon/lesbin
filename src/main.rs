use std::{env, fmt::Write, fs, io::ErrorKind, process::exit};

use color_eyre::Result;
use crossterm::event::{self, Event};
use ratatui::{DefaultTerminal, Frame, layout::{Margin, Rect}, style::{Color, Style}, text::{Span, Text}};

fn main() -> Result<()> {
    color_eyre::install()?;
    
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
    let result = run(terminal, &input_bytes);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal, input_bytes: &[u8]) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, input_bytes))?;
        if matches!(event::read()?, Event::Key(_)) {
            break Ok(());
        }
    }
}

fn draw(frame: &mut Frame, input_bytes: &[u8]) {
    let area = frame.area().inner(Margin::new(2, 1));
    let mut row_buffer = String::new();
    
    for (i, row) in area.rows().enumerate() {
        let offset = i * 0x10;
        
        const ADDR_STYLE: Style = Style::new()
            .fg(Color::Indexed(206));
        
        // Write offset
        row_buffer.clear();
        write!(row_buffer, "{:04x} {:04x}", offset >> 16, offset & 0xFFFF).unwrap();
        frame.render_widget(Span::styled(&row_buffer, ADDR_STYLE), row);
        
        // Write byte values
        row_buffer.clear();
        row_buffer.push_str(":  ");
        for x in input_bytes[offset..offset + 0x8].iter().copied() {
            write!(row_buffer, "{x:02x} ").unwrap();
        }
        row_buffer.push(' ');
        for x in input_bytes[offset + 0x8..offset + 0x10].iter().copied() {
            write!(row_buffer, "{x:02x} ").unwrap();
        }
        
        // Write ascii text
        row_buffer.push(' ');
        
        for x in input_bytes[offset..offset + 0x8].iter().copied() {
            let mut ascii = x as char;
            if x & 0x80 == 1 || !ascii.is_ascii_graphic() {
                ascii = '.';
            }
            
            write!(row_buffer, "{ascii}").unwrap();
        }
        row_buffer.push(' ');
        for x in input_bytes[offset + 0x8..offset + 0x10].iter().copied() {
            let mut ascii = x as char;
            if x & 0x80 == 1 || !ascii.is_ascii_graphic() {
                ascii = '.';
            }
            
            write!(row_buffer, "{ascii}").unwrap();
        }
        
        let row = Rect::new(row.x + 9, row.y, row.width - 9,  row.height);
        frame.render_widget(&row_buffer, row);
    }
}
