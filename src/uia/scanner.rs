use anyhow::{Context, Result};
use std::cell::RefCell;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Accessibility::{
    IUIAutomation, IUIAutomationElement, CUIAutomation, TreeScope_Descendants,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};

thread_local! {
    static AUTOMATION: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn area(&self) -> i64 {
        self.width() as i64 * self.height() as i64
    }

    pub fn center(&self) -> (i32, i32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }

    pub fn contains(&self, other: &ScreenRect) -> bool {
        self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }
}

#[derive(Debug, Clone)]
pub struct ScannedElement {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub control_type: i32,
    pub rect: ScreenRect,
    pub element: IUIAutomationElement,
    pub target_hwnd: HWND,
}

const MIN_AREA: i64 = 36;
const MIN_DIMENSION: i32 = 3;

fn automation() -> Result<IUIAutomation> {
    AUTOMATION.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            let auto: IUIAutomation =
                unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                    .context("create IUIAutomation")?;
            *slot = Some(auto);
        }
        Ok(slot.as_ref().unwrap().clone())
    })
}

pub fn scan_foreground_window(max_elements: usize) -> Result<Vec<ScannedElement>> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        log::warn!("scan: no foreground window");
        return Ok(Vec::new());
    }

    let automation = automation()?;
    let root = unsafe {
        automation
            .ElementFromHandle(hwnd)
            .context("ElementFromHandle")?
    };

    let condition = unsafe {
        automation
            .CreateTrueCondition()
            .context("CreateTrueCondition")?
    };

    let array = unsafe {
        root.FindAll(TreeScope_Descendants, &condition)
            .context("FindAll")?
    };

    let count = unsafe { array.Length().unwrap_or(0) };
    log::info!("scan: foreground hwnd={:?}, raw elements={count}", hwnd.0);

    let mut candidates = Vec::new();
    for i in 0..count {
        if candidates.len() >= max_elements {
            break;
        }
        let el = match unsafe { array.GetElement(i) } {
            Ok(e) => e,
            Err(_) => continue,
        };
        if let Some(scanned) = try_element(&el, hwnd)? {
            candidates.push(scanned);
        }
    }

    log::info!("scan: {} candidates after filter", candidates.len());

    deduplicate(&mut candidates);
    candidates.sort_by(|a, b| {
        a.rect
            .top
            .cmp(&b.rect.top)
            .then(a.rect.left.cmp(&b.rect.left))
    });
    candidates.truncate(max_elements);
    Ok(candidates)
}

fn try_element(element: &IUIAutomationElement, target_hwnd: HWND) -> Result<Option<ScannedElement>> {
    if !is_enabled(element) {
        return Ok(None);
    }

    if !is_actionable(element) {
        return Ok(None);
    }

    let rect = element_rect(element, target_hwnd)?;
    if rect.width() < MIN_DIMENSION || rect.height() < MIN_DIMENSION || rect.area() < MIN_AREA {
        return Ok(None);
    }

    // Skip elements clearly off-screen (but don't trust IsOffscreen alone — many
    // apps report visible controls as offscreen).
    if is_offscreen(element) && !rect_on_screen(&rect) {
        return Ok(None);
    }

    let name = get_name(element).unwrap_or_default();
    let control_type = get_control_type(element);
    Ok(Some(ScannedElement {
        name,
        control_type,
        rect,
        element: element.clone(),
        target_hwnd,
    }))
}

fn rect_on_screen(rect: &ScreenRect) -> bool {
    rect.width() > 0 && rect.height() > 0 && rect.right > 0 && rect.bottom > 0
}

fn is_enabled(element: &IUIAutomationElement) -> bool {
    unsafe {
        element
            .CurrentIsEnabled()
            .map(|b| b.as_bool())
            .unwrap_or(true)
    }
}

fn is_offscreen(element: &IUIAutomationElement) -> bool {
    unsafe {
        element
            .CurrentIsOffscreen()
            .map(|b| b.as_bool())
            .unwrap_or(false)
    }
}

fn get_control_type(element: &IUIAutomationElement) -> i32 {
    unsafe { element.CurrentControlType().map(|t| t.0 as i32).unwrap_or(-1) }
}

fn get_name(element: &IUIAutomationElement) -> Option<String> {
    unsafe { element.CurrentName().ok().map(|s| s.to_string()) }
}

fn has_invoke_pattern(element: &IUIAutomationElement) -> bool {
    use windows::Win32::UI::Accessibility::{
        IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern, IUIAutomationLegacyIAccessiblePattern,
        IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern, IUIAutomationValuePattern,
        UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_LegacyIAccessiblePatternId,
        UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId,
    };

    unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            .is_ok()
            || element
                .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
                .is_ok()
            || element
                .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
                .is_ok()
            || element
                .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId)
                .is_ok()
            || element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .is_ok()
            || element
                .GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(UIA_LegacyIAccessiblePatternId)
                .is_ok()
    }
}

fn is_actionable(element: &IUIAutomationElement) -> bool {
    if has_invoke_pattern(element) {
        return true;
    }
    // Menu items and similar controls are often keyboard-focusable without Invoke.
    unsafe {
        element
            .CurrentIsKeyboardFocusable()
            .map(|b| b.as_bool())
            .unwrap_or(false)
    }
}

fn element_rect(element: &IUIAutomationElement, hwnd: HWND) -> Result<ScreenRect> {
    let rect = unsafe { element.CurrentBoundingRectangle() }.context("bounding rect")?;
    let mut screen_rect = ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };

    if screen_rect.width() <= 0 || screen_rect.height() <= 0 {
        let mut wr = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut wr) }.ok();
        screen_rect = ScreenRect {
            left: wr.left,
            top: wr.top,
            right: wr.right,
            bottom: wr.bottom,
        };
    }

    Ok(screen_rect)
}

fn deduplicate(elements: &mut Vec<ScannedElement>) {
    let mut keep = vec![true; elements.len()];
    for i in 0..elements.len() {
        for j in 0..elements.len() {
            if i == j || !keep[i] || !keep[j] {
                continue;
            }
            if elements[i].rect.contains(&elements[j].rect) && elements[j].rect.area() < elements[i].rect.area()
            {
                keep[i] = false;
            } else if elements[j].rect.contains(&elements[i].rect)
                && elements[i].rect.area() < elements[j].rect.area()
            {
                keep[j] = false;
            }
        }
    }
    let filtered: Vec<_> = elements
        .drain(..)
        .enumerate()
        .filter_map(|(idx, el)| if keep[idx] { Some(el) } else { None })
        .collect();
    *elements = filtered;
}
