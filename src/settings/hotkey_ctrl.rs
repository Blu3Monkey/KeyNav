use crate::config::Config;
use windows::Win32::UI::Controls::{
    HKM_GETHOTKEY, HKM_SETHOTKEY, HOTKEYF_ALT, HOTKEYF_CONTROL, HOTKEYF_SHIFT,
};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{GetDlgItem, SendMessageW};

pub fn set_hotkey(hwnd: HWND, ctrl_id: i32, config: &Config) {
    let mods = hotkey_flags(config);
    let packed = make_hotkey_word(config.hint_hotkey_vk as u8, mods);
    if let Ok(ctrl) = unsafe { GetDlgItem(hwnd, ctrl_id) } {
        unsafe {
            SendMessageW(ctrl, HKM_SETHOTKEY, WPARAM(packed as usize), LPARAM(0));
        }
    }
}

pub fn get_hotkey(hwnd: HWND, ctrl_id: i32, win_checked: bool) -> (u32, u32) {
    let packed = unsafe {
        let Ok(ctrl) = GetDlgItem(hwnd, ctrl_id) else {
            return (0, 0);
        };
        SendMessageW(ctrl, HKM_GETHOTKEY, WPARAM(0), LPARAM(0)).0 as u16
    };
    let vk = (packed & 0xFF) as u32;
    let flags = ((packed >> 8) & 0xFF) as u16;
    let mut mods = 0u32;
    if flags & HOTKEYF_ALT as u16 != 0 {
        mods |= crate::config::MOD_ALT;
    }
    if flags & HOTKEYF_CONTROL as u16 != 0 {
        mods |= crate::config::MOD_CONTROL;
    }
    if flags & HOTKEYF_SHIFT as u16 != 0 {
        mods |= crate::config::MOD_SHIFT;
    }
    if win_checked {
        mods |= crate::config::MOD_WIN;
    }
    (mods, vk)
}

fn hotkey_flags(config: &Config) -> u8 {
    let mut flags = 0u8;
    if config.hotkey_alt() {
        flags |= HOTKEYF_ALT as u8;
    }
    if config.hotkey_ctrl() {
        flags |= HOTKEYF_CONTROL as u8;
    }
    if config.hotkey_shift() {
        flags |= HOTKEYF_SHIFT as u8;
    }
    flags
}

fn make_hotkey_word(vk: u8, mods: u8) -> u16 {
    (vk as u16) | ((mods as u16) << 8)
}
