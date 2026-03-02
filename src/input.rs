use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::{InputState, State, cfg::Keybinds};

pub fn handle_input(event: Event, keybinds: &Keybinds, state: &mut State) {
    match event {
        Event::Key(key_event) => {
            // special case for Ctrl C
            if let KeyCode::Char('c') = key_event.code && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                return;
            }
            
            match &mut state.input_state {
                InputState::Regular => {
                    if !handle_key_main(key_event, keybinds, state) {
                        // Quit if it returns false
                        // TODO: ask if unsaved changes
                        return;
                    }
                },
                InputState::Edit { prev_in_pager } => {
                    match key_event.code {
                        KeyCode::Char(c) => {
                            if c.is_ascii_hexdigit() {
                                handle_edit_input(c, state);
                            }
                        },
                        KeyCode::Esc => {
                            state.queued_input_state = Some(InputState::Regular);
                            
                            if *prev_in_pager {
                                state.selection = None;
                            }
                        },
                        _ => {},
                    }
                    
                    handle_navigation(key_event, keybinds, state);
                    
                    // Quit
                    if keybinds.quit.matches(key_event) {
                        return;
                    }
                    
                    // Save
                    if keybinds.save.matches(key_event) {
                        if let Err(err) = state.save_file() {
                            state.bottom_text = Some(format!("Error: {err}"));
                        }
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
                        },
                        _ => {},
                    }
                    
                    if keybinds.quit.matches(key_event) {
                        return;
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
                        },
                        _ => {},
                    }
                    
                    if keybinds.quit.matches(key_event) {
                        return;
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
            handle_mouse(mouse_event, state);
        },
        _ => {},
    }
}

fn handle_edit_input(c: char, state: &mut State<'_>) {
    if let Some((row, col)) = &mut state.selection
        && let Some(digit) = c.to_digit(16)
    {
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
}

fn handle_key_main(event: KeyEvent, keybinds: &Keybinds, state: &mut State<'_>) -> bool {
    handle_navigation(event, keybinds, state);
    
    if keybinds.toggle_cursor.matches(event) {
        // Toggle pager and selection mode
        if state.selection.is_some() {
            state.selection = None;
        } else {
            state.selection = Some((state.scroll_pos, 0));
        }
    }
    if keybinds.edit.matches(event) {
        // Enable edit mode
        state.queued_input_state = Some(InputState::Edit {
            prev_in_pager: state.selection.is_none(),
        });
        
        if state.selection.is_none() {
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
        },
        _ => {},
    }
    
    true
}

fn handle_navigation(event: KeyEvent, keybinds: &Keybinds, state: &mut State<'_>) {
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
}

fn handle_mouse(event: MouseEvent, state: &mut State<'_>) {
    match state.input_state {
        InputState::Regular | InputState::Edit { .. } => {},
        _ => return,
    }
    
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
