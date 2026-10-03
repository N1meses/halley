//! Client compatibility requests through the production upstream dispatcher.
#[path = "../src/wayland/dispatch.rs"]
mod upstream_protocols;

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{thread, time::Duration};

use smithay::reexports::wayland_server::{Display, protocol::wl_surface::WlSurface};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    BufferAssignment, CompositorClientState, CompositorHandler, CompositorState, SurfaceAttributes,
    with_states,
};
use smithay::wayland::single_pixel_buffer::{SinglePixelBufferState, get_single_pixel_buffer};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_registry, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols::wp::single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1 as pixel;

#[derive(Default)]
struct Observations {
    rgba: Option<[u32; 4]>,
    destroyed_buffers: usize,
}

struct Server {
    compositor: CompositorState,
    observations: Arc<Mutex<Observations>>,
}

#[derive(Default)]
struct ClientData(CompositorClientState);
impl smithay::reexports::wayland_server::backend::ClientData for ClientData {}

impl CompositorHandler for Server {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }
    fn client_compositor_state<'a>(
        &self,
        client: &'a smithay::reexports::wayland_server::Client,
    ) -> &'a CompositorClientState {
        &client.get_data::<ClientData>().unwrap().0
    }
    fn commit(&mut self, surface: &WlSurface) {
        with_states(surface, |states| {
            let mut attributes = states.cached_state.get::<SurfaceAttributes>();
            if let Some(BufferAssignment::NewBuffer(buffer)) = attributes.current().buffer.as_ref()
                && let Ok(pixel) = get_single_pixel_buffer(buffer)
            {
                self.observations.lock().unwrap().rgba = Some([pixel.r, pixel.g, pixel.b, pixel.a]);
            }
        });
    }
}
impl BufferHandler for Server {
    fn buffer_destroyed(
        &mut self,
        _: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    ) {
        self.observations.lock().unwrap().destroyed_buffers += 1;
    }
}
upstream_protocols::delegate_upstream_protocols!(Server);

#[derive(Default)]
struct Client {
    globals: HashMap<String, (u32, u32)>,
}
impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            state.globals.insert(interface, (name, version));
        }
    }
}
delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_buffer::WlBuffer);
delegate_noop!(Client: ignore pixel::WpSinglePixelBufferManagerV1);

struct Fixture {
    observations: Arc<Mutex<Observations>>,
    state: Client,
    queue: EventQueue<Client>,
    registry: wl_registry::WlRegistry,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let (client_socket, server_socket) = UnixStream::pair().unwrap();
        let mut display = Display::<Server>::new().unwrap();
        let mut dh = display.handle();
        let compositor = CompositorState::new::<Server>(&dh);
        let _pixels = SinglePixelBufferState::new::<Server>(&dh);
        dh.insert_client(server_socket, Arc::new(ClientData::default()))
            .unwrap();
        let observations = Arc::new(Mutex::new(Observations::default()));
        let mut server = Server {
            compositor,
            observations: observations.clone(),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                display.dispatch_clients(&mut server).unwrap();
                display.flush_clients().unwrap();
                thread::sleep(Duration::from_millis(1));
            }
        });
        let connection = Connection::from_socket(client_socket).unwrap();
        let mut queue = connection.new_event_queue();
        let registry = connection.display().get_registry(&queue.handle(), ());
        let mut state = Client::default();
        queue.roundtrip(&mut state).unwrap();
        Self {
            observations,
            state,
            queue,
            registry,
            stop,
            worker: Some(worker),
        }
    }
    fn sync(&mut self) {
        self.queue.roundtrip(&mut self.state).unwrap();
    }
    fn surface(&self) -> wl_surface::WlSurface {
        let compositor: wl_compositor::WlCompositor = self.registry.bind(
            self.state.globals["wl_compositor"].0,
            4,
            &self.queue.handle(),
            (),
        );
        compositor.create_surface(&self.queue.handle(), ())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn single_pixel_buffers_preserve_color_and_destroy_cleanly() {
    let mut f = Fixture::new();
    let (name, version) = f.state.globals["wp_single_pixel_buffer_manager_v1"];
    assert_eq!(version, 1);
    let manager: pixel::WpSinglePixelBufferManagerV1 =
        f.registry.bind(name, 1, &f.queue.handle(), ());
    let color = [u32::MAX, 0, u32::MAX / 2, u32::MAX];
    let buffer = manager.create_u32_rgba_buffer(
        color[0],
        color[1],
        color[2],
        color[3],
        &f.queue.handle(),
        (),
    );
    let surface = f.surface();
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    f.sync();
    assert_eq!(f.observations.lock().unwrap().rgba, Some(color));
    surface.attach(None, 0, 0);
    surface.commit();
    buffer.destroy();
    manager.destroy();
    f.sync();
    assert_eq!(f.observations.lock().unwrap().destroyed_buffers, 1);
}
