use std::{env, fs, io::{ErrorKind, stdout}, process::exit};

use anyhow::Result;
use crossterm::{event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind}, execute};
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
    
    // Add panic hook to disable mouse capture
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Err(err) = execute!(stdout(), DisableMouseCapture) {
            eprintln!("Error: {err:?}");
        }
        
        hook(info);
    }));
    
    // Run TUI
    let terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;
    let result = run(terminal, State::new(&input_file, &input_bytes));
    let result2 = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    
    if let Err(err) = result2 {
        eprintln!("Error: {err:?}");
    }
    result
}

struct State<'a> {
    scroll_pos: usize,
    max_rows: usize,
    
    selection: Option<(usize, usize)>,
    
    area: Rect,
    
    file_name: &'a str,
    input_bytes: &'a [u8],
    
    bottom_text: Option<String>,
}

impl<'a> State<'a> {
    fn new(file_name: &'a str, input_bytes: &'a [u8]) -> Self {
        Self {
            scroll_pos: 0,
            max_rows: input_bytes.len().div_ceil(16),
            selection: None,
            area: Rect::default(),
            file_name,
            input_bytes,
            bottom_text: None,
        }
    }
    
    fn visible_content_rows(&self) -> usize {
        self.area.height as usize - 4
    }
}

fn run(mut terminal: DefaultTerminal, mut state: State<'_>) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, &mut state).unwrap())?;
        
        match event::read()? {
            Event::Key(key_event) => {
                // special case for Ctrl C
                if let KeyCode::Char('c') = key_event.code && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
                
                match key_event.code {
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                        if let Some((row, _)) = &mut state.selection {
                            // Move cursor up if it's not at maximum height
                            *row = row.saturating_sub(1);
                            
                            // Scroll up if cursor goes out of bounds
                            if *row < state.scroll_pos {
                                state.scroll_pos = state.scroll_pos.saturating_sub(1);
                            }
                        } else {
                            // Scroll up if it's not at maximum height
                            state.scroll_pos = state.scroll_pos.saturating_sub(1);
                        }
                    },
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                        if let Some((row, _)) = &mut state.selection {
                            // Move cursor down if it's not at maximum height
                            if *row < state.max_rows {
                                *row += 1;
                            }
                            
                            // Scroll down if cursor goes out of bounds
                            if *row >= state.scroll_pos + state.visible_content_rows() {
                                state.scroll_pos += 1;
                            }
                        } else {
                            // Scroll down if it's not at maximum height
                            if state.scroll_pos < state.max_rows {
                                state.scroll_pos += 1;
                            }
                        }
                    },
                    KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                        if let Some((_, col)) = &mut state.selection {
                            if !key_event.modifiers.contains(KeyModifiers::ALT) {
                                // Move cursor left in byte-increments (stop at left edge)
                                *col = col.saturating_sub(2);
                                *col = *col / 2 * 2;
                            } else {
                                // Move cursor left in digit-increments (stop at left edge)
                                *col = col.saturating_sub(1);
                            }
                        }
                    },
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                        if let Some((_, col)) = &mut state.selection {
                            if !key_event.modifiers.contains(KeyModifiers::ALT) {
                                // Move cursor right in byte-increments (stop at right edge)
                                if *col < 0x1e {
                                    *col += 2;
                                    *col = *col / 2 * 2;
                                }
                            } else {
                                // Move cursor right in digit-increments (stop at right edge)
                                if *col < 0x1f {
                                    *col += 1;
                                }
                            }
                        }
                    },
                    KeyCode::Char('c') => {
                        // Toggle pager and selection mode
                        if state.selection.is_some() {
                            state.selection = None;
                        } else {
                            state.selection = Some((state.scroll_pos, 0));
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        // Quit
                        return Ok(());
                    },
                    _ => {},
                }
            },
            Event::Mouse(mouse_event) => {
                match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let mut row = (mouse_event.row as usize).saturating_sub(2);
                        if row >= state.visible_content_rows() {
                            row = state.visible_content_rows() - 1;
                        }
                        
                        if mouse_event.column >= 0x27 {
                            let raw_col = (mouse_event.column as usize).saturating_sub(0x27);
                            let mut col = raw_col / 3 * 2;
                            if mouse_event.modifiers.contains(KeyModifiers::ALT) {
                                col += raw_col % 3;
                            }
                            if col >= 0x10 {
                                col = 0xf;
                            }
                            state.selection = Some((row, col + 0x10));
                        } else {
                            let raw_col = (mouse_event.column as usize).saturating_sub(0xe);
                            let mut col = raw_col / 3 * 2;
                            if mouse_event.modifiers.contains(KeyModifiers::ALT) {
                                col += raw_col % 3;
                            }
                            state.selection = Some((row, col));
                        }
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

fn draw(frame: &mut Frame, state: &mut State<'_>) -> Result<()> {
    state.area = frame.area();
    
    frame.render_widget(Span::styled(state.file_name, TITLE_STYLE), frame.area());
    draw_bottom(frame, state, frame.area().rows().last().unwrap())?;
    
    let area = frame.area().inner(Margin::new(2, 2));
    
    for (i, row) in area.rows().enumerate() {
        if i + state.scroll_pos >= state.max_rows {
            break;
        }
        
        draw_line(frame, state, row, i + state.scroll_pos)?;
    }
    
    Ok(())
}

fn draw_bottom(frame: &mut Frame, state: &State<'_>, row: Rect) -> Result<()> {
    let mut writer = LineWriter::new(frame, row);
    
    if let Some(bottom_text) = state.bottom_text.as_deref() {
        writer.write_str(LineColor::Regular, bottom_text);
    } else if state.selection.is_some() {
        writer.write_str(LineColor::RegularBold, "Q");
        writer.write_str(LineColor::Regular, " exit, ");
        writer.write_str(LineColor::RegularBold, "C");
        writer.write_str(LineColor::Regular, " pager, ");
        writer.write_str(LineColor::RegularBold, "G");
        writer.write_str(LineColor::Regular, " go to, ");
        writer.write_str(LineColor::RegularBold, "HJKL/Arrows");
        writer.write_str(LineColor::Regular, " move selection (");
        writer.write_str(LineColor::RegularBold, "Alt");
        writer.write_str(LineColor::Regular, " to move by digits) ");
    } else {
        writer.write_str(LineColor::RegularBold, "Q");
        writer.write_str(LineColor::Regular, " exit, ");
        writer.write_str(LineColor::RegularBold, "C");
        writer.write_str(LineColor::Regular, " cursor, ");
        writer.write_str(LineColor::RegularBold, "G");
        writer.write_str(LineColor::Regular, " go to, ");
        writer.write_str(LineColor::RegularBold, "J/Down");
        writer.write_str(LineColor::Regular, " scroll down, ");
        writer.write_str(LineColor::RegularBold, "K/Up");
        writer.write_str(LineColor::Regular, " scroll up ");
    }
    
    writer.flush();
    Ok(())
}

fn draw_line(frame: &mut Frame, state: &State<'_>, row: Rect, row_idx: usize) -> Result<()> {
    let offset = row_idx * 0x10;
    
    let selected_col = match state.selection {
        Some((row, col)) => (row_idx == row).then_some(col),
        None => None,
    };
    
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
    let write_byte = |writer: &mut LineWriter<'_, '_>, col: usize, x: u8| -> Result<()> {
        if let Some(selected_col) = selected_col {
            if selected_col / 2 == col && selected_col % 2 == 0 {
                writer.write(LineColor::Highlighted, format_args!("{:01x}", x >> 4))?;
                writer.write(color_of(x), format_args!("{:01x} ", x & 0xF))?;
                return Ok(());
            } else if selected_col / 2 == col && selected_col % 2 == 1 {
                writer.write(color_of(x), format_args!("{:01x}", x >> 4))?;
                writer.write(LineColor::Highlighted, format_args!("{:01x}", x & 0xF))?;
                writer.write_char(LineColor::Regular, ' ');
                return Ok(());
            }
        }
        
        writer.write(color_of(x), format_args!("{:02x} ", x))?;
        Ok(())
    };
    
    for (i, x) in first_half.iter().copied().enumerate() {
        write_byte(&mut writer, i, x)?;
    }
    
    writer.write_whitespace(" ");
    
    for (i, x) in second_half.iter().copied().enumerate() {
        write_byte(&mut writer, i + 0x8, x)?;
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
