use std::io::Cursor;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use image::RgbaImage;
use rodio::{Decoder, OutputStream, Sink, Source};
use softbuffer::Surface;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorIcon, Fullscreen, ResizeDirection, Window, WindowId};

const IMAGE_BYTES: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../unnamed.png"));
const MUSIC_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../orbit-d0d-main-version-29627-02-39.mp3"
));
const _MUSIC_LICENSE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../music.md"));

const BORDER: f64 = 8.0;
const DOUBLE_CLICK_MS: u64 = 400;
const INITIAL_SCALE: u32 = 1;
const INSET_BASE: f32 = 8.0;

fn hit_test(pos: PhysicalPosition<f64>, size: PhysicalSize<u32>) -> Option<ResizeDirection> {
    let w = size.width as f64;
    let h = size.height as f64;
    let left = pos.x < BORDER;
    let right = pos.x > w - BORDER;
    let top = pos.y < BORDER;
    let bottom = pos.y > h - BORDER;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeDirection::NorthWest),
        (_, true, true, _) => Some(ResizeDirection::NorthEast),
        (true, _, _, true) => Some(ResizeDirection::SouthWest),
        (_, true, _, true) => Some(ResizeDirection::SouthEast),
        (true, _, _, _) => Some(ResizeDirection::West),
        (_, true, _, _) => Some(ResizeDirection::East),
        (_, _, true, _) => Some(ResizeDirection::North),
        (_, _, _, true) => Some(ResizeDirection::South),
        _ => None,
    }
}

fn cursor_for(dir: ResizeDirection) -> CursorIcon {
    match dir {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::NsResize,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::EwResize,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::NeswResize,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::NwseResize,
    }
}

struct App {
    window: Option<Arc<Window>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    image: RgbaImage,
    img_width: u32,
    img_height: u32,
    last_click: Option<Instant>,
    pre_expand_size: Option<PhysicalSize<u32>>,
    expanded: bool,
    mouse_pos: PhysicalPosition<f64>,
    _stream: Option<OutputStream>,
    _sink: Option<Sink>,
}

impl App {
    fn new() -> Self {
        let img = image::load_from_memory(IMAGE_BYTES).expect("embedded image is valid");
        let img_width = img.width();
        let img_height = img.height();
        let image = img.to_rgba8();
        Self {
            window: None,
            context: None,
            surface: None,
            image,
            img_width,
            img_height,
            last_click: None,
            pre_expand_size: None,
            expanded: false,
            mouse_pos: PhysicalPosition::new(0.0, 0.0),
            _stream: None,
            _sink: None,
        }
    }

    fn render(&mut self) {
        let Some(window) = &self.window else { return };
        let Some(surface) = &mut self.surface else { return };
        let size = window.inner_size();
        let width = size.width;
        let height = size.height;
        if width == 0 || height == 0 {
            return;
        }

        let _ = surface.resize(
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );
        let Ok(mut buf) = surface.buffer_mut() else { return };

        buf.fill(0xFF000000u32);

        let win_scale = (width as f32 / (self.img_width * INITIAL_SCALE) as f32)
            .min(height as f32 / (self.img_height * INITIAL_SCALE) as f32);
        let inset = (INSET_BASE * win_scale).max(1.0) as u32;
        let avail_w = width.saturating_sub(2 * inset);
        let avail_h = height.saturating_sub(2 * inset);

        let scale = (avail_w as f32 / self.img_width as f32)
            .min(avail_h as f32 / self.img_height as f32);
        let draw_w = (self.img_width as f32 * scale) as u32;
        let draw_h = (self.img_height as f32 * scale) as u32;
        let off_x = inset + (avail_w - draw_w) / 2;
        let off_y = inset + (avail_h - draw_h) / 2;

        for dy in 0..draw_h {
            for dx in 0..draw_w {
                let src_x = ((dx as f32 / scale) as u32).min(self.img_width - 1);
                let src_y = ((dy as f32 / scale) as u32).min(self.img_height - 1);
                let [r, g, b, a] = self.image.get_pixel(src_x, src_y).0;
                let dst_x = off_x + dx;
                let dst_y = off_y + dy;
                if dst_x < width && dst_y < height {
                    let idx = (dst_y * width + dst_x) as usize;
                    let r = r as u32 * a as u32 / 255;
                    let g = g as u32 * a as u32 / 255;
                    let b = b as u32 * a as u32 / 255;
                    buf[idx] = 0xFF000000 | (r << 16) | (g << 8) | b;
                }
            }
        }

        let _ = buf.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let init_w = self.img_width + INSET_BASE as u32 * 2;
        let init_h = self.img_height + INSET_BASE as u32 * 2;

        let attrs = Window::default_attributes()
            .with_title("unnamed")
            .with_inner_size(PhysicalSize::new(init_w, init_h))
            .with_min_inner_size(PhysicalSize::new(self.img_width, self.img_height))
            .with_decorations(false)
            .with_resizable(true);

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(Arc::clone(&window)).unwrap();
        let surface = softbuffer::Surface::new(&context, Arc::clone(&window)).unwrap();

        if let Ok((stream, handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&handle) {
                if let Ok(source) = Decoder::new(Cursor::new(MUSIC_BYTES)) {
                    sink.set_volume(0.5);
                    sink.append(source.repeat_infinite());
                    self._stream = Some(stream);
                    self._sink = Some(sink);
                }
            }
        }

        self.context = Some(context);
        self.surface = Some(surface);
        self.window = Some(window);
        self.render();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(_) | WindowEvent::RedrawRequested => self.render(),

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = position;
                let window = self.window.as_ref().unwrap();
                let cursor = hit_test(position, window.inner_size())
                    .map(cursor_for)
                    .unwrap_or(CursorIcon::Default);
                window.set_cursor(cursor);
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let now = Instant::now();
                let double = self
                    .last_click
                    .map(|t| now.duration_since(t) < Duration::from_millis(DOUBLE_CLICK_MS))
                    .unwrap_or(false);
                self.last_click = Some(now);

                let window = self.window.as_ref().unwrap();
                if double {
                    if self.expanded {
                        window.set_fullscreen(None);
                        if let Some(sz) = self.pre_expand_size {
                            let _ = window.request_inner_size(sz);
                        }
                        self.expanded = false;
                    } else {
                        self.pre_expand_size = Some(window.inner_size());
                        window.set_fullscreen(Some(Fullscreen::Borderless(
                            window.current_monitor(),
                        )));
                        self.expanded = true;
                    }
                } else if let Some(dir) = hit_test(self.mouse_pos, window.inner_size()) {
                    let _ = window.drag_resize_window(dir);
                } else {
                    let _ = window.drag_window();
                }
            }

            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
