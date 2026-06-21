use anyhow::{Context, Result};
use std::path::PathBuf;
use windows::core::w;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ,
};

pub fn exe_path() -> Result<PathBuf> {
    std::env::current_exe().context("current_exe")
}

pub fn is_enabled() -> bool {
    read_run_value().is_ok()
}

pub fn enable() -> Result<()> {
    let path = exe_path()?;
    let quoted = format!("\"{}\"", path.display());
    write_run_value(&quoted)
}

pub fn disable() -> Result<()> {
    delete_run_value()
}

pub fn toggle() -> Result<bool> {
    if is_enabled() {
        disable()?;
        Ok(false)
    } else {
        enable()?;
        Ok(true)
    }
}

fn read_run_value() -> Result<String> {
    unsafe {
        let mut key = HKEY::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
        .ok()
        .context("open Run key")?;

        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        RegGetValueW(
            key,
            None,
            w!("KeyNav"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
        .ok()
        .context("read KeyNav run value")?;
        let _ = RegCloseKey(key);
        let len = size as usize / 2;
        let end = buf.iter().take(len).position(|&c| c == 0).unwrap_or(len);
        Ok(String::from_utf16_lossy(&buf[..end]))
    }
}

fn write_run_value(value: &str) -> Result<()> {
    unsafe {
        let mut key = HKEY::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
        .ok()
        .context("open Run key for write")?;

        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        RegSetValueExW(
            key,
            w!("KeyNav"),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                wide.as_ptr() as *const u8,
                wide.len() * 2,
            )),
        )
        .ok()
        .context("set KeyNav run value")?;
        let _ = RegCloseKey(key);
        Ok(())
    }
}

fn delete_run_value() -> Result<()> {
    unsafe {
        let mut key = HKEY::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
        .ok()
        .context("open Run key for delete")?;

        match RegDeleteValueW(key, w!("KeyNav")).ok() {
            Err(e) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() => {}
            Err(e) => return Err(e).context("delete KeyNav run value"),
            Ok(()) => {}
        }
        let _ = RegCloseKey(key);
        Ok(())
    }
}
