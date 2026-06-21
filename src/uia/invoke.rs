use anyhow::{Context, Result};
use windows::Win32::UI::Accessibility::{
    IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern, IUIAutomationSelectionItemPattern,
    IUIAutomationTogglePattern, IUIAutomationValuePattern, UIA_ExpandCollapsePatternId,
    UIA_InvokePatternId, UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId,
    ExpandCollapseState_Collapsed, ExpandCollapseState_LeafNode,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{SetCursorPos, SetForegroundWindow};

use super::scanner::ScannedElement;

pub fn activate_element(element: &ScannedElement) -> Result<()> {
    unsafe {
        let _ = SetForegroundWindow(element.target_hwnd);
    }

    if try_invoke(&element.element).is_ok() {
        return Ok(());
    }
    if try_toggle(&element.element).is_ok() {
        return Ok(());
    }
    if try_select(&element.element).is_ok() {
        return Ok(());
    }
    if try_expand(&element.element).is_ok() {
        return Ok(());
    }
    if try_focus_value(&element.element).is_ok() {
        return Ok(());
    }

    click_center(element)?;
    Ok(())
}

fn try_invoke(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> Result<()> {
    let pattern = unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            .context("invoke pattern")?
    };
    unsafe { pattern.Invoke() }.ok();
    Ok(())
}

fn try_toggle(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> Result<()> {
    let pattern = unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
            .context("toggle pattern")?
    };
    unsafe { pattern.Toggle() }.ok();
    Ok(())
}

fn try_select(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> Result<()> {
    let pattern = unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
            .context("selection pattern")?
    };
    unsafe { pattern.Select() }.ok();
    Ok(())
}

fn try_expand(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> Result<()> {
    let pattern = unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId)
            .context("expand pattern")?
    };
    let state = unsafe { pattern.CurrentExpandCollapseState() }.unwrap_or(ExpandCollapseState_LeafNode);
    if state == ExpandCollapseState_Collapsed {
        unsafe { pattern.Expand() }.ok();
    }
    Ok(())
}

fn try_focus_value(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> Result<()> {
    let _pattern = unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .context("value pattern")?
    };
    unsafe {
        element.SetFocus().ok();
    }
    Ok(())
}

fn click_center(element: &ScannedElement) -> Result<()> {
    let (x, y) = element.rect.center();
    unsafe {
        SetCursorPos(x, y).context("SetCursorPos")?;
        let down = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: MOUSEEVENTF_LEFTDOWN,
                    ..Default::default()
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: MOUSEEVENTF_LEFTUP,
                    ..Default::default()
                },
            },
        };
        SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
    }
    Ok(())
}
