use std::{
    collections::HashMap,
    env,
    fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write, stdout},
    mem,
    process::exit,
};

use anyhow::{Error, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use memchr::memmem;

use crate::{cfg::Config, input::handle_input, ui::draw};

mod cfg;
mod input;
mod ui;
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
    
    eprintln!("{config:#?}");
    
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
        Err(err) => match err.kind() {
            ErrorKind::NotFound | ErrorKind::IsADirectory => {
                eprintln!("Error: Could not find file '{input_file}'");
                exit(1);
            },
            _ => return Err(err.into()),
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
    // TODO: panic hook
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let result = run(&config, State::new(&input_file, input_bytes));
    let result2 = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
    disable_raw_mode()?;
    
    if let Err(err) = result2 {
        eprintln!("Error: {err:?}");
    }
    result
}

#[derive(Debug)]
enum InputState {
    Regular,
    Edit { prev_in_pager: bool },
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
    
    screen_height: usize,
    
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
            screen_height: 0,
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
        // TODO: factor in user defined margin
        // self.area.height as usize - 5
        8
    }
}

fn run(config: &Config, mut state: State<'_>) -> Result<()> {
    let keybinds = &config.keybinds;
    
    loop {
        draw(config, &mut state)?;
        
        if !handle_input(event::read()?, keybinds, &mut state) {
            return Ok(());
        }
        
        if let Some(queued_input_state) = mem::take(&mut state.queued_input_state) {
            state.input_state = queued_input_state;
        }
    }
}
