// WinTube: YouTube in its own window, with tabs and Shorts / AI-video filters.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::webview::NewWindowResponse;
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, Url, WebviewBuilder, WebviewUrl,
    WindowEvent,
};
use tauri_plugin_opener::OpenerExt;

const FILTER_JS: &str = include_str!("filter.js");
const HOME: &str = "https://www.youtube.com/";
/// Height of the tab bar (the local "shell" page drawn above the YouTube tabs).
const TAB_BAR: f64 = 40.0;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    show_shorts: bool,
    show_ai: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { show_shorts: true, show_ai: true }
    }
}

#[derive(Clone, Serialize)]
struct Tab {
    label: String,
    title: String,
}

#[derive(Clone, Serialize)]
struct TabsPayload {
    tabs: Vec<Tab>,
    active: String,
}

struct AppState {
    tabs: Vec<Tab>,
    active: String,
    next_id: u32,
    /// Shell height: tab bar plus the update banner when it is showing.
    bar: f64,
    settings: Settings,
}

type Shared = Mutex<AppState>;

// ---------------------------------------------------------------- settings

fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: Settings) {
    if let Some(path) = settings_path(app) {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, serde_json::to_string(&settings).unwrap());
    }
}

fn apply_settings(app: &AppHandle, settings: Settings) {
    let labels: Vec<String> = {
        let state = app.state::<Shared>();
        let mut state = state.lock().unwrap();
        state.settings = settings;
        state.tabs.iter().map(|t| t.label.clone()).collect()
    };
    save_settings(app, settings);
    let js = format!(
        "window.__mt2 && window.__mt2.update({})",
        serde_json::to_string(&settings).unwrap()
    );
    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.eval(js.clone());
        }
    }
}

// ---------------------------------------------------------------- layout & tabs

/// Shell across the top; the active tab fills the rest; other tabs are hidden.
fn layout(app: &AppHandle) {
    let Some(window) = app.get_window("main") else { return };
    let Ok(physical) = window.inner_size() else { return };
    let size = physical.to_logical::<f64>(window.scale_factor().unwrap_or(1.0));
    let (tabs, active, bar) = {
        let state = app.state::<Shared>();
        let state = state.lock().unwrap();
        (state.tabs.clone(), state.active.clone(), state.bar)
    };
    if let Some(shell) = app.get_webview("shell") {
        let _ = shell.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(size.width, bar).into(),
        });
    }
    for tab in tabs {
        let Some(webview) = app.get_webview(&tab.label) else { continue };
        if tab.label == active {
            let _ = webview.set_bounds(Rect {
                position: LogicalPosition::new(0.0, bar).into(),
                size: LogicalSize::new(size.width, (size.height - bar).max(0.0)).into(),
            });
            let _ = webview.show();
            let _ = webview.set_focus();
        } else {
            let _ = webview.hide();
        }
    }
}

fn emit_tabs(app: &AppHandle) {
    let payload = {
        let state = app.state::<Shared>();
        let state = state.lock().unwrap();
        TabsPayload { tabs: state.tabs.clone(), active: state.active.clone() }
    };
    let _ = app.emit_to("shell", "tabs", payload);
}

fn open_tab(app: &AppHandle, url: Url) -> tauri::Result<()> {
    let window = app.get_window("main").ok_or(tauri::Error::WindowNotFound)?;
    let (label, settings) = {
        let state = app.state::<Shared>();
        let mut state = state.lock().unwrap();
        state.next_id += 1;
        (format!("tab{}", state.next_id), state.settings)
    };
    let script = format!(
        "window.__mt2Settings = {};\n{}",
        serde_json::to_string(&settings).unwrap(),
        FILTER_JS
    );
    let (nav_app, new_app, title_app) = (app.clone(), app.clone(), app.clone());
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(url))
        .initialization_script(script)
        .on_navigation(move |url| allow_navigation(&nav_app, url))
        .on_new_window(move |url, _features| {
            // Creating a tab from inside this callback would block the UI thread.
            let app = new_app.clone();
            tauri::async_runtime::spawn(async move { handle_new_window(&app, url) });
            NewWindowResponse::Deny
        })
        .on_document_title_changed(move |webview, title| set_title(&title_app, webview.label(), title));
    window.add_child(builder, LogicalPosition::new(0.0, TAB_BAR), LogicalSize::new(1.0, 1.0))?;
    {
        let state = app.state::<Shared>();
        let mut state = state.lock().unwrap();
        state.tabs.push(Tab { label: label.clone(), title: "YouTube".into() });
        state.active = label;
    }
    layout(app);
    emit_tabs(app);
    Ok(())
}

fn close_tab(app: &AppHandle, label: &str) {
    let remaining = {
        let state = app.state::<Shared>();
        let mut state = state.lock().unwrap();
        let Some(index) = state.tabs.iter().position(|t| t.label == label) else { return };
        state.tabs.remove(index);
        if state.active == label {
            if let Some(next) = state.tabs.get(index.min(state.tabs.len().saturating_sub(1))) {
                state.active = next.label.clone();
            }
        }
        state.tabs.len()
    };
    // Closing the web view also stops anything it was playing.
    if let Some(webview) = app.get_webview(label) {
        let _ = webview.close();
    }
    if remaining == 0 {
        app.exit(0);
        return;
    }
    layout(app);
    emit_tabs(app);
}

fn select_tab(app: &AppHandle, label: &str) {
    {
        let state = app.state::<Shared>();
        let mut state = state.lock().unwrap();
        if !state.tabs.iter().any(|t| t.label == label) {
            return;
        }
        state.active = label.to_string();
    }
    layout(app);
    emit_tabs(app);
}

/// Moves to the next (step 1) or previous (step -1) tab, wrapping around.
fn cycle_tab(app: &AppHandle, step: isize) {
    let target = {
        let state = app.state::<Shared>();
        let state = state.lock().unwrap();
        let count = state.tabs.len() as isize;
        let Some(index) = state.tabs.iter().position(|t| t.label == state.active) else { return };
        state.tabs[((index as isize + step).rem_euclid(count)) as usize].label.clone()
    };
    select_tab(app, &target);
}

fn active_label(app: &AppHandle) -> String {
    app.state::<Shared>().lock().unwrap().active.clone()
}

fn set_title(app: &AppHandle, label: &str, title: String) {
    {
        let state = app.state::<Shared>();
        let mut state = state.lock().unwrap();
        let Some(tab) = state.tabs.iter_mut().find(|t| t.label == label) else { return };
        let title = title.strip_suffix(" - YouTube").unwrap_or(&title).trim();
        tab.title = if title.is_empty() { "YouTube".into() } else { title.to_string() };
    }
    emit_tabs(app);
}

// ---------------------------------------------------------------- links

/// YouTube itself and Google sign-in stay inside the app.
fn is_internal(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str() else { return false };
    let host = host.to_ascii_lowercase();
    const SUFFIXES: [&str; 5] =
        ["youtube.com", "youtu.be", "youtube-nocookie.com", "gstatic.com", "googleusercontent.com"];
    SUFFIXES.iter().any(|s| host == *s || host.ends_with(&format!(".{s}")))
        || host.split('.').any(|label| label == "google")
}

/// The real destination of YouTube's "redirect?q=" links (descriptions and comments).
fn unwrap_redirect(url: Url) -> Url {
    let is_redirect = url.host_str().is_some_and(|h| h.ends_with("youtube.com")) && url.path() == "/redirect";
    if is_redirect {
        if let Some(target) = url.query_pairs().find(|(k, _)| k == "q").and_then(|(_, v)| Url::parse(&v).ok()) {
            return target;
        }
    }
    url
}

fn open_in_browser(app: &AppHandle, url: &Url) {
    let _ = app.opener().open_url(url.as_str(), None::<&str>);
}

/// Top-level navigations: links leaving YouTube open in the default browser instead.
fn allow_navigation(app: &AppHandle, url: &Url) -> bool {
    if matches!(url.scheme(), "about" | "data" | "blob" | "javascript") {
        return true;
    }
    let target = unwrap_redirect(url.clone());
    if is_internal(&target) {
        return true;
    }
    open_in_browser(app, &target);
    false
}

/// New-window requests: target="_blank" / Ctrl+click links, and the tab shortcuts
/// sent by the page script as wintube.invalid/<command>.
fn handle_new_window(app: &AppHandle, url: Url) {
    if url.host_str() == Some("wintube.invalid") {
        match url.path() {
            "/newtab" => {
                let _ = open_tab(app, Url::parse(HOME).unwrap());
            }
            "/close" => close_tab(app, &active_label(app)),
            "/next" => cycle_tab(app, 1),
            "/prev" => cycle_tab(app, -1),
            _ => {}
        }
        return;
    }
    let target = unwrap_redirect(url);
    if is_internal(&target) {
        let _ = open_tab(app, target);
    } else if !matches!(target.scheme(), "about" | "data" | "blob" | "javascript") {
        open_in_browser(app, &target);
    }
}

// ---------------------------------------------------------------- commands (called by the shell)

#[tauri::command]
async fn get_tabs(app: AppHandle) -> TabsPayload {
    let state = app.state::<Shared>();
    let state = state.lock().unwrap();
    TabsPayload { tabs: state.tabs.clone(), active: state.active.clone() }
}

#[tauri::command]
async fn new_tab(app: AppHandle) -> Result<(), String> {
    open_tab(&app, Url::parse(HOME).unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn close_tab_cmd(app: AppHandle, label: String) {
    close_tab(&app, &label);
}

#[tauri::command]
async fn close_active(app: AppHandle) {
    close_tab(&app, &active_label(&app));
}

#[tauri::command]
async fn select_tab_cmd(app: AppHandle, label: String) {
    select_tab(&app, &label);
}

#[tauri::command]
async fn cycle_tab_cmd(app: AppHandle, step: isize) {
    cycle_tab(&app, step);
}

/// back / forward / reload / home on the active tab.
#[tauri::command]
async fn nav(app: AppHandle, action: String) {
    let Some(webview) = app.get_webview(&active_label(&app)) else { return };
    let _ = match action.as_str() {
        "back" => webview.eval("history.back()"),
        "forward" => webview.eval("history.forward()"),
        "reload" => webview.eval("location.reload()"),
        "home" => webview.navigate(Url::parse(HOME).unwrap()),
        _ => Ok(()),
    };
}

/// The shell grows when the update banner is shown.
#[tauri::command]
async fn set_bar_height(app: AppHandle, height: f64) {
    app.state::<Shared>().lock().unwrap().bar = height.max(TAB_BAR);
    layout(&app);
}

#[tauri::command]
async fn open_external(app: AppHandle, url: String) {
    if let Ok(url) = Url::parse(&url) {
        open_in_browser(&app, &url);
    }
}

/// Native settings menu under the ⚙ button.
#[tauri::command]
async fn show_menu(app: AppHandle) -> Result<(), String> {
    let settings = app.state::<Shared>().lock().unwrap().settings;
    let version = app.package_info().version.to_string();
    let build = || -> tauri::Result<Menu<tauri::Wry>> {
        Menu::with_items(
            &app,
            &[
                &CheckMenuItem::with_id(&app, "shorts", "Show Shorts", true, settings.show_shorts, Some("Ctrl+Shift+1"))?,
                &CheckMenuItem::with_id(&app, "ai", "Show AI videos", true, settings.show_ai, Some("Ctrl+Shift+2"))?,
                &PredefinedMenuItem::separator(&app)?,
                &MenuItem::with_id(&app, "forget", "Forget learned AI channels", true, None::<&str>)?,
                &PredefinedMenuItem::separator(&app)?,
                &MenuItem::with_id(&app, "updates", "Check for updates…", true, None::<&str>)?,
                &MenuItem::with_id(&app, "about", format!("WinTube {version}"), false, None::<&str>)?,
            ],
        )
    };
    let menu = build().map_err(|e| e.to_string())?;
    let window = app.get_window("main").ok_or("no window")?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_setting(app: AppHandle, key: String) {
    let mut settings = app.state::<Shared>().lock().unwrap().settings;
    match key.as_str() {
        "shorts" => settings.show_shorts = !settings.show_shorts,
        "ai" => settings.show_ai = !settings.show_ai,
        _ => return,
    }
    apply_settings(&app, settings);
}

fn forget_channels(app: &AppHandle) {
    // Learned channels live in YouTube's storage, shared by all tabs; any tab can clear them.
    if let Some(webview) = app.get_webview(&active_label(app)) {
        let _ = webview.eval("window.__mt2 && window.__mt2.forgetChannels()");
    }
}

// ---------------------------------------------------------------- app

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage::<Shared>(Mutex::new(AppState {
            tabs: Vec::new(),
            active: String::new(),
            next_id: 0,
            bar: TAB_BAR,
            settings: Settings::default(),
        }))
        .invoke_handler(tauri::generate_handler![
            get_tabs,
            new_tab,
            close_tab_cmd,
            close_active,
            select_tab_cmd,
            cycle_tab_cmd,
            nav,
            set_bar_height,
            open_external,
            show_menu,
            toggle_setting
        ])
        .on_menu_event(|app, event| {
            let app = app.clone();
            let id = event.id().0.clone();
            tauri::async_runtime::spawn(async move {
                match id.as_str() {
                    "shorts" | "ai" => toggle_setting(app, id).await,
                    "forget" => forget_channels(&app),
                    "updates" => {
                        let _ = app.emit_to("shell", "check-updates", ());
                    }
                    _ => {}
                }
            });
        })
        .setup(|app| {
            let handle = app.handle().clone();
            app.state::<Shared>().lock().unwrap().settings = load_settings(&handle);

            let window = WindowBuilder::new(app, "main")
                .title("WinTube")
                .inner_size(1280.0, 800.0)
                .min_inner_size(640.0, 420.0)
                .build()?;
            window.add_child(
                WebviewBuilder::new("shell", WebviewUrl::App("index.html".into())),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(1280.0, TAB_BAR),
            )?;
            let resize_handle = handle.clone();
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }) {
                    layout(&resize_handle);
                }
            });
            open_tab(&handle, Url::parse(HOME).unwrap())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running WinTube");
}
