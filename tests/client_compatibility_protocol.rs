//! Client compatibility requests through the production upstream dispatcher.
#[path = "../src/wayland/clipboard_helper.rs"]
mod clipboard_helper;
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
use smithay::wayland::content_type::{ContentTypeState, ContentTypeSurfaceCachedState};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::single_pixel_buffer::{SinglePixelBufferState, get_single_pixel_buffer};
use smithay::wayland::xdg_toplevel_icon::{
    ToplevelIconCachedState, XdgToplevelIconHandler, XdgToplevelIconManager,
};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_registry, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols::wp::content_type::v1::client::{
    wp_content_type_manager_v1 as content_manager, wp_content_type_v1 as content,
};
use wayland_protocols::wp::single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1 as pixel;
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_protocols::xdg::toplevel_icon::v1::client::{
    xdg_toplevel_icon_manager_v1 as icon_manager, xdg_toplevel_icon_v1 as icon,
};

#[derive(Default)]
struct Observations {
    rgba: Option<[u32; 4]>,
    destroyed_buffers: usize,
    content_type: u32,
    icon_name: Option<String>,
    toplevels: Vec<WlSurface>,
}

struct Server {
    compositor: CompositorState,
    observations: Arc<Mutex<Observations>>,
    shell: XdgShellState,
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
            let content_type = *states
                .cached_state
                .get::<ContentTypeSurfaceCachedState>()
                .current()
                .content_type() as u32;
            let icon_name = states
                .cached_state
                .get::<ToplevelIconCachedState>()
                .current()
                .icon_name()
                .map(str::to_owned);
            let mut observations = self.observations.lock().unwrap();
            observations.content_type = content_type;
            observations.icon_name = icon_name;
            let mut attributes = states.cached_state.get::<SurfaceAttributes>();
            if let Some(BufferAssignment::NewBuffer(buffer)) = attributes.current().buffer.as_ref()
                && let Ok(pixel) = get_single_pixel_buffer(buffer)
            {
                observations.rgba = Some([pixel.r, pixel.g, pixel.b, pixel.a]);
            }
        });
    }
}
impl XdgToplevelIconHandler for Server {}
impl XdgShellHandler for Server {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.shell
    }
    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.observations
            .lock()
            .unwrap()
            .toplevels
            .push(surface.wl_surface().clone());
    }
    fn new_popup(&mut self, _: PopupSurface, _: PositionerState) {}
    fn grab(
        &mut self,
        _: PopupSurface,
        _: smithay::reexports::wayland_server::protocol::wl_seat::WlSeat,
        _: smithay::utils::Serial,
    ) {
    }
    fn reposition_request(&mut self, _: PopupSurface, _: PositionerState, _: u32) {}
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
delegate_noop!(Client: ignore content_manager::WpContentTypeManagerV1);
delegate_noop!(Client: ignore content::WpContentTypeV1);
delegate_noop!(Client: ignore icon_manager::XdgToplevelIconManagerV1);
delegate_noop!(Client: ignore icon::XdgToplevelIconV1);
delegate_noop!(Client: ignore xdg_wm_base::XdgWmBase);
delegate_noop!(Client: ignore xdg_surface::XdgSurface);
delegate_noop!(Client: ignore xdg_toplevel::XdgToplevel);

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
        let _content = ContentTypeState::new::<Server>(&dh);
        let _icons = XdgToplevelIconManager::new::<Server>(&dh);
        let shell = XdgShellState::new::<Server>(&dh);
        dh.insert_client(server_socket, Arc::new(ClientData::default()))
            .unwrap();
        let observations = Arc::new(Mutex::new(Observations::default()));
        let mut server = Server {
            compositor,
            observations: observations.clone(),
            shell,
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

    fn toplevel(&mut self, app_id: &str) -> (xdg_toplevel::XdgToplevel, WlSurface) {
        let shell: xdg_wm_base::XdgWmBase = self.registry.bind(
            self.state.globals["xdg_wm_base"].0,
            1,
            &self.queue.handle(),
            (),
        );
        let surface = self.surface();
        let xdg_surface = shell.get_xdg_surface(&surface, &self.queue.handle(), ());
        let toplevel = xdg_surface.get_toplevel(&self.queue.handle(), ());
        toplevel.set_app_id(app_id.into());
        self.sync();
        let server_surface = self
            .observations
            .lock()
            .unwrap()
            .toplevels
            .last()
            .unwrap()
            .clone();
        (toplevel, server_surface)
    }
}

const CLIPBOARD_APP_ID: &str = "io.github.bugaevc.wl-clipboard";

#[test]
fn clipboard_helper_remembers_the_exact_caller_with_two_or_three_windows() {
    for count in [2, 3] {
        let mut f = Fixture::new();
        let windows = (0..count)
            .map(|_| f.toplevel("kitty").1)
            .collect::<Vec<_>>();
        for caller in &windows {
            assert!(!clipboard_helper::is_helper(caller));
            assert!(clipboard_helper::saved_focus(caller).is_none());
            let (_, helper) = f.toplevel(CLIPBOARD_APP_ID);
            assert!(clipboard_helper::is_helper(&helper));
            clipboard_helper::remember_focus(&helper, Some(caller), None);
            assert_eq!(
                clipboard_helper::saved_focus(&helper)
                    .unwrap()
                    .window
                    .as_ref(),
                Some(caller)
            );
        }
    }
}

#[test]
fn overlapping_clipboard_helpers_return_to_the_original_terminal() {
    let mut f = Fixture::new();
    let (_, caller) = f.toplevel("kitty");
    let (first_toplevel, first) = f.toplevel(CLIPBOARD_APP_ID);
    clipboard_helper::remember_focus(&first, Some(&caller), None);
    let (_, second) = f.toplevel(CLIPBOARD_APP_ID);
    clipboard_helper::remember_focus(&second, Some(&first), None);
    first_toplevel.destroy();
    f.sync();
    assert_eq!(
        clipboard_helper::saved_focus(&second).unwrap().window,
        Some(caller)
    );
}

#[test]
fn clipboard_helpers_without_a_caller_do_not_invent_a_successor() {
    let mut f = Fixture::new();
    let (_, helper) = f.toplevel(CLIPBOARD_APP_ID);
    clipboard_helper::remember_focus(&helper, None, None);
    let saved = clipboard_helper::saved_focus(&helper).unwrap();
    assert!(saved.window.is_none());
    assert!(saved.layer.is_none());
}

#[test]
fn clipboard_return_identity_survives_metadata_changes_until_teardown() {
    let mut f = Fixture::new();
    let (_, caller) = f.toplevel("kitty");
    let (helper_toplevel, helper) = f.toplevel(CLIPBOARD_APP_ID);
    clipboard_helper::remember_focus(&helper, Some(&caller), None);
    helper_toplevel.set_app_id("changed-after-mapping".into());
    f.sync();
    assert!(!clipboard_helper::is_helper(&helper));
    assert_eq!(
        clipboard_helper::saved_focus(&helper).unwrap().window,
        Some(caller)
    );
}

#[test]
fn content_type_applies_on_commit_and_resets_when_destroyed() {
    let mut f = Fixture::new();
    let (name, version) = f.state.globals["wp_content_type_manager_v1"];
    assert_eq!(version, 1);
    let manager: content_manager::WpContentTypeManagerV1 =
        f.registry.bind(name, 1, &f.queue.handle(), ());
    let surface = f.surface();
    let content = manager.get_surface_content_type(&surface, &f.queue.handle(), ());
    content.set_content_type(content::Type::Game);
    f.sync();
    assert_eq!(
        f.observations.lock().unwrap().content_type,
        content::Type::None as u32
    );
    surface.commit();
    f.sync();
    assert_eq!(
        f.observations.lock().unwrap().content_type,
        content::Type::Game as u32
    );
    content.destroy();
    surface.commit();
    f.sync();
    assert_eq!(
        f.observations.lock().unwrap().content_type,
        content::Type::None as u32
    );
}

#[test]
fn toplevel_icon_metadata_applies_on_commit_and_can_be_cleared() {
    let mut f = Fixture::new();
    let (name, version) = f.state.globals["xdg_toplevel_icon_manager_v1"];
    assert_eq!(version, 1);
    let manager: icon_manager::XdgToplevelIconManagerV1 =
        f.registry.bind(name, 1, &f.queue.handle(), ());
    let shell: xdg_wm_base::XdgWmBase =
        f.registry
            .bind(f.state.globals["xdg_wm_base"].0, 1, &f.queue.handle(), ());
    let surface = f.surface();
    let xdg_surface = shell.get_xdg_surface(&surface, &f.queue.handle(), ());
    let toplevel = xdg_surface.get_toplevel(&f.queue.handle(), ());
    let icon = manager.create_icon(&f.queue.handle(), ());
    icon.set_name("org.example.Game".into());
    manager.set_icon(&toplevel, Some(&icon));
    f.sync();
    assert!(f.observations.lock().unwrap().icon_name.is_none());
    surface.commit();
    f.sync();
    assert_eq!(
        f.observations.lock().unwrap().icon_name.as_deref(),
        Some("org.example.Game")
    );
    manager.set_icon(&toplevel, None);
    surface.commit();
    f.sync();
    assert!(f.observations.lock().unwrap().icon_name.is_none());
    icon.destroy();
    manager.destroy();
    f.sync();
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
