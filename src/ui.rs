use anyhow::Result;
use itertools::Itertools;
use ratatui::{Frame, layout::Rect, style::{Color, Style}, text::{Span, Text}};

use crate::{InputState, State, cfg::{Appearance, Config, Keybinds}, util::{LineColor, LineWriter}};

const TITLE_STYLE: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Rgb(220, 220, 220));

pub fn draw(frame: &mut Frame, config: &Config, state: &mut State<'_>) -> Result<()> {
    state.area = frame.area();
    
    // Draw status ui
    frame.render_widget(Span::styled(state.file_name, TITLE_STYLE), frame.area());
    
    let mut rows = frame.area().rows().skip(frame.area().rows().try_len().unwrap() - 2);
    draw_bottom(frame, &config.keybinds, state, [rows.next().unwrap(), rows.next().unwrap()])?;
    
    // Draw main page
    let Appearance { margin_horizontal, margin_vertical, .. } = config.appearance;
    
    let area = Rect::new(
        frame.area().x + margin_horizontal,
        frame.area().y + margin_vertical + 1,
        frame.area().width - margin_horizontal * 2,
        frame.area().height - margin_vertical * 2 - 3,
    );
    
    for (i, row) in area.rows().enumerate() {
        if i + state.scroll_pos >= state.max_rows {
            break;
        }
        
        draw_line(frame, state, row, i + state.scroll_pos)?;
    }
    
    Ok(())
}

fn draw_bottom(frame: &mut Frame, keybinds: &Keybinds, state: &State<'_>, rows: [Rect; 2]) -> Result<()> {
    let visible_bytes = usize::min(
        (state.scroll_pos + state.visible_content_rows() - 1) * 0x10,
        state.bytes.len() - 0x10,
    );
    let percentage = ((visible_bytes + 0x10) as f32 / state.bytes.len() as f32 * 100.0) as usize;
    let percentage_string = format!("{:x} / {:x}, {}%", visible_bytes, state.bytes.len(), percentage);
    frame.render_widget(Text::raw(&percentage_string).right_aligned(), rows[1]);
    
    // SAFETY: `frame` is not accessed anywhere else in this function from here on
    // Being accessed from multiple LineWriters at the same time is allowed
    let mut line1 = unsafe { LineWriter::new(frame, rows[0]) };
    let mut line2 = unsafe { LineWriter::new(frame, rows[1]) };
    
    let (save_color, save_color_bold) = if state.modified_bytes.is_empty() {
        (LineColor::Zero, LineColor::Zero)
    } else {
        (LineColor::Regular, LineColor::Emphasis)
    };
    
    match &state.input_state {
        InputState::Goto(goto_buffer) => {
            line2.write_str(LineColor::Emphasis, "Go to: 0x");
            line2.write_str(LineColor::Regular, goto_buffer);
            // TODO: figure out blinking cursor
            line2.write_char(LineColor::TextCursor, ' ');
        },
        InputState::Find => {
            line2.write(LineColor::Emphasis, format_args!("Find what?  {}", keybinds.find_binary))?;
            line2.write_str(LineColor::Regular, " bytes, ");
            line2.write(LineColor::Emphasis, format_args!("{}", keybinds.find_text))?;
            line2.write_str(LineColor::Regular, " text, (");
            line2.write_str(LineColor::Emphasis, "Esc");
            line2.write_str(LineColor::Regular, " back)");
        },
        InputState::FindBytes(byte_buffer) => {
            line2.write_str(LineColor::Emphasis, "Find byte sequence (in hex): ");
            
            let chunks = byte_buffer.chars().chunks(2);
            for (i, chunk) in chunks.into_iter().enumerate() {
                for c in chunk {
                    line2.write_char(LineColor::Regular, c);
                }
                
                if i * 2 + 1 < byte_buffer.len() {
                    line2.write_whitespace(" ");
                }
            }
            
            line2.write_char(LineColor::TextCursor, ' ');
        },
        InputState::FindString(string_buffer) => {
            line2.write_str(LineColor::Emphasis, "Find text: ");
            line2.write_str(LineColor::Regular, string_buffer);
            line2.write_char(LineColor::TextCursor, ' ');
        },
        InputState::Edit { .. } => {
            line1.write(LineColor::Emphasis, format_args!("{}", keybinds.quit))?;
            line1.write_str(LineColor::Regular, " exit, ");
            line1.write_str(LineColor::Emphasis, "Esc");
            line1.write_str(LineColor::Regular, " go back, ");
            line1.write_str(LineColor::Emphasis, "0-9 A-F");
            line1.write_str(LineColor::Regular, " overwrite bytes, ");
                line1.write(save_color_bold, format_args!("{}", keybinds.save))?;
                line1.write_str(save_color, " save");
            
            line2.write(LineColor::Emphasis, format_args!("{}{}{}{}/Arrows",
                keybinds.left, keybinds.down, keybinds.up, keybinds.right))?;
            line2.write_str(LineColor::Regular, " move selection (");
            line2.write_str(LineColor::Emphasis, "Alt");
            line2.write_str(LineColor::Regular, " to move by digits) ");
        },
        InputState::Regular => {
            if let Some(bottom_text) = state.bottom_text.as_deref() {
                line2.write_str(LineColor::Regular, bottom_text);
            } else if state.selection.is_some() {
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.quit))?;
                line1.write_str(LineColor::Regular, " exit, ");
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.toggle_cursor))?;
                line1.write_str(LineColor::Regular, " pager,  ");
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.edit))?;
                line1.write_str(LineColor::Regular, " edit, ");
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.go_to))?;
                line1.write_str(LineColor::Regular, " go to, ");
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.find))?;
                line1.write_str(LineColor::Regular, " find, ");
                line1.write(save_color_bold, format_args!("{}", keybinds.save))?;
                line1.write_str(save_color, " save");
                
                line2.write(LineColor::Emphasis, format_args!("{}{}{}{}/Arrows",
                    keybinds.left, keybinds.down, keybinds.up, keybinds.right))?;
                line2.write_str(LineColor::Regular, " move selection (");
                line2.write_str(LineColor::Emphasis, "Alt");
                line2.write_str(LineColor::Regular, " to move by digits) ");
            } else {
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.quit))?;
                line1.write_str(LineColor::Regular, " exit, ");
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.toggle_cursor))?;
                line1.write_str(LineColor::Regular, " cursor, ");
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.edit))?;
                line1.write_str(LineColor::Regular, " edit, ");
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.go_to))?;
                line1.write_str(LineColor::Regular, " go to, ");
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.find))?;
                line1.write_str(LineColor::Regular, " find, ");
                line1.write(save_color_bold, format_args!("{}", keybinds.save))?;
                line1.write_str(save_color, " save");
                
                line2.write(LineColor::Emphasis, format_args!("{}/Down", keybinds.down))?;
                line2.write_str(LineColor::Regular, " scroll down, ");
                line2.write(LineColor::Emphasis, format_args!("{}/Up", keybinds.up))?;
                line2.write_str(LineColor::Regular, " scroll up ");
            }
        },
    }
    
    line1.flush();
    line2.flush();
    Ok(())
}

fn draw_line(frame: &mut Frame, state: &State<'_>, row: Rect, row_idx: usize) -> Result<()> {
    let offset = row_idx * 0x10;
    
    let modified_bytes = state.modified_bytes.get(&row_idx).copied().unwrap_or_default();
    
    let selected_col = match state.selection {
        Some((row, col)) => (row_idx == row).then_some(col),
        None => None,
    };
    
    // SAFETY: `frame` is not used anywhere else in this function
    let mut writer = unsafe { LineWriter::new(frame, row) };
    
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
