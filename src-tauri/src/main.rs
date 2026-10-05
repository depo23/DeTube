// DeTube: YouTube in its own window, with tabs, Shorts / AI-video filters, a channel blocklist and ad blocking.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::webview::NewWindowResponse;
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, Url, WebviewBuilder, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_opener::OpenerExt;

const FILTER_JS: &str = include_str!("filter.js");
const ADBLOCK_JS: &str = include_str!("adblock.js");
const HOME: &str = "https://www.youtube.com/";
/// Height of the tab bar (the local "shell" page drawn above the YouTube tabs).
const TAB_BAR: f64 = 40.0;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    show_shorts: bool,
    #[serde(rename = "showAI")] // the page script's name
    show_ai: bool,
    /// Blocked channels: "@handle" or "channel/uc…", lowercased as the page script compares them.
    #[serde(default)]
    blocked: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { show_shorts: true, show_ai: true, blocked: Vec::new() }
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

fn save_settings(app: &AppHandle, settings: &Settings) {
    if let Some(path) = settings_path(app) {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, serde_json::to_string(settings).unwrap());
    }
}

fn apply_settings(app: &AppHandle, settings: Settings) {
    let labels: Vec<String> = {
        let state = app.state::<Shared>();
        let mut state = state.lock().unwrap();
        state.settings = settings.clone();
        state.tabs.iter().map(|t| t.label.clone()).collect()
    };
    save_settings(app, &settings);
    let _ = app.emit_to("blocked", "blocked", &settings.blocked);
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

/// On Linux, child web views are stacked in the window's vertical box and ignore
/// set_bounds, so the shell is pinned to its height there and the visible tab expands.
#[cfg(target_os = "linux")]
fn fit_shell(app: &AppHandle, height: f64) {
    use gtk::prelude::*;
    let Some(shell) = app.get_webview("shell") else { return };
    let _ = shell.with_webview(move |platform| {
        let widget = platform.inner().upcast::<gtk::Widget>();
        widget.set_size_request(-1, height as i32);
        if let Some(parent) = widget.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) {
            parent.set_child_packing(&widget, false, true, 0, gtk::PackType::Start);
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn fit_shell(_app: &AppHandle, _height: f64) {}

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
        (format!("tab{}", state.next_id), state.settings.clone())
    };
    let script = format!(
        "window.__mt2Settings = {};\n{}",
        serde_json::to_string(&settings).unwrap(),
        FILTER_JS
    );
    let (nav_app, nav_label, new_app, title_app) = (app.clone(), label.clone(), app.clone(), app.clone());
    // Starts blank: the page is loaded once the ad rules are in place, so the first page is covered too.
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(Url::parse("about:blank").unwrap()))
        .initialization_script(script)
        .initialization_script(ADBLOCK_JS)
        .on_navigation(move |url| !live_chat::restart_with_safari_agent(&nav_app, &nav_label, url) && allow_navigation(&nav_app, url))
        .on_new_window(move |url, _features| {
            // Creating a tab from inside this callback would block the UI thread.
            let app = new_app.clone();
            tauri::async_runtime::spawn(async move { handle_new_window(&app, url) });
            NewWindowResponse::Deny
        })
        .on_document_title_changed(move |webview, title| set_title(&title_app, webview.label(), title));
    #[cfg(target_os = "linux")]
    let builder = builder.on_page_load(|webview, payload| live_chat::on_page_load(&webview, &payload));
    let webview = window.add_child(builder, LogicalPosition::new(0.0, TAB_BAR), LogicalSize::new(1.0, 1.0))?;
    let loader = webview.clone();
    ad_rules::install(app, &webview, move || {
        let _ = loader.navigate(url);
    });
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
/// sent by the page script as detube.invalid/<command>.
fn handle_new_window(app: &AppHandle, url: Url) {
    if url.host_str() == Some("detube.invalid") {
        match url.path() {
            "/newtab" => {
                let _ = open_tab(app, Url::parse(HOME).unwrap());
            }
            "/close" => close_tab(app, &active_label(app)),
            "/next" => cycle_tab(app, 1),
            "/prev" => cycle_tab(app, -1),
            "/block" => {
                if let Some((_, channel)) = url.query_pairs().find(|(k, _)| k == "c") {
                    let mut blocked = app.state::<Shared>().lock().unwrap().settings.blocked.clone();
                    blocked.push(channel.into_owned());
                    set_blocklist(app, blocked);
                }
            }
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
    let height = height.max(TAB_BAR);
    app.state::<Shared>().lock().unwrap().bar = height;
    fit_shell(&app, height);
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
    let settings = app.state::<Shared>().lock().unwrap().settings.clone();
    let version = app.package_info().version.to_string();
    let build = || -> tauri::Result<Menu<tauri::Wry>> {
        Menu::with_items(
            &app,
            &[
                &CheckMenuItem::with_id(&app, "shorts", "Show Shorts", true, settings.show_shorts, None::<&str>)?,
                &CheckMenuItem::with_id(&app, "ai", "Show AI videos", true, settings.show_ai, None::<&str>)?,
                &PredefinedMenuItem::separator(&app)?,
                &MenuItem::with_id(&app, "forget", "Forget learned AI channels", true, None::<&str>)?,
                &MenuItem::with_id(&app, "blocked", "Blocked channels…", true, None::<&str>)?,
                &PredefinedMenuItem::separator(&app)?,
                &MenuItem::with_id(&app, "updates", "Check for updates…", true, None::<&str>)?,
                &MenuItem::with_id(&app, "about", format!("DeTube {version}"), false, None::<&str>)?,
            ],
        )
    };
    let menu = build().map_err(|e| e.to_string())?;
    let window = app.get_window("main").ok_or("no window")?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_setting(app: AppHandle, key: String) {
    let mut settings = app.state::<Shared>().lock().unwrap().settings.clone();
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

// ---------------------------------------------------------------- channel blocklist

/// Accepts what people paste: "Name", "@Name", "youtube.com/@Name/videos", "…/channel/UC…".
fn normalize_channel(input: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(input.trim()).decode_utf8_lossy().to_lowercase();
    let rest = decoded.find("youtube.com/").map_or(&decoded[..], |i| &decoded[i + "youtube.com/".len()..]);
    let mut parts = rest.split(['/', '?', '#']).filter(|p| !p.is_empty());
    match parts.next() {
        None => String::new(),
        Some("channel") => parts.next().map(|id| format!("channel/{id}")).unwrap_or_default(),
        Some(first) if first.starts_with('@') => first.to_string(),
        Some(first) => format!("@{first}"),
    }
}

/// Saves the list (normalized, without blanks or duplicates) and pushes it to every tab.
fn set_blocklist(app: &AppHandle, channels: Vec<String>) -> Vec<String> {
    let mut clean: Vec<String> = Vec::new();
    for channel in channels.iter().map(|c| normalize_channel(c)) {
        if !channel.is_empty() && !clean.contains(&channel) {
            clean.push(channel);
        }
    }
    let mut settings = app.state::<Shared>().lock().unwrap().settings.clone();
    settings.blocked = clean.clone();
    apply_settings(app, settings);
    clean
}

#[tauri::command]
async fn get_blocked(app: AppHandle) -> Vec<String> {
    app.state::<Shared>().lock().unwrap().settings.blocked.clone()
}

#[tauri::command]
async fn set_blocked(app: AppHandle, channels: Vec<String>) -> Vec<String> {
    set_blocklist(&app, channels)
}

/// The editor for the blocklist, in its own small window.
fn show_blocked_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("blocked") {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "blocked", WebviewUrl::App("blocked.html".into()))
        .title("Blocked channels")
        .inner_size(420.0, 460.0)
        .min_inner_size(340.0, 320.0)
        .build();
}

// ---------------------------------------------------------------- ad blocking (network)

/// Ad servers and YouTube's ad endpoints, blocked before they load. The page script
/// (adblock.js) handles the rest: ads stripped from the player's data, a skipper for any
/// that still play, and hidden ad slots.
mod ad_rules {
    #[cfg(any(windows, target_os = "linux"))]
    const HOSTS: [&str; 4] = ["doubleclick.net", "googlesyndication.com", "googleadservices.com", "imasdk.googleapis.com"];
    #[cfg(any(windows, target_os = "linux"))]
    const YOUTUBE_PATHS: [&str; 4] = ["pagead/", "api/stats/ads", "ptracking", "get_midroll_info"];

    /// Adds the rules to a new tab, then calls `then` (also if they couldn't be added).
    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn install(_app: &tauri::AppHandle, _webview: &tauri::Webview, then: impl FnOnce() + Send + 'static) {
        then();
    }

    #[cfg(windows)]
    fn is_ad(uri: &str) -> bool {
        let Ok(url) = tauri::Url::parse(uri) else { return false };
        let Some(host) = url.host_str() else { return false };
        let on = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
        HOSTS.iter().any(|h| on(h)) || (on("youtube.com") && YOUTUBE_PATHS.iter().any(|p| url.path().trim_start_matches('/').starts_with(p)))
    }

    /// WebView2: answer matching requests with a 403 instead of sending them.
    #[cfg(windows)]
    pub fn install(_app: &tauri::AppHandle, webview: &tauri::Webview, then: impl FnOnce() + Send + 'static) {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_22, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
        };
        use webview2_com::{take_pwstr, WebResourceRequestedEventHandler};
        use windows_core::{Interface, HSTRING, PWSTR};

        let result = webview.with_webview(move |platform| unsafe {
            if let Ok(core) = platform.controller().CoreWebView2() {
                let env = platform.environment();
                let patterns = HOSTS.iter().map(|h| format!("*{h}/*")).chain(YOUTUBE_PATHS.iter().map(|p| format!("*youtube.com/{p}*")));
                for pattern in patterns {
                    let filter = HSTRING::from(pattern);
                    // Version 22+ also reports requests made by iframes.
                    let _ = match core.cast::<ICoreWebView2_22>() {
                        Ok(core22) => core22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                            &filter,
                            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                            COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                        ),
                        Err(_) => core.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL),
                    };
                }
                // Every handler sees every filtered request (Tauri's own included), so check the URL here.
                let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut uri = PWSTR::null();
                    args.Request()?.Uri(&mut uri)?;
                    if is_ad(&take_pwstr(uri)) {
                        args.SetResponse(&env.CreateWebResourceResponse(None, 403, &HSTRING::from("Blocked"), &HSTRING::new())?)?;
                    }
                    Ok(())
                }));
                let mut token = 0;
                let _ = core.add_WebResourceRequested(&handler, &mut token);
            }
            then();
        });
        if let Err(e) = result {
            eprintln!("DeTube: ad rules not installed: {e}");
        }
    }

    /// WebKitGTK content blocker rules, the same format as Safari's. WebKit's rule syntax has no "|", hence one rule each.
    #[cfg(target_os = "linux")]
    fn rules() -> String {
        let filters = HOSTS
            .iter()
            .map(|h| format!("^[^:]+://+([^:/]+\\.)?{}[:/]", h.replace('.', "\\.")))
            .chain(YOUTUBE_PATHS.iter().map(|p| format!("^[^:]+://+([^:/]+\\.)?youtube\\.com/{p}")));
        let list: Vec<_> = filters
            .map(|f| serde_json::json!({ "trigger": { "url-filter": f }, "action": { "type": "block" } }))
            .collect();
        serde_json::to_string(&list).unwrap()
    }

    #[cfg(target_os = "linux")]
    thread_local! {
        /// Compiled once per launch, on the GTK thread.
        static COMPILED: std::cell::Cell<*mut webkit2gtk::ffi::WebKitUserContentFilter> = const { std::cell::Cell::new(std::ptr::null_mut()) };
    }

    #[cfg(target_os = "linux")]
    pub fn install(app: &tauri::AppHandle, webview: &tauri::Webview, then: impl FnOnce() + Send + 'static) {
        use tauri::Manager;
        use webkit2gtk::glib::translate::ToGlibPtr;
        use webkit2gtk::{ffi, gio, glib, WebViewExt};

        type Done = Box<dyn FnOnce(*mut ffi::WebKitUserContentFilter)>;
        unsafe extern "C" fn saved(store: *mut glib::gobject_ffi::GObject, result: *mut gio::ffi::GAsyncResult, data: glib::ffi::gpointer) {
            let done = Box::from_raw(data as *mut Done);
            let mut error = std::ptr::null_mut();
            let filter = ffi::webkit_user_content_filter_store_save_finish(store as _, result, &mut error);
            if !error.is_null() {
                eprintln!("DeTube: ad rules failed to compile: {}", std::ffi::CStr::from_ptr((*error).message).to_string_lossy());
                glib::ffi::g_error_free(error);
            }
            glib::gobject_ffi::g_object_unref(store);
            done(filter);
        }

        let store_dir = app.path().app_cache_dir().ok().map(|d| d.join("content-filters"));
        let result = webview.with_webview(move |platform| unsafe {
            let Some(manager) = platform.inner().user_content_manager() else { return then() };
            let cached = COMPILED.get();
            if !cached.is_null() {
                ffi::webkit_user_content_manager_add_filter(manager.to_glib_none().0, cached);
                return then();
            }
            let Some(dir) = store_dir.and_then(|d| std::ffi::CString::new(d.to_string_lossy().as_bytes()).ok()) else { return then() };
            let done: Done = Box::new(move |filter| {
                if !filter.is_null() {
                    ffi::webkit_user_content_manager_add_filter(manager.to_glib_none().0, filter);
                    if COMPILED.get().is_null() {
                        COMPILED.set(filter); // kept for the rest of the launch
                    } else {
                        ffi::webkit_user_content_filter_unref(filter);
                    }
                }
                then();
            });
            let store = ffi::webkit_user_content_filter_store_new(dir.as_ptr());
            let source = glib::Bytes::from_owned(rules());
            ffi::webkit_user_content_filter_store_save(
                store,
                c"detube-ads".as_ptr(),
                source.to_glib_none().0,
                std::ptr::null_mut(),
                Some(saved),
                Box::into_raw(Box::new(done)) as glib::ffi::gpointer,
            );
        });
        if let Err(e) = result {
            eprintln!("DeTube: ad rules not installed: {e}");
        }
    }
}

// ---------------------------------------------------------------- live chat

/// WebKitGTK's own user agent gets "your browser is out of date" from YouTube's live chat.
/// Only live chat gets Safari's: with it everywhere, YouTube also serves the full set of ads.
/// (WebView2 identifies as Edge, which live chat accepts as is.)
mod live_chat {
    use tauri::{AppHandle, Url};

    #[cfg(target_os = "linux")]
    const SAFARI: &str =
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

    /// Tabs currently using Safari's user agent.
    #[cfg(target_os = "linux")]
    static SAFARI_TABS: std::sync::Mutex<std::collections::BTreeSet<String>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());

    #[cfg(target_os = "linux")]
    fn is_live_chat(url: &Url) -> bool {
        url.host_str().is_some_and(|h| h == "youtube.com" || h.ends_with(".youtube.com")) && url.path().starts_with("/live_chat")
    }

    #[cfg(target_os = "linux")]
    fn set_user_agent(webview: &tauri::Webview, agent: Option<&'static str>) {
        let _ = webview.with_webview(move |platform| {
            use webkit2gtk::{SettingsExt, WebViewExt};
            if let Some(settings) = WebViewExt::settings(&platform.inner()) {
                settings.set_user_agent(agent); // None: WebKit's default
            }
        });
    }

    /// Live chat (embedded or popped out) about to load with the default agent: cancel it (returns true),
    /// switch the tab to Safari's and load it again — the agent can't change for a load already under way.
    #[cfg(target_os = "linux")]
    pub fn restart_with_safari_agent(app: &AppHandle, label: &str, url: &Url) -> bool {
        if !is_live_chat(url) || !SAFARI_TABS.lock().unwrap().insert(label.to_string()) {
            return false;
        }
        let (app, label, url) = (app.clone(), label.to_string(), url.to_string());
        // Not from inside the navigation callback.
        tauri::async_runtime::spawn(async move {
            let Some(webview) = tauri::Manager::get_webview(&app, &label) else { return };
            set_user_agent(&webview, Some(SAFARI));
            let url = serde_json::to_string(&url).unwrap();
            let _ = webview.eval(format!(
                "(u => {{ const frames = [...document.querySelectorAll('iframe')].filter(f => f.src === u); \
                 if (frames.length) frames.forEach(f => f.src = u); else location.href = u; }})({url})"
            ));
        });
        true
    }

    #[cfg(not(target_os = "linux"))]
    pub fn restart_with_safari_agent(_app: &AppHandle, _label: &str, _url: &Url) -> bool {
        false
    }

    /// Back to the default agent on the next full page load that isn't live chat.
    #[cfg(target_os = "linux")]
    pub fn on_page_load(webview: &tauri::Webview, payload: &tauri::webview::PageLoadPayload<'_>) {
        if payload.event() == tauri::webview::PageLoadEvent::Started
            && !is_live_chat(payload.url())
            && SAFARI_TABS.lock().unwrap().remove(webview.label())
        {
            set_user_agent(webview, None);
        }
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
            toggle_setting,
            get_blocked,
            set_blocked
        ])
        .on_menu_event(|app, event| {
            let app = app.clone();
            let id = event.id().0.clone();
            tauri::async_runtime::spawn(async move {
                match id.as_str() {
                    "shorts" | "ai" => toggle_setting(app, id).await,
                    "forget" => forget_channels(&app),
                    "blocked" => show_blocked_window(&app),
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
                .title("DeTube")
                .inner_size(1280.0, 800.0)
                .min_inner_size(640.0, 420.0)
                .build()?;
            window.add_child(
                WebviewBuilder::new("shell", WebviewUrl::App("index.html".into())),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(1280.0, TAB_BAR),
            )?;
            fit_shell(&handle, TAB_BAR);
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
        .expect("error while running DeTube");
}
