use std::{collections::HashMap, env, fs, io::{ErrorKind, stdout}, mem, process::exit};

use anyhow::{Error, Result};
use crossterm::{event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind}, execute};
use itertools::Itertools;
use memchr::memmem;
use ratatui::{DefaultTerminal, Frame, layout::{Margin, Rect}, style::{Color, Style}, text::{Span, Text}};

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
    let result = run(terminal, State::new(&input_file, input_bytes));
    let result2 = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    
    if let Err(err) = result2 {
        eprintln!("Error: {err:?}");
    }
    result
}

#[derive(Debug)]
enum InputState {
    Regular,
    Goto(String),
    Find,
    FindBytes(String),
    FindString(String),
    // SaveAs,
}

struct State<'a> {
    scroll_pos: usize,
    max_rows: usize,
    
    selection: Option<(usize, usize)>,
    input_state: InputState,
    queued_input_state: Option<InputState>,
    
    area: Rect,
    
    file_name: &'a str,
    bytes: Vec<u8>,
    
    modified_bytes: HashMap<usize, [bool; 0x10]>,
    
    bottom_text: Option<String>,
}

impl<'a> State<'a> {
    fn new(file_name: &'a str, bytes: Vec<u8>) -> Self {
        Self {
            scroll_pos: 0,
            max_rows: bytes.len().div_ceil(16),
            selection: None,
            input_state: InputState::Regular,
            queued_input_state: None,
            area: Rect::default(),
            file_name,
            bytes,
            modified_bytes: HashMap::new(),
            bottom_text: None,
        }
    }
    
    fn commit_input_state(&mut self) {
        match &mut self.input_state {
            InputState::Goto(goto_buffer) => {
                let Ok(goto_offset) = usize::from_str_radix(goto_buffer, 16) else {
                    return;
                };
                
                if goto_offset >= self.bytes.len() {
                    return;
                }
                
                self.scroll_pos = goto_offset / 0x10;
                self.selection = Some((goto_offset / 0x10, (goto_offset % 0x10) * 2));
                self.queued_input_state = Some(InputState::Regular);
            },
            InputState::FindBytes(needle_string) => {
                let Ok(needle) = hex::decode(needle_string) else {
                    return;
                };
                
                let Some(index) = memmem::find(&self.bytes, &needle) else {
                    return;
                };
                
                self.scroll_pos = index / 0x10;
                self.selection = Some((index / 0x10, (index % 0x10) * 2));
                self.queued_input_state = Some(InputState::Regular);
            },
            InputState::FindString(needle_string) => {
                let Some(index) = memmem::find(&self.bytes, needle_string.as_bytes()) else {
                    return;
                };
                
                self.scroll_pos = index / 0x10;
                self.selection = Some((index / 0x10, (index % 0x10) * 2));
                self.queued_input_state = Some(InputState::Regular);
            },
            _ => panic!("State {:?} cannot be committed", self.input_state),
        }
    }
    
    fn save_file(&mut self) -> Result<()> {
        self.modified_bytes.clear();
        fs::write(self.file_name, &self.bytes).map_err(Error::new)
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
                
                match &mut state.input_state {
                    InputState::Regular => {
                        if !handle_key(key_event, &mut state) {
                            // Quit if it returns false
                            return Ok(());
                        }
                    },
                    InputState::Goto(buffer) | InputState::FindBytes(buffer) => {
                        match key_event.code {
                            KeyCode::Backspace => {
                                buffer.pop();
                            },
                            KeyCode::Char(c) => {
                                if c == 'q' || c == 'Q' {
                                    return Ok(());
                                }
                                if c.is_ascii_hexdigit() {
                                    buffer.push(c);
                                }
                            },
                            KeyCode::Enter => {
                                state.commit_input_state();
                            },
                            KeyCode::Esc => {
                                state.queued_input_state = Some(InputState::Regular);
                            }
                            _ => {},
                        }
                    },
                    InputState::FindString(buffer) => {
                        match key_event.code {
                            KeyCode::Backspace => {
                                buffer.pop();
                            },
                            KeyCode::Char(c) => {
                                if c == 'q' || c == 'Q' {
                                    return Ok(());
                                }
                                
                                buffer.push(c);
                            },
                            KeyCode::Enter => {
                                state.commit_input_state();
                            },
                            KeyCode::Esc => {
                                state.queued_input_state = Some(InputState::Regular);
                            }
                            _ => {},
                        }
                    },
                    InputState::Find => {
                        match key_event.code {
                            KeyCode::Char('b') => {
                                state.queued_input_state = Some(InputState::FindBytes(String::new()));
                            },
                            KeyCode::Char('t') => {
                                state.queued_input_state = Some(InputState::FindString(String::new()));
                            },
                            _ => {},
                        }
                    },
                }
            },
            Event::Mouse(mouse_event) => {
                handle_mouse(mouse_event, &mut state);
            },
            _ => {},
        }
        
        if let Some(queued_input_state) = mem::take(&mut state.queued_input_state) {
            state.input_state = queued_input_state;
        }
    }
}

fn handle_key(event: KeyEvent, state: &mut State<'_>) -> bool {
    match event.code {
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
                if *row < state.max_rows - 1 {
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
                if !event.modifiers.contains(KeyModifiers::ALT) {
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
                if !event.modifiers.contains(KeyModifiers::ALT) {
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
        },
        KeyCode::Char('g') => {
            state.queued_input_state = Some(InputState::Goto(String::new()));
        },
        KeyCode::Char('f') => {
            state.queued_input_state = Some(InputState::Find);
        },
        KeyCode::Char('s') | KeyCode::Char('S') => {
            // TODO: Save as
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                if let Err(err) = state.save_file() {
                    state.bottom_text = Some(format!("Error: {err}"));
                }
            }
        },
        KeyCode::Esc => {
            if state.selection.is_some() {
                // Go back to pager if in cursor mode
                state.selection = None;
            } else {
                // Quit if in pager mode
                return false;
            }
        }
        KeyCode::Char('q') => {
            // Quit
            return false;
        },
        KeyCode::Char(c) => {
            if let Some((row, col)) = &mut state.selection
            && let Some(digit) = c.to_digit(10) {
                let offset = *col / 2 + *row * 0x10;
                let prev_byte = state.bytes[offset];
                
                let new_byte = if *col % 2 == 0 {
                    // Modify upper half of byte
                    (prev_byte & 0xF) | ((digit as u8) << 4)
                } else {
                    // Modify lower half of byte
                    (prev_byte & 0xF0) | (digit as u8)
                };
                
                if prev_byte != new_byte {
                    state.bytes[offset] = new_byte;
                    state.modified_bytes.entry(*row).or_default()[*col / 2] = true;
                }
                
                *col += 1;
                if *col >= 0x20 {
                    *col = 0;
                    *row += 1;
                }
            }
        },
        _ => {},
    }
    
    true
}

fn handle_mouse(event: MouseEvent, state: &mut State<'_>) {
    let InputState::Regular = state.input_state else {
        return;
    };
    
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let mut row = (event.row as usize).saturating_sub(2);
            if row >= state.visible_content_rows() {
                row = state.visible_content_rows() - 1;
            }
            
            if event.column >= 0x27 {
                let raw_col = (event.column as usize).saturating_sub(0x27);
                let mut col = raw_col / 3 * 2;
                if event.modifiers.contains(KeyModifiers::ALT) {
                    col += raw_col % 3;
                }
                if col >= 0x10 {
                    col = 0xf;
                }
                state.selection = Some((row + state.scroll_pos, col + 0x10));
            } else {
                let raw_col = (event.column as usize).saturating_sub(0xe);
                let mut col = raw_col / 3 * 2;
                if event.modifiers.contains(KeyModifiers::ALT) {
                    col += raw_col % 3;
                }
                state.selection = Some((row + state.scroll_pos, col));
            }
        },
        _ => {},
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
    let visible_bytes = usize::min(
        (state.scroll_pos + state.visible_content_rows() - 1) * 0x10,
        state.bytes.len() - 0x10,
    );
    let percentage = ((visible_bytes + 0x10) as f32 / state.bytes.len() as f32 * 100.0) as usize;
    let percentage_string = format!("{:x} / {:x}, {}%", visible_bytes, state.bytes.len(), percentage);
    frame.render_widget(Text::raw(&percentage_string).right_aligned(), row);
    
    let mut writer = LineWriter::new(frame, row);
    
    let (save_color, save_color_bold) = if state.modified_bytes.is_empty() {
        (LineColor::Zero, LineColor::Zero)
    } else {
        (LineColor::Regular, LineColor::RegularBold)
    };
    
    match &state.input_state {
        InputState::Goto(goto_buffer) => {
            writer.write_str(LineColor::RegularBold, "Go to: 0x");
            writer.write_str(LineColor::Regular, goto_buffer);
            // TODO: figure out blinking cursor
            writer.write_char(LineColor::TextCursor, ' ');
        },
        InputState::Find => {
            writer.write_str(LineColor::RegularBold, "Find what?  B");
            writer.write_str(LineColor::Regular, " bytes, ");
            writer.write_str(LineColor::RegularBold, "T");
            writer.write_str(LineColor::Regular, " text");
        },
        InputState::FindBytes(byte_buffer) => {
            writer.write_str(LineColor::RegularBold, "Find byte sequence (in hex): ");
            
            let chunks = byte_buffer.chars().chunks(2);
            for (i, chunk) in chunks.into_iter().enumerate() {
                for c in chunk {
                    writer.write_char(LineColor::Regular, c);
                }
                
                if i * 2 + 1 < byte_buffer.len() {
                    writer.write_whitespace(" ");
                }
            }
            
            writer.write_char(LineColor::TextCursor, ' ');
        },
        InputState::FindString(string_buffer) => {
            writer.write_str(LineColor::RegularBold, "Find text: ");
            writer.write_str(LineColor::Regular, string_buffer);
            writer.write_char(LineColor::TextCursor, ' ');
        },
        InputState::Regular => {
            if let Some(bottom_text) = state.bottom_text.as_deref() {
                writer.write_str(LineColor::Regular, bottom_text);
            } else if state.selection.is_some() {
                writer.write_str(LineColor::RegularBold, "Q");
                writer.write_str(LineColor::Regular, " exit, ");
                writer.write_str(LineColor::RegularBold, "C");
                writer.write_str(LineColor::Regular, " pager, ");
                writer.write_str(LineColor::RegularBold, "G");
                writer.write_str(LineColor::Regular, " go to, ");
                writer.write_str(LineColor::RegularBold, "F");
                writer.write_str(LineColor::Regular, " find, ");
                writer.write_str(save_color_bold, "^S");
                writer.write_str(save_color, " save, ");
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
                writer.write_str(LineColor::RegularBold, "F");
                writer.write_str(LineColor::Regular, " find, ");
                writer.write_str(save_color_bold, "^S");
                writer.write_str(save_color, " save, ");
                writer.write_str(LineColor::RegularBold, "J/Down");
                writer.write_str(LineColor::Regular, " scroll down, ");
                writer.write_str(LineColor::RegularBold, "K/Up");
                writer.write_str(LineColor::Regular, " scroll up ");
            }
        },
    }
    
    writer.flush();
    Ok(())
}

fn draw_line(frame: &mut Frame, state: &State<'_>, row: Rect, row_idx: usize) -> Result<()> {
    let offset = row_idx * 0x10;
    
    let modified_bytes = state.modified_bytes.get(&row_idx).copied().unwrap_or_default();
    
    let selected_col = match state.selection {
        Some((row, col)) => (row_idx == row).then_some(col),
        None => None,
    };
    
    let mut writer = LineWriter::new(frame, row);
    
    // Write offset
    writer.write(LineColor::Address, format_args!("{:04x} {:04x}", offset >> 16, offset & 0xFFFF))?;
    writer.write_str(LineColor::Regular, ":  ");
    
    let first_half = &state.bytes[offset..usize::min(
        offset + 0x8, 
        state.bytes.len(),
    )];
    let second_half = &state.bytes[usize::min(
        offset + 0x8, 
        state.bytes.len(),
    )..usize::min(
        offset + 0x10, 
        state.bytes.len(),
    )];
    
    let color_of = |col: usize, x: u8| {
        if modified_bytes[col] {
            LineColor::Modified
        } else if x == 0 {
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
                writer.write(color_of(col, x), format_args!("{:01x} ", x & 0xF))?;
                return Ok(());
            } else if selected_col / 2 == col && selected_col % 2 == 1 {
                writer.write(color_of(col, x), format_args!("{:01x}", x >> 4))?;
                writer.write(LineColor::Highlighted, format_args!("{:01x}", x & 0xF))?;
                writer.write_char(LineColor::Regular, ' ');
                return Ok(());
            }
        }
        
        writer.write(color_of(col, x), format_args!("{:02x} ", x))?;
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
