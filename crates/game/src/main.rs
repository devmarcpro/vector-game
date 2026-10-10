use std::{num::NonZeroU32, rc::Rc, time::Instant};
use vector_core::{Mat3, Vec3};
use vector_render::{Camera, Framebuffer, Wireframe};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

const GREEN: u32 = 0x0030_E050;
const DIM: u32 = 0x0010_6020;
const SIM_DT: f64 = 1.0 / 30.0;

struct State {
    t: f64,
}

fn draw(fb: &mut Framebuffer, st: &State) {
    fb.clear();
    let cam = Camera {
        pos: Vec3::new(0.0, 0.0, -6.0),
        rot: Mat3::IDENTITY,
        fov_y: 1.0,
    };
    let rot = Mat3::rot_y(st.t).mul(&Mat3::rot_x(st.t * 0.6));
    Wireframe::cube(1.5).draw(&cam, fb, &rot, Vec3::ZERO, GREEN);
    let (c, r) = (fb.cols(), fb.rows());
    fb.text(1, 1, "VECTOR  M0 DEMO TECHNIQUE", GREEN);
    fb.text(1, 3, &format!("T={:7.2}  CUBE WIREFRAME", st.t), DIM);
    let gauge = format!("ENERGIE [{}{}]", "#".repeat(7), "-".repeat(3));
    fb.text(1, r - 2, &gauge, GREEN);
    fb.text(c - 14, r - 2, "ECHAP=QUITTER", DIM);
}

struct App {
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    fb: Framebuffer,
    st: State,
    last: Instant,
    acc: f64,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("VECTOR")
            .with_inner_size(winit::dpi::LogicalSize::new(960, 540));
        let Ok(w) = el.create_window(attrs) else {
            el.exit();
            return;
        };
        let w = Rc::new(w);
        let Ok(ctx) = softbuffer::Context::new(w.clone()) else {
            el.exit();
            return;
        };
        self.surface = softbuffer::Surface::new(&ctx, w.clone()).ok();
        w.request_redraw();
        self.window = Some(w);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                el.exit()
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                self.acc += (now - self.last).as_secs_f64().min(0.25);
                self.last = now;
                while self.acc >= SIM_DT {
                    self.st.t += SIM_DT;
                    self.acc -= SIM_DT;
                }
                let (Some(win), Some(surf)) = (&self.window, &mut self.surface) else {
                    return;
                };
                let size = win.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                if self.fb.w != w.get() as usize || self.fb.h != h.get() as usize {
                    self.fb = Framebuffer::new(w.get() as usize, h.get() as usize);
                }
                draw(&mut self.fb, &self.st);
                surf.resize(w, h).ok();
                if let Ok(mut buf) = surf.buffer_mut() {
                    buf.copy_from_slice(&self.fb.px);
                    buf.present().ok();
                }
                win.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        let path = args
            .get(i + 1)
            .map(String::as_str)
            .unwrap_or("snapshot.png");
        let mut fb = Framebuffer::new(960, 540);
        draw(&mut fb, &State { t: 0.8 });
        if let Err(e) = std::fs::write(path, vector_render::png::encode(fb.w, fb.h, &fb.px)) {
            eprintln!("write failed: {e}");
        }
        println!("snapshot -> {path}");
        return;
    }
    let Ok(el) = EventLoop::new() else {
        eprintln!("cannot create event loop");
        return;
    };
    let mut app = App {
        window: None,
        surface: None,
        fb: Framebuffer::new(960, 540),
        st: State { t: 0.0 },
        last: Instant::now(),
        acc: 0.0,
    };
    if let Err(e) = el.run_app(&mut app) {
        eprintln!("run failed: {e}");
    }
}
