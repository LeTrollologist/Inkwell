#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod credentials;
mod llm;
mod markdown;
mod settings;

use std::path::PathBuf;
use std::sync::Arc;

use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Icon, WindowBuilder};
use wry::{WebContext, WebViewBuilder};

use app::{AppContext, Emitter, UserEvent};

fn main() {
    let context = Arc::new(AppContext::new(app::initial_path_from_args()));

    // Keep the WebView2 profile in the user's local app data rather than next
    // to the executable (which may be read-only once installed).
    let profile_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Inkwell")
        .join("WebView2");
    let mut web_context = WebContext::new(Some(profile_dir));

    let mut event_loop_builder = EventLoopBuilder::<UserEvent>::with_user_event();
    let event_loop = event_loop_builder.build();

    let icon = Icon::from_rgba(include_bytes!("../ui/assets/icon.rgba").to_vec(), 64, 64).ok();

    let window = WindowBuilder::new()
        .with_title("Inkwell")
        .with_inner_size(LogicalSize::new(1360.0, 880.0))
        .with_min_inner_size(LogicalSize::new(900.0, 560.0))
        .with_resizable(true)
        .with_decorations(false)
        .with_window_icon(icon)
        .build(&event_loop)
        .expect("failed to create the Inkwell window");

    if let Some(monitor) = window.current_monitor() {
        let window_size = window.outer_size();
        let screen = monitor.size();
        let origin = monitor.position();
        window.set_outer_position(PhysicalPosition::new(
            origin.x + (screen.width.saturating_sub(window_size.width) / 2) as i32,
            origin.y + (screen.height.saturating_sub(window_size.height) / 2) as i32,
        ));
    }

    #[cfg(target_os = "windows")]
    use_full_window_client_area(&window);

    let emitter = Emitter::new(event_loop.create_proxy());
    let ipc_proxy = event_loop.create_proxy();

    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url("http://inkwell.localhost/index.html")
        .with_background_color((10, 11, 15, 255))
        .with_custom_protocol("inkwell".into(), |_id, request| app::serve_asset(request))
        .with_ipc_handler(move |request: wry::http::Request<String>| {
            // Runs on the webview's message pump: hand the payload to the event
            // loop so commands are executed with access to the window.
            let _ = ipc_proxy.send_event(UserEvent::Ipc(request.body().clone()));
        });

    #[cfg(debug_assertions)]
    let builder = builder.with_devtools(true);

    let webview = builder
        .build(&window)
        .expect("failed to create the Inkwell webview");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::Script(js)) => {
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::Ipc(body)) => {
                app::handle_ipc(&body, &window, &context, &emitter);
            }
            Event::UserEvent(UserEvent::Close) => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

/// The frameless window has an invisible resize frame; let the webview cover the
/// entire window instead of leaving an unpainted border that shows what's behind.
#[cfg(target_os = "windows")]
fn use_full_window_client_area(window: &tao::window::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsZoomed, NCCALCSIZE_PARAMS, SetWindowPos, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, WM_NCCALCSIZE,
    };

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_NCCALCSIZE && wparam != 0 {
            // Keep maximized windows inside the monitor work area (taskbar).
            if IsZoomed(hwnd) != 0 {
                let params = &mut *(lparam as *mut NCCALCSIZE_PARAMS);
                let mut info: MONITORINFO = std::mem::zeroed();
                info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                if !monitor.is_null() && GetMonitorInfoW(monitor, &mut info) != 0 {
                    params.rgrc[0] = info.rcWork;
                }
            }
            return 0;
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::Win32(win32) = handle.as_raw() {
            let hwnd = win32.hwnd.get() as HWND;
            unsafe {
                SetWindowSubclass(hwnd, Some(subclass_proc), 1, 0);
                // Force a non-client recalculation so the new client rect applies
                // before the webview is created.
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

