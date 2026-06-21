mod dialog;
mod hotkey_ctrl;

use crate::AppContext;
use anyhow::Result;
use windows::Win32::Foundation::HWND;

pub fn open_dialog(ctx: &mut AppContext) -> Result<()> {
    dialog::open(ctx)
}

pub fn is_open() -> bool {
    dialog::is_open()
}

pub fn settings_hwnd() -> HWND {
    dialog::hwnd()
}
