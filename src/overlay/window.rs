use crate::config::{vk_display_name, Config};
use crate::uia::ClickButton;
use crate::hints::HintTarget;
use anyhow::{Context, Result};
use std::sync::Mutex;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    GetTextExtentPoint32W, InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW,
    UpdateWindow, ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    FF_DONTCARE, FW_BOLD, HDC, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetSystemMetrics, GetWindowRect, RegisterClassW,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, HWND_TOPMOST, LWA_COLORKEY,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SW_HIDE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_PAINT, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::PCWSTR;

const OVERLAY_CLASS: PCWSTR = windows::core::w!("KeyNavOverlay");
/// Pixels of this color become fully transparent (and click-through) in the
/// layered window. Badges use non-black colors so they stay visible.
const KEY_COLOR: u32 = 0x0000_0000;
const BADGE_PAD: i32 = 4;

struct RenderBadge {
    x: i32,
    y: i32,
    label: String,
}

struct RenderData {
    origin_x: i32,
    origin_y: i32,
    font_size: i32,
    bg: u32,
    fg: u32,
    badges: Vec<RenderBadge>,
    status: String,
    status_bounds: RECT,
}

static RENDER: Mutex<Option<RenderData>> = Mutex::new(None);

pub struct OverlayWindow {
    hwnd: HWND,
    config: Config,
    vx: i32,
    vy: i32,
    vw: i32,
    vh: i32,
}

impl OverlayWindow {
    pub fn create(config: &Config) -> Result<Self> {
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .context("GetModuleHandleW")?;

            let class = WNDCLASSW {
                lpfnWndProc: Some(overlay_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: OVERLAY_CLASS,
                ..Default::default()
            };
            RegisterClassW(&class);
        }

        let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

        let ex_style =
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT;
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(ex_style.0),
                OVERLAY_CLASS,
                windows::core::w!("KeyNav"),
                WINDOW_STYLE(WS_POPUP.0),
                vx,
                vy,
                vw,
                vh,
                None,
                None,
                None,
                None,
            )
        }?;

        if hwnd.0.is_null() {
            anyhow::bail!("CreateWindowExW overlay failed");
        }

        unsafe {
            SetLayeredWindowAttributes(hwnd, COLORREF(KEY_COLOR), 0, LWA_COLORKEY)
                .context("SetLayeredWindowAttributes")?;
        }

        Ok(Self {
            hwnd,
            config: config.clone(),
            vx,
            vy,
            vw,
            vh,
        })
    }

    pub fn set_config(&mut self, config: &Config) {
        self.config = config.clone();
    }

    pub fn show_hints(
        &self,
        targets: &[HintTarget],
        prefix: &str,
        button: Option<ClickButton>,
    ) -> Result<()> {
        let (mode, bg, fg) = match button {
            Some(ClickButton::Right) => ("Right click", self.config.right_badge_bg_color, self.config.right_badge_text_color),
            Some(ClickButton::Left) => ("Left click", self.config.badge_bg_color, self.config.badge_text_color),
            None => ("Default action", self.config.badge_bg_color, self.config.badge_text_color),
        };
        let switch_key = vk_display_name(self.config.switch_mode_vk);
        let status = format!("{mode} · {switch_key} to switch");
        let mut status_bounds = RECT {
            left: self.vx,
            top: self.vy,
            right: self.vx + self.vw,
            bottom: self.vy + self.vh,
        };
        if let Some(target) = targets.first() {
            let mut window = RECT::default();
            if unsafe { GetWindowRect(target.element.target_hwnd, &mut window) }.is_ok() {
                status_bounds.left = status_bounds.left.max(window.left);
                status_bounds.top = status_bounds.top.max(window.top);
                status_bounds.right = status_bounds.right.min(window.right);
                status_bounds.bottom = status_bounds.bottom.min(window.bottom);
            }
        }
        let badges: Vec<RenderBadge> = targets
            .iter()
            .filter(|t| prefix.is_empty() || t.label.starts_with(prefix))
            .map(|t| RenderBadge {
                x: t.element.rect.left,
                y: t.element.rect.top,
                label: t.label.clone(),
            })
            .collect();

        {
            let mut guard = RENDER.lock().unwrap();
            *guard = Some(RenderData {
                origin_x: self.vx,
                origin_y: self.vy,
                font_size: self.config.font_size,
                bg: bg.max(1),
                fg,
                badges,
                status,
                status_bounds,
            });
        }

        unsafe {
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.vx,
                self.vy,
                self.vw,
                self.vh,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .context("SetWindowPos")?;
            let _ = InvalidateRect(self.hwnd, None, TRUE);
            let _ = UpdateWindow(self.hwnd);
        }
        Ok(())
    }

    pub fn hide(&self) {
        {
            let mut guard = RENDER.lock().unwrap();
            *guard = None;
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe fn paint(hwnd: HWND, hdc: HDC) {
    let guard = RENDER.lock().unwrap();
    let data = match guard.as_ref() {
        Some(d) => d,
        None => return,
    };

    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);

    // Whole surface starts as the transparent key color.
    let key_brush = CreateSolidBrush(COLORREF(KEY_COLOR));
    FillRect(hdc, &client, key_brush);
    let _ = DeleteObject(key_brush);

    let font = CreateFontW(
        data.font_size,
        0,
        0,
        0,
        FW_BOLD.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        ANTIALIASED_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        windows::core::w!("Consolas"),
    );
    let old_font = SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT);

    let bg_brush = CreateSolidBrush(COLORREF(data.bg));
    for badge in &data.badges {
        let wide: Vec<u16> = badge.label.encode_utf16().collect();
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &wide, &mut sz);

        let x = badge.x - data.origin_x;
        let y = badge.y - data.origin_y;
        let rect = RECT {
            left: x,
            top: y,
            right: x + sz.cx + BADGE_PAD * 2,
            bottom: y + sz.cy + BADGE_PAD * 2,
        };
        FillRect(hdc, &rect, bg_brush);
        SetTextColor(hdc, COLORREF(data.fg));
        let _ = TextOutW(hdc, x + BADGE_PAD, y + BADGE_PAD, &wide);
    }
    let status: Vec<u16> = data.status.encode_utf16().collect();
    let mut size = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &status, &mut size);
    let width = size.cx + BADGE_PAD * 2;
    let height = size.cy + BADGE_PAD * 2;
    let bounds = data.status_bounds;
    let left = ((bounds.left + bounds.right - width) / 2 - data.origin_x)
        .clamp(0, (client.right - width).max(0));
    let top = (bounds.bottom - data.origin_y - height - 8)
        .clamp(0, (client.bottom - height).max(0));
    let status_rect = RECT { left, top, right: left + width, bottom: top + height };
    FillRect(hdc, &status_rect, bg_brush);
    SetTextColor(hdc, COLORREF(data.fg));
    let _ = TextOutW(hdc, left + BADGE_PAD, top + BADGE_PAD, &status);

    let _ = DeleteObject(bg_brush);
    SelectObject(hdc, old_font);
    let _ = DeleteObject(font);
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint(hwnd, hdc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
