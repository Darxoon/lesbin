use std::io::stdout;

use anyhow::Result;
use crossterm::{cursor::MoveTo, execute, style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor}, terminal};
use itertools::Itertools;

use crate::{InputState, State, cfg::{Appearance, Config, Keybinds}, util::{LineColor, LineWriter}};

pub const PADDING_TOP: u16 = 1;
pub const PADDING_BOTTOM: u16 = 2;

pub fn draw(config: &Config, state: &mut State<'_>) -> Result<()> {
    let (width, height) = terminal::size()?;
    state.screen_height = height;
    
    // Draw status ui
    execute!(
        stdout(),
        MoveTo(0, 0),
        SetForegroundColor(crossterm::style::Color::Black),
        SetBackgroundColor(crossterm::style::Color::Rgb { r: 220, g: 220, b: 220 }),
        Print(state.file_name),
        ResetColor,
    )?;
    
    draw_bottom(&config.keybinds, state, width, height - 2)?;
    
    // Draw main page
    let Appearance { margin_horizontal, margin_vertical, .. } = config.appearance;
    
    for i in 0..height - (margin_vertical * 2 + PADDING_TOP + PADDING_BOTTOM) {
        let absolute_row_idx = i as usize + state.scroll_pos;
        if absolute_row_idx >= state.max_rows {
            break;
        }
        
        draw_line(state, margin_horizontal, i + margin_vertical + PADDING_TOP, absolute_row_idx)?;
    }
    
    Ok(())
}

fn draw_bottom(keybinds: &Keybinds, state: &State<'_>, width: u16, start_y: u16) -> Result<()> {
    let mut line1 = LineWriter::new(0, start_y);
    let mut line2 = LineWriter::new(0, start_y + 1);
    
    let (save_color, save_color_bold) = if state.modified_bytes.is_empty() {
        (LineColor::Zero, LineColor::Zero)
    } else {
        (LineColor::Regular, LineColor::Emphasis)
    };
    
    match &state.input_state {
        InputState::Goto(goto_buffer) => {
            line2.write_str(LineColor::Emphasis, "Go to: 0x")?;
            line2.write_str(LineColor::Regular, goto_buffer)?;
            // TODO: figure out blinking cursor
            line2.write_char(LineColor::TextCursor, ' ')?;
        },
        InputState::Find => {
            line2.write(LineColor::Emphasis, format_args!("Find what?  {}", keybinds.find_binary))?;
            line2.write_str(LineColor::Regular, " bytes, ")?;
            line2.write(LineColor::Emphasis, format_args!("{}", keybinds.find_text))?;
            line2.write_str(LineColor::Regular, " text, (")?;
            line2.write_str(LineColor::Emphasis, "Esc")?;
            line2.write_str(LineColor::Regular, " back)")?;
        },
        InputState::FindBytes(byte_buffer) => {
            line2.write_str(LineColor::Emphasis, "Find byte sequence (in hex): ")?;
            
            let chunks = byte_buffer.chars().chunks(2);
            for (i, chunk) in chunks.into_iter().enumerate() {
                for c in chunk {
                    line2.write_char(LineColor::Regular, c)?;
                }
                
                if i * 2 + 1 < byte_buffer.len() {
                    line2.write_whitespace(" ");
                }
            }
            
            line2.write_char(LineColor::TextCursor, ' ')?;
        },
        InputState::FindString(string_buffer) => {
            line2.write_str(LineColor::Emphasis, "Find text: ")?;
            line2.write_str(LineColor::Regular, string_buffer)?;
            line2.write_char(LineColor::TextCursor, ' ')?;
        },
        InputState::Edit { .. } => {
            line1.write(LineColor::Emphasis, format_args!("{}", keybinds.quit))?;
            line1.write_str(LineColor::Regular, " exit, ")?;
            line1.write_str(LineColor::Emphasis, "Esc")?;
            line1.write_str(LineColor::Regular, " go back, ")?;
            line1.write_str(LineColor::Emphasis, "0-9 A-F")?;
            line1.write_str(LineColor::Regular, " overwrite bytes, ")?;
                line1.write(save_color_bold, format_args!("{}", keybinds.save))?;
                line1.write_str(save_color, " save")?;
            
            line2.write(LineColor::Emphasis, format_args!("{}{}{}{}/Arrows",
                keybinds.left, keybinds.down, keybinds.up, keybinds.right))?;
            line2.write_str(LineColor::Regular, " move selection (")?;
            line2.write_str(LineColor::Emphasis, "Alt")?;
            line2.write_str(LineColor::Regular, " to move by digits) ")?;
        },
        InputState::Regular => {
            if let Some(bottom_text) = state.bottom_text.as_deref() {
                line2.write_str(LineColor::Regular, bottom_text)?;
            } else if state.selection.is_some() {
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.quit))?;
                line1.write_str(LineColor::Regular, " exit, ")?;
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.toggle_cursor))?;
                line1.write_str(LineColor::Regular, " pager,  ")?;
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.edit))?;
                line1.write_str(LineColor::Regular, " edit, ")?;
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.go_to))?;
                line1.write_str(LineColor::Regular, " go to, ")?;
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.find))?;
                line1.write_str(LineColor::Regular, " find, ")?;
                line1.write(save_color_bold, format_args!("{}", keybinds.save))?;
                line1.write_str(save_color, " save")?;
                
                line2.write(LineColor::Emphasis, format_args!("{}{}{}{}/Arrows",
                    keybinds.left, keybinds.down, keybinds.up, keybinds.right))?;
                line2.write_str(LineColor::Regular, " move selection (")?;
                line2.write_str(LineColor::Emphasis, "Alt")?;
                line2.write_str(LineColor::Regular, " to move by digits) ")?;
            } else {
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.quit))?;
                line1.write_str(LineColor::Regular, " exit, ")?;
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.toggle_cursor))?;
                line1.write_str(LineColor::Regular, " cursor, ")?;
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.edit))?;
                line1.write_str(LineColor::Regular, " edit, ")?;
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.go_to))?;
                line1.write_str(LineColor::Regular, " go to, ")?;
                line1.write(LineColor::Emphasis, format_args!("{}", keybinds.find))?;
                line1.write_str(LineColor::Regular, " find, ")?;
                line1.write(save_color_bold, format_args!("{}", keybinds.save))?;
                line1.write_str(save_color, " save")?;
                
                line2.write(LineColor::Emphasis, format_args!("{}/Down", keybinds.down))?;
                line2.write_str(LineColor::Regular, " scroll down, ")?;
                line2.write(LineColor::Emphasis, format_args!("{}/Up", keybinds.up))?;
                line2.write_str(LineColor::Regular, " scroll up ")?;
            }
        },
    }
    
    // display percentage
    let visible_bytes = usize::min(
        (state.scroll_pos + state.visible_content_rows() - 1) * 0x10,
        state.bytes.len() - 0x10,
    );
    let percentage = ((visible_bytes + 0x10) as f32 / state.bytes.len() as f32 * 100.0) as usize;
    let percentage_string = format!("{:x} / {:x}, {}%", visible_bytes, state.bytes.len(), percentage);
    
    line2.seek(width - percentage_string.len() as u16)?;
    line2.write_str(LineColor::Regular, &percentage_string)?;
    
    line1.flush()?;
    line2.flush()?;
    Ok(())
}

fn draw_line(state: &State<'_>, x: u16, y: u16, row_idx: usize) -> Result<()> {
    let offset = row_idx * 0x10;
    
    let modified_bytes = state.modified_bytes.get(&row_idx).copied().unwrap_or_default();
    
    let selected_col = match state.selection {
        Some((row, col)) => (row_idx == row).then_some(col),
        None => None,
    };
    
    let mut writer = LineWriter::new(x, y);
    
    // Write offset
    writer.write(LineColor::Address, format_args!("{:04x} {:04x}", offset >> 16, offset & 0xFFFF))?;
    writer.write_str(LineColor::Regular, ":  ")?;
    
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
    let write_byte = |writer: &mut LineWriter, col: usize, x: u8| -> Result<()> {
        if let Some(selected_col) = selected_col {
            if selected_col / 2 == col && selected_col % 2 == 0 {
                writer.write(LineColor::Highlighted, format_args!("{:01x}", x >> 4))?;
                writer.write(color_of(col, x), format_args!("{:01x} ", x & 0xF))?;
                return Ok(());
            } else if selected_col / 2 == col && selected_col % 2 == 1 {
                writer.write(color_of(col, x), format_args!("{:01x}", x >> 4))?;
                writer.write(LineColor::Highlighted, format_args!("{:01x}", x & 0xF))?;
                writer.write_char(LineColor::Regular, ' ')?;
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
    writer.seek(64)?;
    
    for x in first_half.iter().copied() {
        let mut ascii = x as char;
        if x & 0x80 == 1 || !ascii.is_ascii_graphic() {
            ascii = '.';
        }
        
        writer.write_char(LineColor::Regular, ascii)?;
    }
    
    writer.write_whitespace(" ");
    
    for x in second_half.iter().copied() {
        let mut ascii = x as char;
        if x & 0x80 == 1 || !ascii.is_ascii_graphic() {
            ascii = '.';
        }
        
        writer.write_char(LineColor::Regular, ascii)?;
    }
    
    writer.flush()?;
    Ok(())
}
