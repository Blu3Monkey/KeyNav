mod invoke;
mod scanner;

pub use invoke::{activate_element, ClickButton};
pub use scanner::{scan_foreground_window, write_hint_diagnostics, ScannedElement};
