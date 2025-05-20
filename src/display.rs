use crate::{chip8::VirtualMachine, configuration::WindowSettings};
use lerp::Lerp;
use sdl2::{
    EventPump, Sdl,
    pixels::Color,
    rect::Point,
    render::Canvas,
    video::{FullscreenType::*, Window},
};
use std::time::{Duration, Instant};

pub struct VirtualScreen {
    pub canvas: Canvas<Window>,
    pub event_pump: EventPump,
    background_color: Color,
    foreground_color: Color,
    fading_pixels: [Duration; 2048],
    pixel_fade_duration: Duration,
}

impl VirtualScreen {
    pub fn initialize(
        sdl_context: &Sdl,
        title: &str,
        settings: &WindowSettings,
    ) -> anyhow::Result<Self> {
        let background_color: Color = Color::RGB(
            settings.background_color[0],
            settings.background_color[1],
            settings.background_color[2],
        );
        let foreground_color: Color = Color::RGB(
            settings.foreground_color[0],
            settings.foreground_color[1],
            settings.foreground_color[2],
        );

        let event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;
        let video_subsystem = sdl_context.video().map_err(anyhow::Error::msg)?;
        let mut window = video_subsystem
            .window(title, settings.width, settings.height)
            .position_centered()
            .build()?;

        if settings.fullscreen {
            window.set_fullscreen(Desktop).map_err(anyhow::Error::msg)?;
        }

        let mut canvas = window.into_canvas().present_vsync().build()?;

        // Set the canvas to the same size as Chip-8 VM frame buffer
        canvas.set_logical_size(64, 32)?;

        canvas.set_draw_color(Color::BLACK);
        canvas.clear();

        Ok(Self {
            canvas,
            event_pump,
            background_color,
            foreground_color,
            fading_pixels: [Duration::ZERO; 2048],
            pixel_fade_duration: Duration::from_micros(settings.pixel_fade_micros),
        })
    }

    /// Renders the CHip-8 VM frame buffer to the SDL2 canvas pixel-by-pixel
    pub fn render_chip_8_frame(
        &mut self,
        vm: &VirtualMachine,
        mol_start_time: &Instant,
        settings: &WindowSettings,
    ) -> Result<(), String> {
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut current_pixel: Point;

        for (screen_location, buffer_pixel_on) in vm.fb.iter().enumerate() {
            // This is actually faster than using .offset() on an existing point
            current_pixel = Point::new(x, y);

            //Draw pixels to screen
            if *buffer_pixel_on {
                // Draw pixel as on foreground color
                self.canvas.set_draw_color(self.foreground_color);
                self.canvas.draw_point(current_pixel)?;
                if settings.sprite_flicker_filter {
                    self.fading_pixels[screen_location] = self.pixel_fade_duration;
                }
            } else if
            // Draw pixels with anti-flicker feature by blending previously on pixels towards
            // background color
            self.fading_pixels[screen_location] > Duration::ZERO
                && settings.sprite_flicker_filter
            {
                self.fading_pixels[screen_location] =
                    self.fading_pixels[screen_location].saturating_sub(mol_start_time.elapsed());

                let ratio = (self.fading_pixels[screen_location].as_micros()
                    / self.pixel_fade_duration.as_micros()) as f32;
                let r = (self.foreground_color.r as f32)
                    .lerp_bounded(self.background_color.r as f32, ratio)
                    as u8;
                let g = (self.foreground_color.g as f32)
                    .lerp_bounded(self.background_color.g as f32, ratio)
                    as u8;
                let b = (self.foreground_color.b as f32)
                    .lerp_bounded(self.background_color.b as f32, ratio)
                    as u8;
                let fade_color: Color = Color::RGB(r, g, b);

                self.canvas.set_draw_color(fade_color);
                self.canvas.draw_point(current_pixel)?;
            } else {
                // Draw fully off pixels as background color
                self.canvas.set_draw_color(self.background_color);
                self.canvas.draw_point(current_pixel)?;
            }

            match (screen_location + 1) % 64 {
                0 => {
                    x = 0;
                    y += 1;
                }
                _ => {
                    x += 1;
                }
            }
        }

        // Present the new render to the application window so the player actually sees it
        self.canvas.present();
        Ok(())
    }
}
