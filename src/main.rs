use std::{
    collections::HashMap,
    env, fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write, stdout},
    mem,
    process::exit,
};

use anyhow::{Error, Result};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
};
use itertools::Itertools;
use memchr::memmem;
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Margin, Rect},
    style::{Color, Style},
    text::{Span, Text},
};

use crate::{cfg::{Config, Keybinds}, util::{LineColor, LineWriter}};

mod cfg;
mod util;

const DEFAULT_CONFIG: &str = include_str!("res/default_config.toml");

fn main() -> Result<()> {
    let mut config_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("testing/config.toml")?;
    
    let config: Config = if config_file.metadata()?.len() == 0 {
        config_file.write_all(DEFAULT_CONFIG.as_bytes())?;
        toml::from_str(DEFAULT_CONFIG)?
    } else {
        let mut content = String::new();
        config_file.read_to_string(&mut content)?;
        
        toml::from_str(&content)?
    };
    
    println!("{config:#?}");
    
    // let test_config = Config::default();
    // let test_config_string = toml::to_string_pretty(&test_config)?;
    
    // fs::write("testing/config.toml", &test_config_string)?;
    
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
    let result = run(terminal, &config, State::new(&input_file, input_bytes));
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

fn run(mut terminal: DefaultTerminal, config: &Config, mut state: State<'_>) -> Result<()> {
    let keybinds = &config.keybinds;
    
    loop {
        terminal.draw(|frame| draw(frame, &config.keybinds, &mut state).unwrap())?;
        
        match event::read()? {
            Event::Key(key_event) => {
                // special case for Ctrl C
                if let KeyCode::Char('c') = key_event.code && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
                
                match &mut state.input_state {
                    InputState::Regular => {
                        if !handle_key(key_event, keybinds, &mut state) {
                            // Quit if it returns false
                            // TODO: ask if unsaved changes
                            return Ok(());
                        }
                    },
                    InputState::Goto(buffer) | InputState::FindBytes(buffer) => {
                        match key_event.code {
                            KeyCode::Backspace => {
                                buffer.pop();
                            },
                            KeyCode::Char(c) => {
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
                        
                        if keybinds.quit.matches(key_event) {
                            return Ok(())
                        }
                    },
                    InputState::FindString(buffer) => {
                        match key_event.code {
                            KeyCode::Backspace => {
                                buffer.pop();
                            },
                            KeyCode::Char(c) => {
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
                        
                        if keybinds.quit.matches(key_event) {
                            return Ok(())
                        }
                    },
                    InputState::Find => {
                        if key_event.code == KeyCode::Esc {
                            state.queued_input_state = Some(InputState::Regular);
                        }
                        
                        if keybinds.find_binary.matches(key_event) {
                            state.queued_input_state = Some(InputState::FindBytes(String::new()));
                        }
                        
                        if keybinds.find_text.matches(key_event) {
                            state.queued_input_state = Some(InputState::FindString(String::new()));
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

fn handle_key(event: KeyEvent, keybinds: &Keybinds, state: &mut State<'_>) -> bool {
    if event.code == KeyCode::Up || keybinds.up.matches(event) {
        // Up
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
    }
    if event.code == KeyCode::Down || keybinds.down.matches(event) {
        // Down
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
    }
    if event.code == KeyCode::Left || keybinds.left.matches(event) {
        // Left
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
    }
    if event.code == KeyCode::Right || keybinds.right.matches(event) {
        // Right
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
    }
    if keybinds.toggle_cursor.matches(event) {
        // Toggle pager and selection mode
        if state.selection.is_some() {
            state.selection = None;
        } else {
            state.selection = Some((state.scroll_pos, 0));
        }
    }
    if keybinds.go_to.matches(event) {
        // Go to
        state.queued_input_state = Some(InputState::Goto(String::new()));
    }
    if keybinds.find.matches(event) {
        // Find
        state.queued_input_state = Some(InputState::Find);
    }
    if keybinds.save.matches(event) {
        // TODO: Save as
        if let Err(err) = state.save_file() {
            state.bottom_text = Some(format!("Error: {err}"));
        }
    }
    if keybinds.quit.matches(event) {
        // Quit
        return false;
    }
    
    match event.code {
        KeyCode::Home => {
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                state.scroll_pos = 0;
            }
            
            if let Some((row, col)) = &mut state.selection {
                if event.modifiers.contains(KeyModifiers::CONTROL) {
                    *row = 0;
                    *col = 0;
                } else {
                    *col = 0;
                }
            }
        },
        KeyCode::End => {
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                state.scroll_pos = usize::max(
                    state.scroll_pos,
                    state.bytes.len() / 0x10 - (state.area.height as usize - 4) + 1,
                );
            }
            
            if let Some((row, col)) = &mut state.selection {
                if event.modifiers.contains(KeyModifiers::CONTROL) {
                    *row = state.bytes.len() / 0x10;
                    *col = (state.bytes.len() % 0x10) * 2 - 1;
                } else {
                    *col = 0x1f;
                }
                
                if !event.modifiers.contains(KeyModifiers::ALT) {
                    *col -= 1;
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

fn draw(frame: &mut Frame, keybinds: &Keybinds, state: &mut State<'_>) -> Result<()> {
    state.area = frame.area();
    
    frame.render_widget(Span::styled(state.file_name, TITLE_STYLE), frame.area());
    draw_bottom(frame, keybinds, state, frame.area().rows().last().unwrap())?;
    
    let area = frame.area().inner(Margin::new(2, 2));
    
    for (i, row) in area.rows().enumerate() {
        if i + state.scroll_pos >= state.max_rows {
            break;
        }
        
        draw_line(frame, state, row, i + state.scroll_pos)?;
    }
    
    Ok(())
}

fn draw_bottom(frame: &mut Frame, keybinds: &Keybinds, state: &State<'_>, row: Rect) -> Result<()> {
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
        (LineColor::Regular, LineColor::Emphasis)
    };
    
    match &state.input_state {
        InputState::Goto(goto_buffer) => {
            writer.write_str(LineColor::Emphasis, "Go to: 0x");
            writer.write_str(LineColor::Regular, goto_buffer);
            // TODO: figure out blinking cursor
            writer.write_char(LineColor::TextCursor, ' ');
        },
        InputState::Find => {
            writer.write(LineColor::Emphasis, format_args!("Find what?  {}", keybinds.find_binary))?;
            writer.write_str(LineColor::Regular, " bytes, ");
            writer.write(LineColor::Emphasis, format_args!("{}", keybinds.find_text))?;
            writer.write_str(LineColor::Regular, " text");
        },
        InputState::FindBytes(byte_buffer) => {
            writer.write_str(LineColor::Emphasis, "Find byte sequence (in hex): ");
            
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
            writer.write_str(LineColor::Emphasis, "Find text: ");
            writer.write_str(LineColor::Regular, string_buffer);
            writer.write_char(LineColor::TextCursor, ' ');
        },
        InputState::Regular => {
            if let Some(bottom_text) = state.bottom_text.as_deref() {
                writer.write_str(LineColor::Regular, bottom_text);
            } else if state.selection.is_some() {
                writer.write(LineColor::Emphasis, format_args!("{}", keybinds.quit))?;
                writer.write_str(LineColor::Regular, " exit, ");
                writer.write(LineColor::Emphasis, format_args!("{}", keybinds.toggle_cursor))?;
                writer.write_str(LineColor::Regular, " pager, ");
                writer.write(LineColor::Emphasis, format_args!("{}", keybinds.go_to))?;
                writer.write_str(LineColor::Regular, " go to, ");
                writer.write(LineColor::Emphasis, format_args!("{}", keybinds.find))?;
                writer.write_str(LineColor::Regular, " find, ");
                writer.write(save_color_bold, format_args!("{}", keybinds.save))?;
                writer.write_str(save_color, " save, ");
                writer.write(LineColor::Emphasis, format_args!("{}{}{}{}/Arrows",
                    keybinds.left, keybinds.down, keybinds.up, keybinds.right))?;
                writer.write_str(LineColor::Regular, " move selection (");
                writer.write_str(LineColor::Emphasis, "Alt");
                writer.write_str(LineColor::Regular, " to move by digits) ");
            } else {
                writer.write(LineColor::Emphasis, format_args!("{}", keybinds.quit))?;
                writer.write_str(LineColor::Regular, " exit, ");
                writer.write(LineColor::Emphasis, format_args!("{}", keybinds.toggle_cursor))?;
                writer.write_str(LineColor::Regular, " cursor, ");
                writer.write(LineColor::Emphasis, format_args!("{}", keybinds.go_to))?;
                writer.write_str(LineColor::Regular, " go to, ");
                writer.write(LineColor::Emphasis, format_args!("{}", keybinds.find))?;
                writer.write_str(LineColor::Regular, " find, ");
                writer.write(save_color_bold, format_args!("{}", keybinds.save))?;
                writer.write_str(save_color, " save, ");
                writer.write(LineColor::Emphasis, format_args!("{}/Down", keybinds.down))?;
                writer.write_str(LineColor::Regular, " scroll down, ");
                writer.write(LineColor::Emphasis, format_args!("{}/Up", keybinds.up))?;
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
