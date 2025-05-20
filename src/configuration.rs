use config::Config;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};
use terminal_menu::{TerminalMenuItem, button, label, menu, mut_menu, run};

/// Contains all the settings related to the operation of Chip-8 VM.
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Chip8Settings {
    pub shift_quirk: bool,
    pub or_and_xor_quirk: bool,
    pub mem_quirk: bool,
    pub sprite_wrapping_quirk: bool,
    pub jump_offset_quirk: bool,
    pub execution_speed_multiple: f32,
    pub font_memory_starting_location: u16,
    pub program_folder_path: String,
}

/// Contains all the settings related to the interpreter window.
#[derive(Deserialize, Serialize)]
pub(crate) struct WindowSettings {
    pub width: u32,
    pub height: u32,
    pub background_color: [u8; 3],
    pub foreground_color: [u8; 3],
    pub fullscreen: bool,
    pub sprite_flicker_filter: bool,
    pub pixel_fade_micros: u64,
}

/// Contains all the settings related to sound.
#[derive(Deserialize, Serialize)]
pub(crate) struct SoundSettings {
    pub tone: f32,
    pub volume: f32,
}

/// A container that contains all the settings categories. Used for distribution of the appropriate
/// category of settings to each module.
#[derive(Deserialize, Serialize)]
pub(crate) struct Settings {
    pub chip8: Chip8Settings,
    pub window: WindowSettings,
    pub sound: SoundSettings,
}

impl Settings {
    /// Loads all the settings in settings.toml into a container and returns it.
    pub fn load() -> anyhow::Result<Self> {
        // If settings.toml does not exist in the project root directory, create it.
        if !fs::exists("settings.toml")? {
            Self::create_settings_file()?;
        }

        let settings = Config::builder()
            .add_source(config::File::with_name("settings"))
            .build()?;

        // ? operator is not possible here due to the Deserialize trait so ConfigError must be
        // manually mapped in this instance.
        let result = settings.try_deserialize();
        match result {
            Ok(settings) => Ok(settings),
            Err(config_error) => Err(anyhow::Error::msg(config_error.to_string())),
        }
    }

    /// Creates an populates settings.toml file
    fn create_settings_file() -> anyhow::Result<()> {
        #[rustfmt::skip]
        let settings_toml = toml::toml! {
            [chip8]
            shift_quirk = false
            or_and_xor_quirk = true
            mem_quirk = true
            sprite_wrapping_quirk = true
            jump_offset_quirk = false
            execution_speed_multiple = 1.0
            font_memory_starting_location = 0x050
            program_folder_path = "programs"

            [window]
            width = 768
            height = 384
            fullscreen = false
            background_color = [0, 0, 0]
            foreground_color = [255, 255, 255]
            sprite_flicker_filter = true
            pixel_fade_micros = (100)

            [sound]
            tone = 330.0
            volume = 0.5
        }.to_string();

        fs::write("settings.toml", settings_toml)?;

        Ok(())
    }
}

/// Spawns a menu with all the programs in their programs folder. Returns a result containing the
/// path to the selected program as a String. No idea how this will behave with symbolic links or
/// a program folder that is located outside the project root folder.
pub fn ask_for_program(settings: &Chip8Settings) -> anyhow::Result<PathBuf> {
    // Get the pathbufs of all the files in the program folder and put them in a Vec.
    let program_folder_path = Path::new(&settings.program_folder_path);

    // If the programs folder specified in setting.toml doesn't exit, create it.
    if !fs::exists(program_folder_path)? {
        fs::create_dir(program_folder_path)?;
    }

    let mut program_pathbufs: Vec<PathBuf> = fs::read_dir(program_folder_path)?
        .map(|result| result.map(|dir_entry| dir_entry.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    // Remove any files that are not Chip-8 programs from the list. This will show any hidden programs.
    program_pathbufs.retain(|path_buf| {
        path_buf
            .to_str()
            .is_some_and(|file_name| file_name.ends_with(".ch8"))
    });

    if program_pathbufs.is_empty() {
        panic!("No programs found in the folder specified in settings.toml.");
    }

    // Sort this collection so all the rest will be sorted too.
    program_pathbufs.sort();

    // Convert those pathbufs into &Paths.
    let program_paths: Vec<&Path> = program_pathbufs
        .iter()
        .map(|path_buf| path_buf.as_path())
        .collect::<Vec<&Path>>();

    // Convert those &Paths to &OsStrs
    let program_os_strs: Vec<&OsStr> = program_paths
        .iter()
        .map(|program_path| {
            program_path
                .file_name()
                .expect("No file exists at the provided program path.")
        })
        .collect();

    // Convert those &OsStrs to &strs for better readability in the menu.
    let program_names: Vec<&str> = program_os_strs
        .iter()
        .map(|program_os_str| {
            program_os_str
                .to_str()
                .expect("Could not convert program &OsStr to &str")
        })
        .map(|with_extension| with_extension.trim_end_matches(".ch8"))
        .collect();

    // Convert the program names into menu buttons.
    let mut program_buttons: Vec<TerminalMenuItem> = program_names
        .iter()
        .map(|program_name| button(program_name.to_string()))
        .collect();

    // Create the menu instructions labels.
    let mut menu_items: Vec<TerminalMenuItem> = vec![
        label("-----------------------------------------"),
        label("Select a program to run."),
        label("Use 'WASD' or arrow keys to navigate,"),
        label("enter to select, and 'Q' or esc to exit."),
        label("-----------------------------------------"),
    ];

    let number_of_labels = menu_items.len();

    // Combine all menu items into one Vec.
    menu_items.append(&mut program_buttons);

    // Create the menu with the program file names as buttons.
    let pick_program_menu = menu(menu_items);

    // Present the menu and le the user choose a program
    run(&pick_program_menu);

    if mut_menu(&pick_program_menu).canceled() {
        return Err(anyhow::Error::msg(
            "User exited the program selection menu.",
        ));
    }

    // Get the selected item index to use on the original Pathbuf Vec.
    let selection_index = mut_menu(&pick_program_menu).selected_item_index() - number_of_labels;

    // Convert the &Pathbuf from the Vec into an owned type (Pathbuf) so it can be passed back
    // to main(). Yes, path manipulation is terrible.
    Ok(program_pathbufs[selection_index].to_owned())
}
