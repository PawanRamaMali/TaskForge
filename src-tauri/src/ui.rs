//! Window layout: the full window or the small always-on-top mini view.
//! Both load the same page; the frontend picks the compact layout by window label.

use serde_json::json;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const MINI_LABEL: &str = "mini";
const MAIN_LABEL: &str = "main";
/// Argument that marks the elevated copy from "Restart as administrator".
pub const RELAUNCH_FLAG: &str = "--relaunch";
const MINI_WIDTH: f64 = 280.0;
const MINI_HEIGHT: f64 = 224.0;

/// Switch between the full window and the mini view, and remember the choice.
pub fn set_mini_mode(app: &AppHandle, on: bool) -> tauri::Result<()> {
    if on {
        show_mini(app)?;
        if let Some(main) = app.get_webview_window(MAIN_LABEL) {
            main.hide()?;
        }
    } else {
        show_main(app)?;
        if let Some(mini) = app.get_webview_window(MINI_LABEL) {
            mini.close()?;
        }
    }
    save_pref(app, on);
    Ok(())
}

/// At startup, reopen the view used last. The main window starts hidden so the
/// mini view doesn't flash the full window first.
pub fn restore_view(app: &AppHandle) {
    if load_pref(app) && set_mini_mode(app, true).is_ok() {
        return;
    }
    let _ = show_main(app);
}

fn show_main(app: &AppHandle) -> tauri::Result<()> {
    if let Some(main) = app.get_webview_window(MAIN_LABEL) {
        main.show()?;
        main.unminimize()?;
        main.set_focus()?;
    }
    Ok(())
}

fn show_mini(app: &AppHandle) -> tauri::Result<()> {
    if let Some(mini) = app.get_webview_window(MINI_LABEL) {
        mini.show()?;
        return mini.set_focus();
    }
    let mut builder = WebviewWindowBuilder::new(app, MINI_LABEL, WebviewUrl::default())
        .title("TaskForge mini")
        .inner_size(MINI_WIDTH, MINI_HEIGHT)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true);
    if let Some((x, y)) = default_position(app) {
        builder = builder.position(x, y);
    }
    let mini = builder.build()?;

    // Closed some other way (Alt+F4): bring the full window back instead of
    // leaving the app with nothing on screen.
    let handle = app.clone();
    mini.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { .. } = event {
            if let Some(main) = handle.get_webview_window(MAIN_LABEL) {
                if !main.is_visible().unwrap_or(true) {
                    let _ = main.show();
                    let _ = main.set_focus();
                    save_pref(&handle, false);
                }
            }
        }
    });
    Ok(())
}

/// Top-right corner of the primary monitor.
fn default_position(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok()??;
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let pos = monitor.position().to_logical::<f64>(scale);
    Some((pos.x + size.width - MINI_WIDTH - 16.0, pos.y + 16.0))
}

/// Windows single-instance guard. Because the "open in place of Task Manager" setting
/// makes Ctrl+Shift+Esc launch TaskForge, a running instance would otherwise get a new
/// window, sampler and tray icon on every press. Returns false when another instance was
/// focused instead and this process should exit.
#[cfg(windows)]
pub fn claim_single_instance() -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;

    let relaunch = std::env::args().any(|a| a == RELAUNCH_FLAG);
    // Session-local name, so a second signed-in user gets their own instance.
    let name = HSTRING::from("TaskForge-single-instance");
    let already_running = unsafe {
        // The handle is intentionally leaked: the OS holds the mutex for this process's
        // lifetime, and dropping the value does not close the handle.
        let _ = CreateMutexW(None, false, PCWSTR(name.as_ptr()));
        GetLastError() == ERROR_ALREADY_EXISTS
    };
    // The elevated relaunch takes over as the old instance exits; everyone else forwards.
    if already_running && !relaunch {
        focus_running_instance();
        return false;
    }
    true
}

/// Bring the already-running TaskForge to the front. Tries the mini view first so a
/// press while in compact mode doesn't pop the full window.
#[cfg(windows)]
fn focus_running_instance() {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    for title in ["TaskForge mini", "TaskForge"] {
        let t = HSTRING::from(title);
        unsafe {
            if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(t.as_ptr())) {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                if SetForegroundWindow(hwnd).as_bool() {
                    return;
                }
            }
        }
    }
}

fn pref_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("view.json"))
}

fn load_pref(app: &AppHandle) -> bool {
    pref_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("mini").and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

fn save_pref(app: &AppHandle, mini: bool) {
    if let Some(path) = pref_path(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, json!({ "mini": mini }).to_string());
    }
}
