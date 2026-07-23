use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use stora_core::settings::TrayDoubleClickAction;
use stora_core::Result;

use crate::state::AppState;

/// Builds the system tray icon and its menu.
///
/// The tooltip reports free space and the last scan time; the icon itself is
/// static, because a constantly animating tray icon is noise, not information.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let open =
        MenuItem::with_id(app, "open", "Open Stora", true, None::<&str>).map_err(to_error)?;
    let scan =
        MenuItem::with_id(app, "scan", "Scan storage", true, None::<&str>).map_err(to_error)?;
    let review = MenuItem::with_id(app, "cleanup", "Review cleanup", true, None::<&str>)
        .map_err(to_error)?;
    let large = MenuItem::with_id(app, "large", "Open largest files", true, None::<&str>)
        .map_err(to_error)?;
    let settings =
        MenuItem::with_id(app, "settings", "Settings", true, None::<&str>).map_err(to_error)?;
    let exit =
        MenuItem::with_id(app, "exit", "Exit Stora", true, None::<&str>).map_err(to_error)?;

    let separator_one = PredefinedMenuItem::separator(app).map_err(to_error)?;
    let separator_two = PredefinedMenuItem::separator(app).map_err(to_error)?;

    let menu = Menu::with_items(
        app,
        &[
            &open,
            &separator_one,
            &scan,
            &review,
            &large,
            &separator_two,
            &settings,
            &exit,
        ],
    )
    .map_err(to_error)?;

    TrayIconBuilder::with_id("stora-tray")
        .tooltip("Stora")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "exit" => app.exit(0),
            "open" => show_main_window(app),
            id => {
                // Navigation requests are forwarded to the frontend, which
                // owns routing.
                show_main_window(app);
                let page = match id {
                    "scan" => "home",
                    "cleanup" => "cleanup",
                    "large" => "largeFiles",
                    "settings" => "settings",
                    _ => return,
                };
                use tauri::Emitter;
                let _ = app.emit("stora://navigate", page);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                let action = app
                    .state::<AppState>()
                    .settings()
                    .map(|settings| settings.tray_double_click_action)
                    .unwrap_or(TrayDoubleClickAction::Open);

                match action {
                    TrayDoubleClickAction::Open => show_main_window(app),
                    TrayDoubleClickAction::Scan => navigate(app, "home"),
                    TrayDoubleClickAction::Cleanup => navigate(app, "cleanup"),
                    TrayDoubleClickAction::LargeFiles => navigate(app, "largeFiles"),
                }
            }
        })
        .build(app)
        .map_err(to_error)?;

    Ok(())
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn navigate<R: Runtime>(app: &AppHandle<R>, page: &str) {
    show_main_window(app);
    use tauri::Emitter;
    let _ = app.emit("stora://navigate", page);
}

/// Updates the tray tooltip with current free space.
pub fn update_tooltip<R: Runtime>(app: &AppHandle<R>, text: &str) {
    if let Some(tray) = app.tray_by_id("stora-tray") {
        let _ = tray.set_tooltip(Some(text));
    }
}

fn to_error(err: tauri::Error) -> stora_core::StoraError {
    stora_core::StoraError::Internal(format!("tray: {err}"))
}
