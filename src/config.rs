use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;
pub const MOD_WIN: u32 = 0x8;

pub const DEFAULT_ALPHABET: &str = "qwertasdfgzxcvb";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(default = "default_hotkey_modifiers")]
    pub hint_hotkey_modifiers: u32,
    #[serde(default = "default_hotkey_vk")]
    pub hint_hotkey_vk: u32,
    #[serde(default = "default_hint_alphabet")]
    pub hint_alphabet: String,
    #[serde(default = "default_font_size")]
    pub font_size: i32,
    #[serde(default = "default_badge_bg")]
    pub badge_bg_color: u32,
    pub right_badge_bg_color: u32,
    pub right_badge_text_color: u32,
    pub switch_mode_vk: u32,
    pub left_click_vk: u32,
    pub right_click_vk: u32,
    pub cancel_vk: u32,
    pub erase_vk: u32,
    #[serde(default = "default_badge_text")]
    pub badge_text_color: u32,
    #[serde(default = "default_max_elements")]
    pub max_elements: usize,
}

fn default_hotkey_modifiers() -> u32 {
    MOD_CONTROL | MOD_SHIFT
}

fn default_hotkey_vk() -> u32 {
    0x20 // VK_SPACE
}

fn default_hint_alphabet() -> String {
    DEFAULT_ALPHABET.to_string()
}

fn default_font_size() -> i32 {
    14
}

fn default_badge_bg() -> u32 {
    0x00CC6600 // orange BGR
}

fn default_badge_text() -> u32 {
    0x00FFFFFF // white
}

fn default_max_elements() -> usize {
    500
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hint_hotkey_modifiers: default_hotkey_modifiers(),
            hint_hotkey_vk: default_hotkey_vk(),
            hint_alphabet: default_hint_alphabet(),
            font_size: default_font_size(),
            badge_bg_color: default_badge_bg(),
            right_badge_bg_color: 0x008D437E,
            right_badge_text_color: default_badge_text(),
            switch_mode_vk: 0x09,
            left_click_vk: 0x31,
            right_click_vk: 0x32,
            cancel_vk: 0x1B,
            erase_vk: 0x08,
            badge_text_color: default_badge_text(),
            max_elements: default_max_elements(),
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let base = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        base.join("keynav").join("config.toml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
            }
            let default = Self::default();
            default.save()?;
            return Ok(default);
        }
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(&path, text)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        let alphabet = self.parse_alphabet()?;
        let mut seen = HashSet::new();
        for vk in self.hint_keys() {
            if !supported_hint_keys().contains(&vk) {
                bail!("Unsupported hint shortcut: {}", vk_display_name(vk));
            }
            if !seen.insert(vk) {
                bail!("{} is assigned to multiple hint shortcuts", vk_display_name(vk));
            }
            if alphabet.iter().any(|c| c.to_ascii_uppercase() as u32 == vk) {
                bail!("{} is already used in the hint alphabet", vk_display_name(vk));
            }
        }
        self.validate_hotkey()?;
        if self.font_size < 8 || self.font_size > 32 {
            bail!("font_size must be between 8 and 32");
        }
        if self.max_elements < 10 || self.max_elements > 2000 {
            bail!("max_elements must be between 10 and 2000");
        }
        Ok(())
    }

    pub fn hint_keys(&self) -> [u32; 5] {
        [self.switch_mode_vk, self.left_click_vk, self.right_click_vk, self.cancel_vk, self.erase_vk]
    }

    pub fn parse_alphabet(&self) -> Result<Vec<char>> {
        validate_alphabet_str(&self.hint_alphabet)
    }

    pub fn alphabet_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        for ch in ['i', 'l', 'o'] {
            if self.hint_alphabet.contains(ch) {
                warnings.push(format!(
                    "'{ch}' can be hard to read on screen — consider removing it"
                ));
            }
        }
        warnings
    }

    pub fn validate_hotkey(&self) -> Result<()> {
        let mods = self.hint_hotkey_modifiers & 0x0F;
        if mods == 0 {
            bail!("hotkey must include at least one modifier (Ctrl, Shift, Alt, or Win)");
        }
        if is_modifier_vk(self.hint_hotkey_vk) {
            bail!("hotkey must include a non-modifier key");
        }
        Ok(())
    }

    pub fn hotkey_ctrl(&self) -> bool {
        self.hint_hotkey_modifiers & MOD_CONTROL != 0
    }

    pub fn hotkey_shift(&self) -> bool {
        self.hint_hotkey_modifiers & MOD_SHIFT != 0
    }

    pub fn hotkey_alt(&self) -> bool {
        self.hint_hotkey_modifiers & MOD_ALT != 0
    }

    pub fn hotkey_win(&self) -> bool {
        self.hint_hotkey_modifiers & MOD_WIN != 0
    }

    pub fn set_hotkey_from_parts(&mut self, ctrl: bool, shift: bool, alt: bool, win: bool, vk: u32) {
        let mut mods = 0u32;
        if alt {
            mods |= MOD_ALT;
        }
        if ctrl {
            mods |= MOD_CONTROL;
        }
        if shift {
            mods |= MOD_SHIFT;
        }
        if win {
            mods |= MOD_WIN;
        }
        self.hint_hotkey_modifiers = mods;
        self.hint_hotkey_vk = vk;
    }

    pub fn hotkey_display(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.hotkey_alt() {
            parts.push("Alt".into());
        }
        if self.hotkey_ctrl() {
            parts.push("Ctrl".into());
        }
        if self.hotkey_shift() {
            parts.push("Shift".into());
        }
        if self.hotkey_win() {
            parts.push("Win".into());
        }
        parts.push(vk_display_name(self.hint_hotkey_vk));
        parts.join("+")
    }
}

pub fn validate_alphabet_str(s: &str) -> Result<Vec<char>> {
    let trimmed: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if trimmed.len() < 8 {
        bail!("hint alphabet needs at least 8 unique letters");
    }
    if trimmed.len() > 26 {
        bail!("hint alphabet cannot exceed 26 letters");
    }

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for ch in trimmed.chars() {
        if !ch.is_ascii_alphabetic() {
            bail!("hint alphabet must contain only a-z letters");
        }
        let lower = ch.to_ascii_lowercase();
        if !seen.insert(lower) {
            bail!("hint alphabet contains duplicate '{lower}'");
        }
        out.push(lower);
    }
    Ok(out)
}

pub fn is_modifier_vk(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5
    )
}

fn supported_hint_keys() -> Vec<u32> {
    [0x08, 0x09, 0x0D, 0x1B, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2D, 0x2E]
        .into_iter()
        .chain(0x30..=0x39)
        .chain(0x41..=0x5A)
        .chain(0x60..=0x69)
        .chain(0x70..=0x87)
        .collect()
}

pub fn parse_hint_key(text: &str) -> Result<u32> {
    supported_hint_keys()
        .into_iter()
        .find(|&vk| vk_display_name(vk).eq_ignore_ascii_case(text.trim()))
        .with_context(|| format!("Unknown key '{}'", text.trim()))
}

pub fn vk_display_name(vk: u32) -> String {
    match vk {
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x30..=0x39 => format!("{}", (vk - 0x30) as u8 as char),
        0x41..=0x5A => char::from_u32(vk).unwrap_or('?').to_string(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0xBA..=0xC0 => format!("Oem{}", vk),
        0xDB..=0xDF => format!("Oem{}", vk),
        _ => format!("VK_{vk:02X}"),
    }
}

pub fn bgr_to_rgba(bgr: u32) -> [f32; 4] {
    let b = (bgr & 0xFF) as f32 / 255.0;
    let g = ((bgr >> 8) & 0xFF) as f32 / 255.0;
    let r = ((bgr >> 16) & 0xFF) as f32 / 255.0;
    [r, g, b, 1.0]
}

pub fn rgba_to_bgr(rgba: [f32; 4]) -> u32 {
    let r = (rgba[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (rgba[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (rgba[2].clamp(0.0, 1.0) * 255.0) as u32;
    b | (g << 8) | (r << 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_config_gets_default_bindings_and_keeps_its_palette() {
        let config: Config = toml::from_str("badge_bg_color = 123").unwrap();
        assert_eq!(config.badge_bg_color, 123);
        assert_eq!(config.switch_mode_vk, 0x09);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn shortcut_validation_rejects_duplicates_and_hint_letters() {
        let mut config = Config::default();
        config.switch_mode_vk = config.left_click_vk;
        assert!(config.validate().is_err());
        config.switch_mode_vk = parse_hint_key("q").unwrap();
        assert!(config.validate().is_err());
        config.switch_mode_vk = parse_hint_key("F8").unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn parse_default_alphabet() {
        let cfg = Config::default();
        assert_eq!(cfg.parse_alphabet().unwrap().len(), 15);
    }

    #[test]
    fn reject_duplicate_letters() {
        assert!(validate_alphabet_str("aabbccdd").is_err());
    }

    #[test]
    fn hotkey_display_default() {
        assert_eq!(Config::default().hotkey_display(), "Ctrl+Shift+Space");
    }

    #[test]
    fn left_right_modifier_vks_are_modifiers() {
        assert!(is_modifier_vk(0xA0));
        assert!(is_modifier_vk(0xA2));
        assert!(!is_modifier_vk(0x48));
    }
}
