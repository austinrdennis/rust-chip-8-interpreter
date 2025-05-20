use crate::chip8::VirtualMachine;
use sdl2::{EventPump, event::Event, keyboard::Scancode};
use std::time::{Duration, Instant};

const VALID_KEYS: std::ops::RangeInclusive<usize> = 0x0..=0xf;
pub const KEYUP_RELEASE_DURATION: Duration = Duration::from_millis(30);

/// Polls the keyboard for input events and passes it back to the caller wrapped in an Option.
pub fn poll_for_input(event_pump: &mut EventPump) -> Vec<Option<usize>> {
    let mut input_events = Vec::new();

    // Poll for both KeyDown and KeyUp events. Both are needed to detect a change in state of each
    // key for one of the Chip-8 VM functions as just knowing if a key is pressed or not isn't enough
    // information. The use of Scancodes over Keycodes is a subtle difference in what they represent.
    // At first glance, their functionality is seemingly identical, but Scancodes are the number
    // representing the physical button pushed where as Keycodes are the number representing the
    // symbol on that key. Using Scancodes allows keyboards with different layouts and symbols to be
    // used just like a QWERTY keyboard.
    for event in event_pump.poll_iter() {
        input_events.push(match event {
            Event::Quit { .. }
            | Event::KeyDown {
                scancode: Some(Scancode::Escape),
                ..
            } => Some(usize::MAX),
            Event::KeyDown {
                scancode: Some(Scancode::Return),
                ..
            } => Some(usize::MAX - 1),
            Event::KeyDown {
                scancode: Some(Scancode::Num1),
                ..
            } => Some(0x1),
            Event::KeyDown {
                scancode: Some(Scancode::Num2),
                ..
            } => Some(0x2),
            Event::KeyDown {
                scancode: Some(Scancode::Num3),
                ..
            } => Some(0x3),
            Event::KeyDown {
                scancode: Some(Scancode::Num4),
                ..
            } => Some(0xc),
            Event::KeyDown {
                scancode: Some(Scancode::Q),
                ..
            } => Some(0x4),
            Event::KeyDown {
                scancode: Some(Scancode::W),
                ..
            } => Some(0x5),
            Event::KeyDown {
                scancode: Some(Scancode::E),
                ..
            } => Some(0x6),
            Event::KeyDown {
                scancode: Some(Scancode::R),
                ..
            } => Some(0xd),
            Event::KeyDown {
                scancode: Some(Scancode::A),
                ..
            } => Some(0x7),
            Event::KeyDown {
                scancode: Some(Scancode::S),
                ..
            } => Some(0x8),
            Event::KeyDown {
                scancode: Some(Scancode::D),
                ..
            } => Some(0x9),
            Event::KeyDown {
                scancode: Some(Scancode::F),
                ..
            } => Some(0xe),
            Event::KeyDown {
                scancode: Some(Scancode::Z),
                ..
            } => Some(0xa),
            Event::KeyDown {
                scancode: Some(Scancode::X),
                ..
            } => Some(0x0),
            Event::KeyDown {
                scancode: Some(Scancode::C),
                ..
            } => Some(0xb),
            Event::KeyDown {
                scancode: Some(Scancode::V),
                ..
            } => Some(0xf),
            Event::KeyUp {
                scancode: Some(Scancode::Num1),
                ..
            } => Some(0x10),
            Event::KeyUp {
                scancode: Some(Scancode::Num2),
                ..
            } => Some(0x20),
            Event::KeyUp {
                scancode: Some(Scancode::Num3),
                ..
            } => Some(0x30),
            Event::KeyUp {
                scancode: Some(Scancode::Num4),
                ..
            } => Some(0xc0),
            Event::KeyUp {
                scancode: Some(Scancode::Q),
                ..
            } => Some(0x40),
            Event::KeyUp {
                scancode: Some(Scancode::W),
                ..
            } => Some(0x50),
            Event::KeyUp {
                scancode: Some(Scancode::E),
                ..
            } => Some(0x60),
            Event::KeyUp {
                scancode: Some(Scancode::R),
                ..
            } => Some(0xd0),
            Event::KeyUp {
                scancode: Some(Scancode::A),
                ..
            } => Some(0x70),
            Event::KeyUp {
                scancode: Some(Scancode::S),
                ..
            } => Some(0x80),
            Event::KeyUp {
                scancode: Some(Scancode::D),
                ..
            } => Some(0x90),
            Event::KeyUp {
                scancode: Some(Scancode::F),
                ..
            } => Some(0xe0),
            Event::KeyUp {
                scancode: Some(Scancode::Z),
                ..
            } => Some(0xa0),
            Event::KeyUp {
                scancode: Some(Scancode::X),
                ..
            } => Some(0x100),
            Event::KeyUp {
                scancode: Some(Scancode::C),
                ..
            } => Some(0xb0),
            Event::KeyUp {
                scancode: Some(Scancode::V),
                ..
            } => Some(0xf0),
            _ => None,
        });
    }

    input_events
}

/// Takes in an input event and sets the corresponding Chip-8 VM keypad value to pressed or not pressed.
pub fn set_keypad_value(
    vm: &mut VirtualMachine,
    input_event: usize,
    keypad_shadow_timers: &mut [Instant; 16],
) {
    let key_event: usize = input_event;

    match key_event {
        // Keydown
        0x0..=0xf => {
            let key_down = key_event;

            if VALID_KEYS.contains(&key_down) {
                vm.keypad[key_down] = true;
            }
        }
        //KeyUp
        _ => {
            // 0x100 is a special case to represent the 0 key up event.
            let key_up = if key_event == 0x100 {
                0x0
            } else {
                key_event >> 4
            };

            if VALID_KEYS.contains(&key_up) {
                vm.keypad[key_up] = false;
                vm.keypad_shadow[key_up] = true;
                // Start timing how long a key has been released for.
                keypad_shadow_timers[key_up] = Instant::now();
            }
        }
    }
}
