mod autostart;
mod config;
mod hints;
mod hotkey;
mod overlay;
mod settings;
mod uia;

use anyhow::{Context, Result};
use config::Config;
use hints::{assign_hints, HintMatcher, MatchResult};
use hotkey::{is_hotkey_message, HotkeyRegistration};
use overlay::OverlayWindow;
use settings::{is_open, open_dialog, settings_hwnd};
use std::env;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::thread;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use uia::{activate_element, scan_foreground_window};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_BACK, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    IsDialogMessageW, PostMessageW, PostQuitMessage, RegisterClassW, SetWindowsHookExW, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, HHOOK, KBDLLHOOKSTRUCT, MB_OK, MSG, WH_KEYBOARD_LL, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_KEYDOWN, WM_SYSKEYDOWN, WNDCLASSW,
};

const MSG_CLASS: PCWSTR = windows::core::w!("KeyNavMessage");
const SINGLE_INSTANCE_MUTEX: PCWSTR = windows::core::w!("KeyNavSingleInstance");

pub const WM_KEYNAV_HINT: u32 = WM_APP + 1;
const WM_KEYNAV_KEY: u32 = WM_APP + 2;
const WM_KEYNAV_QUIT: u32 = WM_APP + 3;
const WM_KEYNAV_OPEN_SETTINGS: u32 = WM_APP + 6;

static MESSAGE_HWND: AtomicIsize = AtomicIsize::new(0);
static HINT_ACTIVE: AtomicBool = AtomicBool::new(false);
static HOTKEY_VK: AtomicU32 = AtomicU32::new(0x20);
static HOTKEY_MODS: AtomicU32 = AtomicU32::new(0x6);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    Idle,
    HintMode,
}

pub(crate) struct AppContext {
    state: AppState,
    config: Config,
    targets: Vec<hints::HintTarget>,
    matcher: HintMatcher,
    overlay: OverlayWindow,
    hook: Option<HHOOK>,
    hotkey: Option<HotkeyRegistration>,
    message_hwnd: HWND,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    ensure_single_instance()?;

    let config = Config::load().context("load config")?;
    update_hotkey_atomics(&config);
    log::info!("KeyNav started — {}", config.hotkey_display());
    log::info!("Config: {}", Config::config_path().display());

    let message_hwnd = create_message_window()?;
    MESSAGE_HWND.store(message_hwnd.0 as isize, Ordering::SeqCst);

    let overlay = OverlayWindow::create(&config)?;

    let open_settings_on_start = env::args().any(|a| a == "--settings");

    let tray_hwnd = message_hwnd.0 as isize;
    thread::spawn(move || {
        if let Err(e) = run_tray(tray_hwnd) {
            log::error!("tray error: {e:#}");
        }
    });

    let mut ctx = AppContext {
        state: AppState::Idle,
        config: config.clone(),
        targets: Vec::new(),
        matcher: HintMatcher::new(),
        overlay,
        hook: None,
        hotkey: register_hotkey(message_hwnd, &config),
        message_hwnd,
    };

    install_global_keyboard_hook(&mut ctx)?;

    if open_settings_on_start {
        open_dialog(&mut ctx)?;
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            match msg.message {
                WM_KEYNAV_HINT => {
                    log::info!("hint hotkey received");
                    start_hint_mode(&mut ctx);
                }
                m if is_hotkey_message(m, msg.wParam) => {
                    log::info!("WM_HOTKEY received");
                    start_hint_mode(&mut ctx);
                }
                WM_KEYNAV_KEY => handle_key(&mut ctx, msg.wParam.0 as u32),
                WM_KEYNAV_OPEN_SETTINGS => {
                    if let Err(e) = open_dialog(&mut ctx) {
                        log::warn!("open settings failed: {e:#}");
                    }
                }
                WM_KEYNAV_QUIT => {
                    finish_hint_mode(&mut ctx);
                    PostQuitMessage(0);
                }
                _ => {}
            }

            if is_open() && IsDialogMessageW(settings_hwnd(), &mut msg).as_bool() {
                continue;
            }

            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    finish_hint_mode(&mut ctx);
    Ok(())
}

fn register_hotkey(hwnd: HWND, config: &Config) -> Option<HotkeyRegistration> {
    match HotkeyRegistration::register(hwnd, config.hint_hotkey_modifiers, config.hint_hotkey_vk) {
        Ok(h) => {
            log::info!(
                "RegisterHotKey OK (vk={}, mods={:#x})",
                config.hint_hotkey_vk,
                config.hint_hotkey_modifiers
            );
            Some(h)
        }
        Err(e) => {
            log::warn!("RegisterHotKey failed ({e:#}) — using keyboard hook only");
            None
        }
    }
}

/// Apply config live. Returns an optional warning when RegisterHotKey fails but hook fallback remains active.
pub(crate) fn apply_config(ctx: &mut AppContext, config: Config) -> Result<Option<String>> {
    config.validate().context("validate config")?;
    config.save().context("save config")?;

    ctx.hotkey = None;
    let warning = match HotkeyRegistration::register(
        ctx.message_hwnd,
        config.hint_hotkey_modifiers,
        config.hint_hotkey_vk,
    ) {
        Ok(h) => {
            ctx.hotkey = Some(h);
            None
        }
        Err(e) => {
            log::warn!("RegisterHotKey failed on apply: {e:#}");
            Some(format!(
                "Saved, but RegisterHotKey failed ({e:#}). The keyboard hook fallback is still active."
            ))
        }
    };

    update_hotkey_atomics(&config);
    ctx.overlay.set_config(&config);
    ctx.config = config;
    log::info!("config applied — hotkey: {}", ctx.config.hotkey_display());
    Ok(warning)
}

fn update_hotkey_atomics(config: &Config) {
    HOTKEY_VK.store(config.hint_hotkey_vk, Ordering::SeqCst);
    HOTKEY_MODS.store(config.hint_hotkey_modifiers & 0x0F, Ordering::SeqCst);
}

fn start_hint_mode(ctx: &mut AppContext) {
    if ctx.state == AppState::HintMode {
        return;
    }

    let elements = match scan_foreground_window(ctx.config.max_elements) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("scan failed: {e:#}");
            unsafe {
                let _ = MessageBeep(MB_OK);
            }
            return;
        }
    };
    if elements.is_empty() {
        log::info!("no actionable elements in the foreground window");
        unsafe {
            let _ = MessageBeep(MB_OK);
        }
        return;
    }

    let alphabet = match ctx.config.parse_alphabet() {
        Ok(a) => a,
        Err(e) => {
            log::warn!("alphabet parse failed: {e:#}, using default");
            Config::default().parse_alphabet().unwrap_or_default()
        }
    };

    ctx.targets = assign_hints(elements, &alphabet);
    ctx.matcher.clear();
    ctx.state = AppState::HintMode;

    if let Err(e) = ctx.overlay.show_hints(&ctx.targets, "") {
        log::warn!("overlay failed: {e:#}");
        ctx.state = AppState::Idle;
        return;
    }
    HINT_ACTIVE.store(true, Ordering::SeqCst);
    log::info!("hint mode: {} targets", ctx.targets.len());
}

fn handle_key(ctx: &mut AppContext, vk: u32) {
    if ctx.state != AppState::HintMode {
        return;
    }

    if vk == VK_ESCAPE.0 as u32 {
        finish_hint_mode(ctx);
        return;
    }
    if vk == VK_BACK.0 as u32 {
        ctx.matcher.pop_char();
        let _ = ctx.overlay.show_hints(&ctx.targets, ctx.matcher.prefix());
        return;
    }
    if (0x41..=0x5A).contains(&vk) {
        let ch = (vk as u8 as char).to_ascii_lowercase();
        ctx.matcher.push_char(ch);
        match ctx.matcher.evaluate_against(&ctx.targets) {
            MatchResult::Partial => {
                let _ = ctx.overlay.show_hints(&ctx.targets, ctx.matcher.prefix());
            }
            MatchResult::Unique(target) => {
                finish_hint_mode(ctx);
                if let Err(e) = activate_element(&target.element) {
                    log::warn!("activate failed: {e:#}");
                }
            }
            MatchResult::None => {
                ctx.matcher.pop_char();
            }
        }
    }
}

fn finish_hint_mode(ctx: &mut AppContext) {
    ctx.overlay.hide();
    ctx.matcher.clear();
    ctx.targets.clear();
    ctx.state = AppState::Idle;
    HINT_ACTIVE.store(false, Ordering::SeqCst);
}

fn install_global_keyboard_hook(ctx: &mut AppContext) -> Result<()> {
    if ctx.hook.is_some() {
        return Ok(());
    }
    unsafe {
        let module = GetModuleHandleW(None)?;
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), module, 0)?;
        ctx.hook = Some(hook);
        log::info!("global keyboard hook installed");
    }
    Ok(())
}

fn chord_matches(vk: u32) -> bool {
    if vk != HOTKEY_VK.load(Ordering::SeqCst) {
        return false;
    }
    let want = HOTKEY_MODS.load(Ordering::SeqCst);
    unsafe {
        let ctrl = GetAsyncKeyState(VK_CONTROL.0 as i32) & 0x8000u16 as i16 != 0;
        let shift = GetAsyncKeyState(VK_SHIFT.0 as i32) & 0x8000u16 as i16 != 0;
        let alt = GetAsyncKeyState(VK_MENU.0 as i32) & 0x8000u16 as i16 != 0;
        let win = GetAsyncKeyState(VK_LWIN.0 as i32) & 0x8000u16 as i16 != 0
            || GetAsyncKeyState(VK_RWIN.0 as i32) & 0x8000u16 as i16 != 0;
        let have = (if alt { 0x1 } else { 0 })
            | (if ctrl { 0x2 } else { 0 })
            | (if shift { 0x4 } else { 0 })
            | (if win { 0x8 } else { 0 });
        have == want
    }
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let is_keydown = wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize;
    if !is_keydown {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
    let vk = kb.vkCode;

    if HINT_ACTIVE.load(Ordering::SeqCst) {
        let interesting =
            vk == VK_ESCAPE.0 as u32 || vk == VK_BACK.0 as u32 || (0x41..=0x5A).contains(&vk);
        if interesting {
            post_to_main(WM_KEYNAV_KEY, vk);
            return LRESULT(1);
        }
    } else if chord_matches(vk) {
        post_to_main(WM_KEYNAV_HINT, 0);
        return LRESULT(1);
    }

    CallNextHookEx(None, code, wparam, lparam)
}

fn post_to_main(msg: u32, vk: u32) {
    let hwnd = MESSAGE_HWND.load(Ordering::SeqCst);
    if hwnd == 0 {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            HWND(hwnd as *mut core::ffi::c_void),
            msg,
            WPARAM(vk as usize),
            LPARAM(0),
        );
    }
}

fn ensure_single_instance() -> Result<()> {
    unsafe {
        let handle = CreateMutexW(None, true, SINGLE_INSTANCE_MUTEX)?;
        if windows::Win32::Foundation::GetLastError()
            == windows::Win32::Foundation::ERROR_ALREADY_EXISTS
        {
            anyhow::bail!("KeyNav is already running");
        }
        let _ = handle;
    }
    Ok(())
}

fn create_message_window() -> Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(message_wnd_proc),
            hInstance: instance.into(),
            lpszClassName: MSG_CLASS,
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        RegisterClassW(&class);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            MSG_CLASS,
            windows::core::w!("KeyNavMessage"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            None,
            None,
        )?;
        if hwnd.0.is_null() {
            anyhow::bail!("failed to create message window");
        }
        Ok(hwnd)
    }
}

unsafe extern "system" fn message_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn tray_icon() -> Result<Icon> {
    if let Ok(icon) = Icon::from_resource(1, Some((16, 16))) {
        return Ok(icon);
    }
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for px in rgba.chunks_exact_mut(4) {
        px[0] = 0xFF;
        px[1] = 0x66;
        px[2] = 0x00;
        px[3] = 0xFF;
    }
    Icon::from_rgba(rgba, 16, 16).context("tray icon")
}

fn run_tray(hwnd_raw: isize) -> Result<()> {
    let menu = Menu::new();
    let hint_now = MenuItem::new("Hint now", true, None);
    let settings = MenuItem::new("Settings…", true, None);
    let autostart_label = if autostart::is_enabled() {
        "Start at login ✓"
    } else {
        "Start at login"
    };
    let autostart_item = MenuItem::new(autostart_label, true, None);
    let open_config = MenuItem::new("Open config folder", true, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append(&hint_now)?;
    menu.append(&settings)?;
    menu.append(&autostart_item)?;
    menu.append(&open_config)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;

    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("KeyNav — Ctrl+Shift+Space to hint")
        .with_icon(tray_icon()?)
        .build()?;

    let menu_channel = MenuEvent::receiver();

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);

            while let Ok(event) = menu_channel.try_recv() {
                if event.id == hint_now.id() {
                    post(hwnd_raw, WM_KEYNAV_HINT);
                } else if event.id == settings.id() {
                    post(hwnd_raw, WM_KEYNAV_OPEN_SETTINGS);
                } else if event.id == autostart_item.id() {
                    match autostart::toggle() {
                        Ok(enabled) => log::info!("start at login: {enabled}"),
                        Err(e) => log::warn!("autostart toggle failed: {e:#}"),
                    }
                } else if event.id == open_config.id() {
                    if let Some(parent) = Config::config_path().parent() {
                        let _ = std::process::Command::new("explorer").arg(parent).spawn();
                    }
                } else if event.id == quit.id() {
                    post(hwnd_raw, WM_KEYNAV_QUIT);
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn post(hwnd_raw: isize, msg: u32) {
    if hwnd_raw == 0 {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            HWND(hwnd_raw as *mut core::ffi::c_void),
            msg,
            WPARAM(0),
            LPARAM(0),
        );
    }
}
