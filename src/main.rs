mod audio_handler;
mod chip8;
mod configuration;
mod display;
mod input_handler;

use audio_handler::*;
use chip8::VirtualMachine;
use configuration::*;
use display::VirtualScreen;
use input_handler as IH;
use std::time::Instant;

const QUIT: usize = usize::MAX;
const RESET: usize = usize::MAX - 1;

fn main() -> anyhow::Result<()> {
    // An array of Instants that represent when each key changed state from pressed to released.
    let mut keypad_shadow_timers: [Instant; 16] = [Instant::now(); 16];

    // Setup all user settings.
    let settings = Settings::load()?;

    // Get the path of program the user selected so it can be passed to the Chip-8 VM to load.
    let program_pathbuf = configuration::ask_for_program(&settings.chip8)?;
    let program_path = program_pathbuf.as_path();

    // Initialize everything needed to run the Main Operating Loop (MOL).
    let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
    let buzzer = Buzzer::initialize(&sdl_context, &settings.sound).map_err(anyhow::Error::msg)?;
    let mut vs = VirtualScreen::initialize(&sdl_context, "Chip 8", &settings.window)?;
    let mut vm =
        VirtualMachine::initialize(&settings.chip8, program_path).map_err(anyhow::Error::msg)?;

    // Main Operating Loop (MOL). This will run until the user either hits the window close button
    // or presses the Quit key as specified in the input handler.
    'MOL: loop {
        // Get the time at the start of the loop for frame time calculations inside the Chip-8 VM
        let mol_start_time = Instant::now();

        // Get input events
        let input_events = IH::poll_for_input(&mut vs.event_pump);
        for event in input_events.iter() {
            match event {
                Some(QUIT) => {
                    break 'MOL;
                }
                Some(RESET) => vm.reset(),
                // Whatever remaining event picked up by the input handler must be a keypad key
                Some(key) => IH::set_keypad_value(&mut vm, *key, &mut keypad_shadow_timers),
                None => (),
            }
        }

        // Simulate the Chip-8 VM for a single operation cycle
        vm.simulate_operation_cycle(&mol_start_time, &mut keypad_shadow_timers);

        // Play or pause the buzzer as appropriate
        if vm.sound_timer > 0 {
            buzzer.resume();
        } else {
            buzzer.pause();
        }

        // Update VS with Chip-8 VM frame buffer data
        if vm.draw_flag {
            vs.render_chip_8_frame(&vm, &mol_start_time, &settings.window)
                .map_err(anyhow::Error::msg)?;
            vm.draw_flag = false;
        }
    }

    Ok(())
}
