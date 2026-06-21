use anyhow::{Context, Result};
use windows::Win32::Foundation::{HWND, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::WindowsAndMessaging::WM_HOTKEY;

pub const HOTKEY_ID_HINT: i32 = 1;
pub const MOD_NOREPEAT: u32 = 0x4000;

pub struct HotkeyRegistration {
    hwnd: HWND,
    id: i32,
}

impl HotkeyRegistration {
    pub fn register(hwnd: HWND, modifiers: u32, vk: u32) -> Result<Self> {
        let mods = modifiers | MOD_NOREPEAT;
        unsafe {
            RegisterHotKey(hwnd, HOTKEY_ID_HINT, HOT_KEY_MODIFIERS(mods), vk)
                .context("RegisterHotKey failed — the chord may already be in use")?;
        }
        Ok(Self {
            hwnd,
            id: HOTKEY_ID_HINT,
        })
    }
}

impl Drop for HotkeyRegistration {
    fn drop(&mut self) {
        unsafe {
            let _ = UnregisterHotKey(self.hwnd, self.id);
        }
    }
}

pub fn is_hotkey_message(msg: u32, wparam: WPARAM) -> bool {
    msg == WM_HOTKEY && wparam.0 == HOTKEY_ID_HINT as usize
}
