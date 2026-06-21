use crate::config::{validate_alphabet_str, Config, DEFAULT_ALPHABET};
use crate::settings::hotkey_ctrl;
use crate::AppContext;
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, HMODULE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::Dialogs::{
    ChooseColorW, CHOOSECOLORW, CC_RGBINIT, CC_SOLIDCOLOR,
};
use windows::Win32::UI::Controls::{
    BST_CHECKED, BST_UNCHECKED, CCM_SETVERSION, InitCommonControlsEx, INITCOMMONCONTROLSEX,
    ICC_BAR_CLASSES, ICC_HOTKEY_CLASS, TBS_AUTOTICKS, TBS_HORZ, TRACKBAR_CLASSW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetDlgItem, GetWindowLongPtrW, GetWindowTextLengthW,
    GetWindowTextW, IsWindow, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW,
    ShowWindow, BM_GETCHECK, BM_SETCHECK, ES_AUTOHSCROLL, GWLP_USERDATA, HMENU, SW_SHOW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_HSCROLL,
    WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_DLGMODALFRAME, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    WS_VSCROLL,
};

const SETTINGS_CLASS: PCWSTR = w!("KeyNavSettings");
const TBM_GETPOS: u32 = 0x0400;
const TBM_SETPOS: u32 = 0x0405;
const TBM_SETRANGE: u32 = 0x0404;
const TBM_SETTICFREQ: u32 = 0x0414;

const IDC_HOTKEY: i32 = 1001;
const IDC_CHK_WIN: i32 = 1002;
const IDC_ALPHABET: i32 = 1010;
const IDC_PRESET_HOME: i32 = 1011;
const IDC_PRESET_QWERTY: i32 = 1012;
const IDC_PRESET_RESET: i32 = 1013;
const IDC_FONT_SLIDER: i32 = 1020;
const IDC_FONT_LABEL: i32 = 1021;
const IDC_MAX_ELEMENTS: i32 = 1022;
const IDC_BG_COLOR: i32 = 1023;
const IDC_TEXT_COLOR: i32 = 1024;
const IDC_ERROR: i32 = 1030;
const IDC_SAVE: i32 = 1;
const IDC_CANCEL: i32 = 2;

static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);

struct DialogState {
    draft: Config,
    bg_color: COLORREF,
    text_color: COLORREF,
    ctx: *mut AppContext,
}

pub fn hwnd() -> HWND {
    let raw = SETTINGS_HWND.load(Ordering::SeqCst);
    HWND(raw as *mut core::ffi::c_void)
}

pub fn is_open() -> bool {
    unsafe { IsWindow(hwnd()).as_bool() }
}

pub fn open(ctx: &mut AppContext) -> Result<()> {
    init_common_controls();
    if is_open() {
        unsafe {
            let _ = SetForegroundWindow(hwnd());
        }
        return Ok(());
    }

    unsafe {
        let instance = GetModuleHandleW(None)?;
        register_class(instance)?;

        let state = Box::new(DialogState {
            draft: ctx.config.clone(),
            bg_color: COLORREF(ctx.config.badge_bg_color),
            text_color: COLORREF(ctx.config.badge_text_color),
            ctx: ctx as *mut AppContext,
        });

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_DLGMODALFRAME.0),
            SETTINGS_CLASS,
            w!("KeyNav Settings"),
            WINDOW_STYLE((WS_CAPTION | WS_SYSMENU | WS_VSCROLL).0),
            100,
            100,
            480,
            520,
            None,
            None,
            instance,
            None,
        )?;

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

        create_child_controls(hwnd, instance)?;
        load_controls(hwnd, ctx.config.clone())?;

        SETTINGS_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(())
}

fn init_common_controls() {
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_HOTKEY_CLASS | ICC_BAR_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
    }
}

unsafe fn register_class(instance: HMODULE) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{RegisterClassW, WNDCLASSW};
    let class = WNDCLASSW {
        lpfnWndProc: Some(settings_wnd_proc),
        hInstance: instance.into(),
        lpszClassName: SETTINGS_CLASS,
        ..Default::default()
    };
    RegisterClassW(&class);
    Ok(())
}

unsafe fn create_child_controls(parent: HWND, instance: HMODULE) -> Result<()> {
    let mut y = 12i32;
    let label = |text: PCWSTR, yy: i32| {
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            text,
            WINDOW_STYLE((WS_CHILD | WS_VISIBLE).0),
            12,
            yy,
            440,
            16,
            parent,
            None,
            instance,
            None,
        );
    };
    let btn = |text: PCWSTR, id: i32, x: i32, yy: i32, width: i32| {
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            text,
            WINDOW_STYLE((WS_CHILD | WS_VISIBLE | WS_TABSTOP).0),
            x,
            yy,
            width,
            24,
            parent,
            HMENU(id as isize as *mut _),
            instance,
            None,
        );
    };

    label(w!("Activation hotkey (click field, press chord):"), y);
    y += 20;
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("msctls_hotkey32"),
        w!(""),
        WINDOW_STYLE((WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER).0),
        12,
        y,
        200,
        24,
        parent,
        HMENU(IDC_HOTKEY as isize as *mut _),
        instance,
        None,
    );
    btn(w!("Win"), IDC_CHK_WIN, 220, y, 60);
    y += 36;

    label(w!("Hint letters (order matters):"), y);
    y += 18;
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("EDIT"),
        w!(""),
        WINDOW_STYLE((WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER).0 | ES_AUTOHSCROLL as u32),
        12,
        y,
        440,
        24,
        parent,
        HMENU(IDC_ALPHABET as isize as *mut _),
        instance,
        None,
    );
    y += 30;
    btn(w!("Home row"), IDC_PRESET_HOME, 12, y, 90);
    btn(w!("QWERTY left"), IDC_PRESET_QWERTY, 108, y, 90);
    btn(w!("Reset"), IDC_PRESET_RESET, 204, y, 70);
    y += 36;

    label(w!("Appearance"), y);
    y += 18;
    label(w!("Font size:"), y);
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        TRACKBAR_CLASSW,
        w!(""),
        WINDOW_STYLE((WS_CHILD | WS_VISIBLE | WS_TABSTOP).0 | TBS_HORZ | TBS_AUTOTICKS),
        12,
        y + 16,
        300,
        30,
        parent,
        HMENU(IDC_FONT_SLIDER as isize as *mut _),
        instance,
        None,
    );
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!("14"),
        WINDOW_STYLE((WS_CHILD | WS_VISIBLE).0),
        320,
        y + 20,
        40,
        16,
        parent,
        HMENU(IDC_FONT_LABEL as isize as *mut _),
        instance,
        None,
    );
    y += 52;

    btn(w!("Badge background…"), IDC_BG_COLOR, 12, y, 140);
    btn(w!("Badge text…"), IDC_TEXT_COLOR, 160, y, 140);
    y += 32;

    label(w!("Max elements per scan:"), y);
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("EDIT"),
        w!("500"),
        WINDOW_STYLE((WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER).0 | ES_AUTOHSCROLL as u32),
        160,
        y - 2,
        80,
        24,
        parent,
        HMENU(IDC_MAX_ELEMENTS as isize as *mut _),
        instance,
        None,
    );
    y += 36;

    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!(""),
        WINDOW_STYLE((WS_CHILD | WS_VISIBLE).0),
        12,
        y,
        440,
        32,
        parent,
        HMENU(IDC_ERROR as isize as *mut _),
        instance,
        None,
    );
    y += 40;

    btn(w!("Save"), IDC_SAVE, 280, y, 80);
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("BUTTON"),
        w!("Cancel"),
        WINDOW_STYLE((WS_CHILD | WS_VISIBLE | WS_TABSTOP).0),
        370,
        y,
        80,
        28,
        parent,
        HMENU(IDC_CANCEL as isize as *mut _),
        instance,
        None,
    );

    if let Ok(slider) = GetDlgItem(parent, IDC_FONT_SLIDER) {
        SendMessageW(slider, CCM_SETVERSION, WPARAM(0x0005), LPARAM(0));
    }
    Ok(())
}

unsafe fn load_controls(hwnd: HWND, config: Config) -> Result<()> {
    hotkey_ctrl::set_hotkey(hwnd, IDC_HOTKEY, &config);
    set_check(hwnd, IDC_CHK_WIN, config.hotkey_win());
    set_text(hwnd, IDC_ALPHABET, &config.hint_alphabet);
    set_text(hwnd, IDC_MAX_ELEMENTS, &config.max_elements.to_string());

    if let Ok(slider) = GetDlgItem(hwnd, IDC_FONT_SLIDER) {
        SendMessageW(
            slider,
            TBM_SETRANGE,
            WPARAM(1),
            LPARAM(((24i32 as isize) << 16) | 10),
        );
        SendMessageW(slider, TBM_SETTICFREQ, WPARAM(1), LPARAM(0));
        SendMessageW(
            slider,
            TBM_SETPOS,
            WPARAM(1),
            LPARAM(config.font_size.clamp(10, 24) as isize),
        );
    }
    update_font_label(hwnd);
    set_error(hwnd, "");
    Ok(())
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_HSCROLL => {
            update_font_label(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u16;
            if code == 0 {
                match id {
                    IDC_SAVE => {
                        if let Err(e) = on_save(hwnd) {
                            set_error(hwnd, &format!("{e:#}"));
                        }
                    }
                    IDC_CANCEL => {
                        let _ = DestroyWindow(hwnd);
                    }
                    IDC_PRESET_HOME => set_text(hwnd, IDC_ALPHABET, "asdfghjkl"),
                    IDC_PRESET_QWERTY => set_text(hwnd, IDC_ALPHABET, DEFAULT_ALPHABET),
                    IDC_PRESET_RESET => {
                        set_text(hwnd, IDC_ALPHABET, &Config::default().hint_alphabet);
                    }
                    IDC_BG_COLOR => pick_color(hwnd, true),
                    IDC_TEXT_COLOR => pick_color(hwnd, false),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            SETTINGS_HWND.store(0, Ordering::SeqCst);
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogState;
            if !state_ptr.is_null() {
                drop(Box::from_raw(state_ptr));
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            LRESULT(0)
        }
        _ => windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn on_save(hwnd: HWND) -> Result<()> {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogState;
    if state_ptr.is_null() {
        anyhow::bail!("internal error: missing dialog state");
    }
    let state = &mut *state_ptr;
    let ctx = &mut *state.ctx;

    let win = get_check(hwnd, IDC_CHK_WIN);
    let (mods, vk) = hotkey_ctrl::get_hotkey(hwnd, IDC_HOTKEY, win);
    state.draft.set_hotkey_from_parts(
        mods & crate::config::MOD_CONTROL != 0,
        mods & crate::config::MOD_SHIFT != 0,
        mods & crate::config::MOD_ALT != 0,
        mods & crate::config::MOD_WIN != 0,
        vk,
    );
    state.draft.hint_alphabet = get_text(hwnd, IDC_ALPHABET);
    state.draft.font_size = get_slider_pos(hwnd, IDC_FONT_SLIDER).clamp(10, 24);
    state.draft.badge_bg_color = state.bg_color.0;
    state.draft.badge_text_color = state.text_color.0;
    state.draft.max_elements = get_text(hwnd, IDC_MAX_ELEMENTS)
        .parse()
        .context("max elements must be a number")?;

    validate_alphabet_str(&state.draft.hint_alphabet).context("alphabet")?;
    state.draft.validate_hotkey().context("hotkey")?;
    state.draft.validate().context("config")?;

    match crate::apply_config(ctx, state.draft.clone()) {
        Ok(Some(warning)) => show_info(hwnd, &warning),
        Ok(None) => {}
        Err(e) => anyhow::bail!("{e:#}"),
    }
    let _ = DestroyWindow(hwnd);
    Ok(())
}

unsafe fn pick_color(hwnd: HWND, background: bool) {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogState;
    if state_ptr.is_null() {
        return;
    }
    let state = &mut *state_ptr;
    static mut CUSTOM_COLORS: [COLORREF; 16] = [COLORREF(0); 16];
    let mut cc = CHOOSECOLORW {
        lpCustColors: std::ptr::addr_of_mut!(CUSTOM_COLORS).cast(),
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: hwnd,
        rgbResult: if background {
            state.bg_color
        } else {
            state.text_color
        },
        Flags: CC_RGBINIT | CC_SOLIDCOLOR,
        ..Default::default()
    };
    if ChooseColorW(&mut cc).as_bool() {
        if background {
            state.bg_color = cc.rgbResult;
        } else {
            state.text_color = cc.rgbResult;
        }
    }
}

unsafe fn set_text(hwnd: HWND, id: i32, text: &str) {
    if let Ok(ctrl) = GetDlgItem(hwnd, id) {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = SetWindowTextW(ctrl, PCWSTR(wide.as_ptr()));
    }
}

unsafe fn get_text(hwnd: HWND, id: i32) -> String {
    let Ok(ctrl) = GetDlgItem(hwnd, id) else {
        return String::new();
    };
    let len = GetWindowTextLengthW(ctrl);
    let mut buf = vec![0u16; len as usize + 1];
    let read = GetWindowTextW(ctrl, &mut buf);
    String::from_utf16_lossy(&buf[..read as usize])
}

unsafe fn set_check(hwnd: HWND, id: i32, checked: bool) {
    if let Ok(ctrl) = GetDlgItem(hwnd, id) {
        SendMessageW(
            ctrl,
            BM_SETCHECK,
            WPARAM(if checked {
                BST_CHECKED.0 as usize
            } else {
                BST_UNCHECKED.0 as usize
            }),
            LPARAM(0),
        );
    }
}

unsafe fn get_check(hwnd: HWND, id: i32) -> bool {
    let Ok(ctrl) = GetDlgItem(hwnd, id) else {
        return false;
    };
    SendMessageW(ctrl, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED.0 as isize
}

unsafe fn get_slider_pos(hwnd: HWND, id: i32) -> i32 {
    let Ok(ctrl) = GetDlgItem(hwnd, id) else {
        return 14;
    };
    SendMessageW(ctrl, TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as i32
}

unsafe fn update_font_label(hwnd: HWND) {
    let pos = get_slider_pos(hwnd, IDC_FONT_SLIDER);
    set_text(hwnd, IDC_FONT_LABEL, &pos.to_string());
}

unsafe fn set_error(hwnd: HWND, text: &str) {
    set_text(hwnd, IDC_ERROR, text);
}

unsafe fn show_info(hwnd: HWND, text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = MessageBoxW(hwnd, PCWSTR(wide.as_ptr()), w!("KeyNav"), MB_OK | MB_ICONWARNING);
}
