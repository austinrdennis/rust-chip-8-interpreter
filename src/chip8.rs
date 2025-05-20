use crate::{configuration::Chip8Settings, input_handler::KEYUP_RELEASE_DURATION};
use rand::random;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

/// At 60 FPS/Hz, the frame time budget is 16.67 milliseconds.
const MAX_FRAME_TIME: Duration = Duration::from_nanos(16_666_667);

/// Each character in the font is a sprite, which are is composed of 5 rows of 8 pixels. Each
/// sprite row can be represented by a single byte and then loaded row-by-row into memory. Each of
/// these sprites are 5 rows tall.
#[rustfmt::skip]
const FONT_DATA: [u8; 80] = [
    0xf0, 0x90, 0x90, 0x90, 0xf0, // 0               "0"  |  Binary  | Hex
    0x20, 0x60, 0x20, 0x20, 0x70, // 1              ------------------------
    0xf0, 0x10, 0xf0, 0x80, 0xf0, // 2              ****  | 11110000 | 0xf0
    0xf0, 0x10, 0xf0, 0x10, 0xf0, // 3              *  *  | 10010000 | 0x90
    0x90, 0x90, 0xf0, 0x10, 0x10, // 4              *  *  | 10010000 | 0x90
    0xf0, 0x80, 0xf0, 0x10, 0xf0, // 5              *  *  | 10010000 | 0x90
    0xf0, 0x80, 0xf0, 0x90, 0xf0, // 6              ****  | 11110000 | 0xf0
    0xf0, 0x10, 0x20, 0x40, 0x40, // 7
    0xf0, 0x90, 0xf0, 0x90, 0xf0, // 8             Each row represents a line
    0xf0, 0x90, 0xf0, 0x10, 0xf0, // 9             of 8 pixels on this 4x5
    0xf0, 0x90, 0xf0, 0x90, 0x90, // A             pixel font. Each digit is
    0xe0, 0x90, 0xe0, 0x90, 0xe0, // B             justified to the left of
    0xf0, 0x80, 0x80, 0x80, 0xf0, // C             the row. This makes sense
    0xe0, 0x90, 0x90, 0x90, 0xe0, // D             when drawing words left-to-right
    0xf0, 0x80, 0xf0, 0x80, 0xf0, // E             on the XOR pixel screen.
    0xf0, 0x80, 0xf0, 0x80, 0x80, // F
];

/// Representation of Chip-8 virtual machine.
pub struct VirtualMachine {
    /// VM working memory. Total address range: 0x000 to 0xfff.
    mem: [u8; 4096],
    /// General purpose registers V0 to VF. VF is used to set flags by operations and shouldn't be
    /// used by a program directly to store anything except flags, but this isn't a hard rule.
    v: [u8; 16],
    /// Index register. Used to point to locations in memory, not store data inside itself.
    i: u16,
    /// The VM's stack. A Vec<u16> was chosen for the convenience of calling the .push() and .pop()
    /// methods, but an array could have been used here instead for better resource efficiency.
    /// Using a vector also means a stack pointer is not needed.
    stack: Vec<u16>,
    /// Program counter. Points to the next opcode in memory.
    pc: u16,
    /// When set by a program to an arbitrary value (0-255), counts down to 0 at a rate of -60 per
    /// second. For general use by the a program.
    delay_timer: u8,
    /// When set by a program to an arbitrary value (0-255), counts down to 0 at a rate of -60 per
    /// second. Sounds buzzer while not 0. Used by programs to generate sound affects.
    pub sound_timer: u8,
    /// Keypad input register. Each bool represents the status of a different key.
    pub keypad: [bool; 16],
    /// A copy of the keypad input register to check for a change in state from pressed to released.
    pub keypad_shadow: [bool; 16],
    /// Frame buffer that totals 2048 pixels (64 x 32 resolution). Used to store state of each pixel
    /// so it can be rendered to the screen. There's a more efficient way of representing this
    /// (a 256 byte array), but it would require bit level encoding and decoding. This is a lot
    /// easier to work with and worth the 8x bigger memory footprint. The performance delta between
    /// the two methods is literally imperceptible to the user during gameplay.
    pub fb: [bool; 2048],
    /// Indicates the Chip-8 VM frame is done and it should be rendered to the virtual screen.
    pub draw_flag: bool,
    ///Starting locations for each character in the built-in font (0-F).
    font_locations: [u16; 16],
    /// Represents total time elapsed since the beginning of the current frame.
    frame_time: Duration,
    /// Settings for the Chip-8 VM as specified in settings.toml.
    settings: Chip8Settings,
}

impl VirtualMachine {
    /// Creates and returns a new instance of the Chip-8 virtual machine. Loads the built-in font
    /// into memory and opens a program file (ROM) and load it into memory at location 0x200.
    pub fn initialize(settings: &Chip8Settings, program_path: &Path) -> anyhow::Result<Self> {
        //-----------------------------------------------------------
        // Initialize memory and load built-in font
        //-----------------------------------------------------------
        // This starting address default is 0x050 and is arbitrary but it's popular convention. The
        // font data can exist anywhere between 0x000 and 0x1ff (inclusive) so long as it fits in
        // that range.
        let mut mem: [u8; 4096] = [0; 4096];
        let mut font_locations: [u16; 16] = [0; 16];
        let mut font_offset: u16 = settings.font_memory_starting_location;
        let mut char: usize = 0;

        for (iteration, byte) in FONT_DATA.iter().enumerate() {
            // Every 5 bytes is a complete sprite, so store that starting location for later use in
            // an operation.
            if iteration % 5 == 0 {
                font_locations[char] = font_offset;
                char += 1;
            }
            // Load each font byte into a continuous region of memory.
            mem[font_offset as usize] = *byte;
            font_offset += 0x001;
        }

        //-----------------------------------------------------------
        // Load the program into memory
        //-----------------------------------------------------------
        let mut program_offset: usize = 0x200;
        let program_data: Vec<u8> = fs::read(program_path)?;

        for bytes in program_data.iter() {
            mem[program_offset] = *bytes;
            program_offset += 0x01;
        }

        //-----------------------------------------------------------
        // Initialize the rest of the VirtualMachine and construct it
        //-----------------------------------------------------------
        Ok(Self {
            mem,
            v: [0; 16],
            i: 0,
            pc: 0x200,
            stack: Vec::with_capacity(16), // Will never be bigger than a size of 16.
            delay_timer: 0,
            sound_timer: 0,
            keypad: [false; 16],
            keypad_shadow: [false; 16],
            fb: [false; 2048],
            draw_flag: false,
            font_locations,
            frame_time: Duration::ZERO,
            // The lifetime annotations to borrow this are not not worth the squeeze. The performance
            // hit is so little, it's fine to just clone it into an owned type.
            settings: settings.clone(),
        })
    }

    /// Resets the Chip-8 VM. Trying to avoid allocating additional real machine memory whenever
    /// possible. This will not allow the user to select a new program.
    pub fn reset(&mut self) {
        // Clear frame buffer
        for pixel in self.fb.iter_mut() {
            *pixel = false;
        }

        // Clear the stack. Capacity of the stack stays the same so no new memory takes place.
        // Equivalent to calling self.stack.pop() in a loop until the vec is empty.
        self.stack.clear();

        // Clear V registers
        for byte in self.v.iter_mut() {
            *byte = 0;
        }

        // Clear I register
        self.i = 0;

        // Clear keypad arrays
        for key in self.keypad.iter_mut() {
            *key = false;
        }

        for key in self.keypad_shadow.iter_mut() {
            *key = false;
        }

        // Reset timers
        self.delay_timer = 0;
        self.sound_timer = 0;

        // Set program counter to program start address
        self.pc = 0x200;
    }

    /// Simulates one operation cycle (not clock cycle) of the Chip-8 VM.
    pub fn simulate_operation_cycle(
        &mut self,
        mol_start_time: &Instant,
        keypad_shadow_timers: &mut [Instant; 16],
    ) {
        let opcode = self.fetch_opcode();

        // This duration represents the average duration the operation would take on a real COSMIC
        // VIP system to get the execution timing right relative to other operations. The overall
        // execution speed can be adjusted with a multiple that gets applied to each of these
        // numbers.
        let cycle_duration = self.decode_opcode_and_execute_operation(opcode);

        // Update the frame time with how long the operation cycle took (simulated time) plus how
        // long since the start of the current frame (actual time).
        self.frame_time += cycle_duration.unwrap_or(Duration::ZERO);
        self.frame_time = self.frame_time.saturating_add(mol_start_time.elapsed());

        // After the release duration had passed for each key, set the key shadow of each to reflect
        // that state.
        for (key, pressed) in self.keypad_shadow.iter_mut().enumerate() {
            if *pressed
                && KEYUP_RELEASE_DURATION.saturating_sub(keypad_shadow_timers[key].elapsed())
                    == Duration::ZERO
            {
                *pressed = false;
            }
        }

        // Out of frame time budget, set everything up for the next frame and tell the virtual
        // screen to render the frame buffer.
        if self.frame_time > MAX_FRAME_TIME {
            if self.delay_timer > 0 {
                self.delay_timer -= 1;
            }
            if self.sound_timer > 0 {
                self.sound_timer -= 1;
            }

            self.draw_flag = true;
            self.frame_time = Duration::ZERO;
        }
    }

    /// Fetches the opcode bytes from the next two locations in memory, constructs the opcode from
    /// those bytes, and returns the complete opcode.
    fn fetch_opcode(&mut self) -> u16 {
        let opcode: u16 = if self.pc + 1 < (self.mem.len() as u16) {
            // The Chip-8 VM was written in big endian byte order and almost every modern computing
            // context uses little endian byte order so a byte swap on the first read byte is required.
            (self.mem[self.pc as usize] as u16).swap_bytes()
                | (self.mem[(self.pc as usize) + 1] as u16)
        } else {
            panic!(
                "Chip-8 VM program counter reached the end of memory and attempted to read another byte."
            );
        };

        opcode
    }

    /// Decodes the provided opcode and calls the appropriate operation function.
    fn decode_opcode_and_execute_operation(&mut self, opcode: u16) -> Option<Duration> {
        // Extract the operands from the opcodes to pass into the operation functions. This technique
        // is known as bit masking and it's going to be used a lot in this module.
        let n: u8 = (opcode & 0x000f) as u8;
        let nn: u8 = (opcode & 0x00ff) as u8;
        let nnn: u16 = opcode & 0x0fff;
        let x: usize = (opcode & 0x0f00).swap_bytes() as usize;
        let y: usize = ((opcode & 0x00f0) >> 4) as usize;

        // Match the opcode to an operation function. There is a more efficient way to do this
        // (using function pointers), but it's much more confusing to look at and performance is not
        // a problem here. I leave that as an exercise to the reader.
        match opcode & 0xf000 {
            0x0000 => match opcode & 0x00ff {
                0x0000 => self.call_routine(nnn),
                0x00e0 => self.clear_display(),
                0x00ee => self.subroutine_return(),
                _ => Self::invalid_operation(opcode),
            },
            0x1000 => self.jump_to_nnn(nnn),
            0x2000 => self.call_subroutine(nnn),
            0x3000 => self.skip_if_eq_nn(x, nn),
            0x4000 => self.skip_if_neq_nn(x, nn),
            0x5000 => self.skip_if_eq(x, y),
            0x6000 => self.set_vx_to_nn(x, nn),
            0x7000 => self.add_nn_to_vx(x, nn),
            0x8000 => match opcode & 0xf00f {
                0x8001 => self.or(x, y),
                0x8002 => self.and(x, y),
                0x8003 => self.xor(x, y),
                0x8004 => self.add(x, y),
                0x8005 => self.subtract_vy_from_vx(x, y),
                0x8006 => self.shift_right(x, y),
                0x8007 => self.subtract_vx_from_vy(x, y),
                0x800e => self.shift_left(x, y),
                _ => self.clone(x, y),
            },
            0x9000 => self.skip_if_neq(x, y),
            0xa000 => self.set_i_to_nnn(nnn),
            0xb000 => self.jump_to_v0_plus_nnn(nnn),
            0xc000 => self.random_and_nn(x, nn),
            0xd000 => self.draw_sprite(x, y, n),
            0xe000 => match opcode & 0xf0ff {
                0xe09e => self.skip_if_pressed(x),
                0xe0a1 => self.skip_if_not_pressed(x),
                _ => Self::invalid_operation(opcode),
            },
            0xf000 => match opcode & 0xf0ff {
                0xf007 => self.clone_dt_into_vx(x),
                0xf00a => self.store_keypress(x),
                0xf015 => self.set_delay_timer(x),
                0xf018 => self.set_sound_timer(x),
                0xf01e => self.add_vx_to_i(x),
                0xf029 => self.set_i_to_font_sprite_location(x),
                0xf033 => self.bcd_vx(x),
                0xf055 => self.dump_registers(x),
                0xf065 => self.load_registers(x),
                _ => Self::invalid_operation(opcode),
            },
            _ => Self::invalid_operation(opcode),
        }
    }

    //-----------------------------------------------
    // Operation Functions
    //-----------------------------------------------
    /// Panic as the VM has no idea what to do with an opcode that's not in the list.
    fn invalid_operation(opcode: u16) -> Option<Duration> {
        panic!("Chip-8 VM opcode '{:#06x}' not recognized.", opcode)
    }

    /// 0NNN: This instruction is only used on the old computers on which the Chip-8 VM was
    /// originally implemented. It is typically ignored by modern interpreters, including this one,
    /// but its signature is here for completeness and timing.
    fn call_routine(&mut self, _nnn: u16) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((100.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        // Does literally nothing.

        self.pc += 2;
        Some(op_duration)
    }

    /// 00E0: Clear the display (clears the frame buffer in this implementation).
    fn clear_display(&mut self) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((109.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        for pixel in self.fb.iter_mut() {
            *pixel = false;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 00EE: Return from a subroutine. Sets the program counter to the address at the top of the
    /// stack (the return address), then pops the return address off the stack and sets the program
    /// counter to the next instruction.
    fn subroutine_return(&mut self) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((105.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.pc = *self
            .stack
            .last()
            .expect("Failed to return from a Chip-8 subroutine because the stack is empty.");
        self.stack.pop();

        // This seems weird to do since this function just set the PC, but think of it as returning
        // to the where of the previous instruction left off. This progresses the program by getting
        // the next instruction. If this wasn't here, the program would do this operation repeatedly
        // until there were no more addresses on the stack and then the interpreter would crash.
        self.pc += 2;
        Some(op_duration)
    }

    /// 1NNN: Jump to address NNN. Sets the program counter to NNN.
    fn jump_to_nnn(&mut self, nnn: u16) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((105.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.pc = nnn;

        Some(op_duration)
    }

    /// 2NNN: Call subroutine at NNN. Pushes the value of the program counter onto the stack and
    /// then sets the program counter to nnn.
    fn call_subroutine(&mut self, nnn: u16) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((105.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.stack.push(self.pc);
        self.pc = nnn;

        Some(op_duration)
    }

    /// 3XNN: Skip next instruction if Vx == NN. Compares value of register Vx to NN, and if they
    /// are equal, increments the program counter by 2 (usually the next instruction is a jump to
    /// skip a code block).
    fn skip_if_eq_nn(&mut self, x: usize, nn: u8) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((61.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        if self.v[x] == nn {
            self.pc += 2;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 4XNN: Skip next instruction if Vx != NN. Compares value of register Vx to NN, and if they
    /// are equal, increments the program counter by 2 (usually the next instruction is a jump to
    /// skip a code block).
    fn skip_if_neq_nn(&mut self, x: usize, nn: u8) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((61.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        if self.v[x] != nn {
            self.pc += 2;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 5XY0: Skip next instruction if Vx == Vy. Compares value of register Vx to the value of
    /// register Vy and, if they are equal, increments the program counter by 2 (usually the next
    /// instruction is a jump to skip a code block).
    fn skip_if_eq(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((61.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        if self.v[x] == self.v[y] {
            self.pc += 2;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 6XNN: Set Vx to NN. Puts the value NN into register Vx.
    fn set_vx_to_nn(&mut self, x: usize, nn: u8) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((27.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.v[x] = nn;

        self.pc += 2;
        Some(op_duration)
    }

    /// 7XNN: Add NN to Vx. Adds NN to the value of register Vx, then stores the result in Vx
    /// (carry flag is not changed).
    fn add_nn_to_vx(&mut self, x: usize, nn: u8) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((45.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.v[x] = self.v[x].wrapping_add(nn);

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XY0: Clone Vy to Vx. Stores the value of register Vy in register Vx (the value of Vy
    /// remains unchanged).
    fn clone(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((45.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.v[x] = self.v[y];

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XY1: Set Vx to Vx OR Vy. Performs a bitwise OR on the values of Vx and Vy, then stores
    /// the result in Vx. Quirk: Reset the carry flag to zero after the operation.
    fn or(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((200.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.v[x] |= self.v[y];

        if self.settings.or_and_xor_quirk {
            self.v[0xf] = 0;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XY2: Set Vx to Vx AND Vy. Performs a bitwise AND on the values of Vx and Vy, then stores
    /// the result in Vx. Quirk: Reset the carry flag to zero after the operation.
    fn and(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((200.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.v[x] &= self.v[y];

        if self.settings.or_and_xor_quirk {
            self.v[0xf] = 0;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XY3: Set Vx to Vx XOR Vy. Performs a bitwise XOR on the values of Vx and Vy, then stores
    /// the result in Vx. Quirk: Reset the carry flag to zero after the operation.
    fn xor(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((200.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.v[x] ^= self.v[y];

        if self.settings.or_and_xor_quirk {
            self.v[0xf] = 0;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XY4: Set Vx = Vx + Vy and set VF = carry. The values of Vx and Vy are added together.
    /// If the addition results in an overflow (i.e. > 255), VF is set to 1 and otherwise it's set
    /// to 0.
    fn add(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((45.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let result: (u8, bool) = self.v[x].overflowing_add(self.v[y]);

        self.v[x] = result.0;

        if result.1 {
            self.v[0xf] = 1;
        } else {
            self.v[0xf] = 0;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XY5: Set Vx = Vx - Vy and set VF = !borrow. Vy is subtracted from Vx and the results
    /// stored in Vx. If the subtraction results in an underflow, then VF is set to 0 otherwise
    /// VF is set to 1 (opposite of what you expect).
    fn subtract_vy_from_vx(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((200.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let result: (u8, bool) = self.v[x].overflowing_sub(self.v[y]);

        self.v[x] = result.0;

        if result.1 {
            self.v[0xf] = 0;
        } else {
            self.v[0xf] = 1;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XY6: Set Vx = Vy and then set Vx = Vx bit shifted right by 1. If the least-significant bit
    /// of Vx is 1, then VF is set to 1, otherwise it's set to 0. Then Vx is shifted right by 1.
    /// Quirk: Ignore Vy and just shift the contents of Vx as is.
    fn shift_right(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((200.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        if !self.settings.shift_quirk {
            self.v[x] = self.v[y];
        }

        let bit_shifted_out: u8 = self.v[x] & 1;
        self.v[x] = self.v[x].wrapping_shr(1);

        self.v[0xf] = bit_shifted_out;

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XY7: Set Vx = Vy - Vx and set VF = !borrow. Vx is subtracted from Vy and the result is
    /// stored in Vx. If the subtraction results in an underflow, then VF is set to 0 otherwise
    /// VF is set to 1 (opposite of what you expect).
    fn subtract_vx_from_vy(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((200.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let result: (u8, bool) = self.v[y].overflowing_sub(self.v[x]);

        self.v[x] = result.0;

        if result.1 {
            self.v[0xf] = 0;
        } else {
            self.v[0xf] = 1;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// 8XYE: Set Vx = Vy and then set Vx = Vx bit shifted left by 1. If the most-significant bit
    /// of Vx is 1, then VF is set to 1, it's set to 0. Then Vx is shifted left by 1.
    /// Quirk: Ignore Vy and just shift the contents of Vx as is.
    fn shift_left(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((200.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }
        if !self.settings.shift_quirk {
            self.v[x] = self.v[y];
        }

        let bit_shifted_out: u8 = (self.v[x] & 0b10000000) >> 7;
        self.v[x] = self.v[x].wrapping_shl(1);

        self.v[0xf] = bit_shifted_out;

        self.pc += 2;
        Some(op_duration)
    }

    /// 9XY0: Skip next instruction if Vx != Vy. Compares value of register Vx to the value of
    /// register Vy and, if they are not equal, increments the program counter by 2 (usually
    /// the next instruction is a jump to skip a code block).
    fn skip_if_neq(&mut self, x: usize, y: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((61.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        if self.v[x] != self.v[y] {
            self.pc += 2;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// ANNN: Set I = nnn. The value of register I is set to nnn.
    fn set_i_to_nnn(&mut self, nnn: u16) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((55.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.i = nnn;

        self.pc += 2;
        Some(op_duration)
    }

    /// BNNN: Jump to location NNN + V0. The program counter is set to NNN plus the value of V0.
    /// Quirk: The program counter is set to NNN plus the value of Vx where x is the most
    /// significant digit in NNN (ie. XNN) instead of V0.
    fn jump_to_v0_plus_nnn(&mut self, nnn: u16) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((105.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        if self.settings.jump_offset_quirk {
            let x = (nnn & 0xf00).swap_bytes() as usize;
            self.pc = nnn.wrapping_add(self.v[x] as u16);
        } else {
            self.pc = nnn.wrapping_add(self.v[0] as u16);
        }

        Some(op_duration)
    }

    /// CXNN: Set Vx = random byte AND NN. Generates a random number from 0 to 255 inclusive, which
    /// is then bitwise ANDed with the value NN. The results are stored in Vx.
    fn random_and_nn(&mut self, x: usize, nn: u8) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((164.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.v[x] = nn & random::<u8>();

        self.pc += 2;
        Some(op_duration)
    }

    /// DXYN: Display N height sprite starting at memory location I at (Vx, Vy), set VF = collision.
    /// Reads N bytes from memory, starting at the address stored in I. These bytes are then
    /// displayed as sprites on screen at coordinates (Vx, Vy) that has a width of 8 pixels and a
    /// height of N pixels. Each row of 8 pixels is read as a bit-coded byte starting from memory
    /// location I (I value does not change). Sprites are XORed onto the existing screen. If this
    /// causes any pixels to be erased, VF is set to 1, otherwise it is set to 0.
    /// Quirk: If the sprite's starting position outside the coordinates of the display, it wraps
    /// around to the opposite side of the screen. Sprites themselves don't wrap once they begin
    /// to be drawn, but the starting of the sprite point wraps before drawing begins.
    fn draw_sprite(&mut self, x: usize, y: usize, n: u8) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((10_734.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let i = self.i as usize;
        let mut sprite_row: u8;
        let mut sprite_pixel: u8;
        let mut fb_pixel: bool;
        let mut fb_pixel_index: usize;
        let mut collision = false;
        let mut x = self.v[x] as usize;
        let mut y = self.v[y] as usize;

        if self.settings.sprite_wrapping_quirk {
            // The modulo operator (%) is used on the x and y coordinates from Vx and Vy to properly
            // wrap the starting values inside the bounds of the screen.
            x %= 64;
            y %= 32;
        }

        // Iterate over each row in a sprite
        'get_sprite_rows: for current_row in 0..n as usize {
            // If next sprite row would be drawn off the bottom of the screen, stop drawing sprite.
            if y + current_row > 31 {
                break 'get_sprite_rows;
            }

            // Sprite bytes are stored in big endian so their bits have to be reversed for modern
            // computers
            sprite_row = self.mem[i + current_row].reverse_bits();

            // Iterate over each bit (pixel) in a row
            'set_fb_pixel: for current_pixel in 0..8 {
                // If next sprite pixel in row would be drawn off the right of the screen, stop
                // drawing this row and move on to the next.
                if x + current_pixel > 63 {
                    break 'set_fb_pixel;
                }

                // Get the value of each pixel in the sprite and frame buffer.
                fb_pixel_index = (y + current_row) * 64 + (x + current_pixel);
                fb_pixel = self.fb[fb_pixel_index];
                sprite_pixel = (sprite_row >> current_pixel) % 2;

                // This is effectively an XOR operation on the frame buffer pixel with the sprite
                // pixel. A collision is if the frame buffer pixel turns off as result of the XOR
                // operation.
                if sprite_pixel == 1 {
                    match fb_pixel {
                        false => {
                            self.fb[fb_pixel_index] = true;
                        }

                        true => {
                            self.fb[fb_pixel_index] = false;
                            collision = true;
                        }
                    }
                }
            }
        }

        // If any collision occurred during the drawing of the sprite, it is indicated in the flag
        // register.
        if collision {
            self.v[0xf] = 1;
        } else {
            self.v[0xf] = 0;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// EX9E: Skip next instruction if key with the value of Vx is pressed at time of check. Checks
    /// the keyboard, and if the key corresponding to the value of Vx (only considering the lowest
    /// nibble) is currently in the down position, program counter is increased by 2.
    fn skip_if_pressed(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((73.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let key = (self.v[x] & 0x000f) as usize;

        if self.keypad[key] {
            self.pc += 2;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// EXA1: Skip next instruction if key with the value of Vx is not pressed at time of check.
    /// Checks the keyboard, and if the key corresponding to the value of Vx (only considering
    /// the lowest nibble) is currently in the up position, program counter is increased by 2.
    fn skip_if_not_pressed(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((73.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let key = (self.v[x] & 0x000f) as usize;

        if !self.keypad[key] {
            self.pc += 2;
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// FX07: Set Vx = delay timer value.
    fn clone_dt_into_vx(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((27.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.v[x] = self.delay_timer;

        self.pc += 2;
        Some(op_duration)
    }

    /// FX0A: Wait for a key press, store which key is pressed in Vx. All execution stops (delay
    /// and sound timers continue processing) until a key is pressed and then released.
    fn store_keypress(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((200.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        for (key, pressed) in self.keypad_shadow.iter().enumerate() {
            if *pressed && !self.keypad[key] {
                self.v[x] = key as u8;
                self.pc += 2;
                return Some(op_duration); // Return early to allow program execution to continue.
            }
        }
        Some(op_duration)
    }

    /// FX15: Set delay timer = Vx.
    fn set_delay_timer(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((45.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.delay_timer = self.v[x];

        self.pc += 2;
        Some(op_duration)
    }

    /// FX18: Set sound timer = Vx.
    fn set_sound_timer(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((45.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.sound_timer = self.v[x];

        self.pc += 2;
        Some(op_duration)
    }

    /// FX1E: Set I = I + Vx.
    fn add_vx_to_i(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((86.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        self.i = self.i.wrapping_add(self.v[x] as u16);

        self.pc += 2;
        Some(op_duration)
    }

    /// FX29: Set I to the memory location in of the sprite representing the character in Vx (only
    /// considering the lowest nibble).
    fn set_i_to_font_sprite_location(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((91.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let font_char: u8 = self.v[x] & 0x0f;
        self.i = self.font_locations[font_char as usize];

        self.pc += 2;
        Some(op_duration)
    }

    /// FX33: Store binary-coded decimal (BCD) representation of Vx in memory locations I (hundreds
    /// digit), I+1(tens digit), and I+2 (ones digit).
    fn bcd_vx(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((927.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let i = self.i as usize;
        self.mem[i] = self.v[x] / 100;
        self.mem[i + 1] = (self.v[x] / 10) % 10;
        self.mem[i + 2] = (self.v[x] % 100) % 10;

        self.pc += 2;
        Some(op_duration)
    }

    /// FX55: Store registers V0 through Vx (inclusive) in memory starting at the location in I.
    /// The offset from I is increased by 1 for each value written, but I itself is left unmodified.
    /// Quirk: VI is also increased by 1 for each register stored and the final value of VI is
    /// V[i] + x + 1.
    fn dump_registers(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((605.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let mut register: usize = 0;
        let mut i_offset = self.i as usize;

        while x >= register {
            self.mem[i_offset] = self.v[register];
            register += 1;
            i_offset += 1;

            if self.settings.mem_quirk {
                self.i += 1;
            }
        }

        self.pc += 2;
        Some(op_duration)
    }

    /// FX65: Fill registers V0 through Vx (inclusive) from memory starting at the location in I.
    /// The offset from I is increased by 1 for each value read, but I itself is left unmodified.
    /// Quirk: VI is also increased by 1 for each register stored and the final value of VI is
    /// V[i] + x + 1.
    fn load_registers(&mut self, x: usize) -> Option<Duration> {
        let op_duration =
            Duration::from_micros((605.0 * self.settings.execution_speed_multiple) as u64);
        if self.frame_time.saturating_add(op_duration) > MAX_FRAME_TIME {
            return None;
        }

        let mut register: usize = 0;
        let mut i_offset = self.i as usize;

        while x >= register {
            self.v[register] = self.mem[i_offset];
            register += 1;
            i_offset += 1;

            if self.settings.mem_quirk {
                self.i += 1;
            }
        }

        self.pc += 2;
        Some(op_duration)
    }
}
