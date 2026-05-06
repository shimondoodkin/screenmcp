//! Windows UIAutomation accessibility tree extraction.
//!
//! Walks the UIA control view starting from the desktop root and produces
//! a sparse JSON tree compatible with the Android `ui_tree` command output.

use serde_json::{json, Value};

use crate::config::Config;
use crate::commands::{get_output_scale, scale_bounds_in_value};

/// Full UIAutomation accessibility tree.
/// Walks the control view from the desktop root, extracting element properties
/// and interaction patterns to match the Android ui_tree output format.
#[cfg(windows)]
pub fn handle_ui_tree_raw() -> Result<Value, String> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::*;

    // COM guard: initialize on entry, uninitialize on drop
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize(); }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("CoInitializeEx failed: {e}"))?;
    }
    let _com = ComGuard;

    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)
            .map_err(|e| format!("failed to create IUIAutomation: {e}"))?
    };

    let walker = unsafe {
        automation
            .ControlViewWalker()
            .map_err(|e| format!("ControlViewWalker failed: {e}"))?
    };

    let root = unsafe {
        automation
            .GetRootElement()
            .map_err(|e| format!("GetRootElement failed: {e}"))?
    };

    // Virtual screen bounds (covers all monitors)
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    };
    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    let viewport = [vx, vy, vx + vw, vy + vh];

    // Walk top-level children of the desktop (each top-level window).
    // ControlViewWalker returns siblings front-to-back (z-order), so we
    // track rects of already-included siblings and skip any window whose
    // bounds are fully enclosed by one in front of it.
    let mut children = Vec::new();
    let max_depth: u32 = 10;
    let mut covered_rects: Vec<[i32; 4]> = Vec::new();

    let mut child = unsafe { walker.GetFirstChildElement(&root).ok() };
    while let Some(ref el) = child {
        if let Some(node) = walk_element(el, &walker, 1, max_depth, &mut covered_rects, &viewport) {
            children.push(node);
        }
        child = unsafe { walker.GetNextSiblingElement(el).ok() };
    }

    Ok(json!({ "tree": children, "os": "windows" }))
}

/// Check if rect `inner` is fully enclosed by rect `outer`.
/// Rects are [left, top, right, bottom].
#[cfg(windows)]
fn is_fully_enclosed(inner: &[i32; 4], outer: &[i32; 4]) -> bool {
    inner[0] >= outer[0] && inner[1] >= outer[1] && inner[2] <= outer[2] && inner[3] <= outer[3]
}

/// Recursively walk a UIAutomation element and its children.
/// `sibling_rects` tracks bounding rects of previously included siblings
/// at the same level (front-to-back z-order) for occlusion culling.
/// `viewport` is [left, top, right, bottom] of the virtual screen.
#[cfg(windows)]
fn walk_element(
    el: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    depth: u32,
    max_depth: u32,
    sibling_rects: &mut Vec<[i32; 4]>,
    viewport: &[i32; 4],
) -> Option<Value> {
    use windows::core::Interface;
    use windows::Win32::UI::Accessibility::*;

    // ── Phase 1: cheap filters (2-3 COM calls) ── skip entire subtrees early

    // Skip offscreen / invisible elements (and don't recurse into them)
    let is_offscreen = unsafe { el.CurrentIsOffscreen().ok() }
        .map(|b| b.as_bool())
        .unwrap_or(false);
    if is_offscreen {
        return None;
    }

    // Spatial filtering on bounding rect
    let bounds_raw = unsafe { el.CurrentBoundingRectangle().ok() };
    let has_real_bounds = bounds_raw.as_ref().map_or(false, |r| r.right > r.left && r.bottom > r.top);
    if has_real_bounds {
        let r = bounds_raw.as_ref().unwrap();
        let rect = [r.left, r.top, r.right, r.bottom];
        // Skip elements entirely outside the viewport
        if rect[2] <= viewport[0] || rect[0] >= viewport[2]
            || rect[3] <= viewport[1] || rect[1] >= viewport[3]
        {
            return None;
        }
        // Z-order occlusion: skip elements fully covered by a sibling in front
        if sibling_rects.iter().any(|sr| is_fully_enclosed(&rect, sr)) {
            return None;
        }
    }

    // ── Phase 2: minimal props for noise filter (2 COM calls) + recurse

    let name = unsafe { el.CurrentName().ok() }
        .map(|s| s.to_string())
        .unwrap_or_default();
    let automation_id = unsafe { el.CurrentAutomationId().ok() }
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Recurse into children (fresh sibling_rects per level for occlusion)
    let mut child_nodes = Vec::new();
    if depth < max_depth {
        let mut child_sibling_rects: Vec<[i32; 4]> = Vec::new();
        let mut child = unsafe { walker.GetFirstChildElement(el).ok() };
        while let Some(ref c) = child {
            if let Some(node) = walk_element(c, walker, depth + 1, max_depth, &mut child_sibling_rects, viewport) {
                child_nodes.push(node);
            }
            child = unsafe { walker.GetNextSiblingElement(c).ok() };
        }
    }

    // Noise filter: skip leaf nodes with empty name AND empty automationId
    if child_nodes.is_empty() && name.is_empty() && automation_id.is_empty() {
        return None;
    }

    // Skip zero/no-bounds elements that have no visible children
    if !has_real_bounds && child_nodes.is_empty() {
        return None;
    }

    // ── Phase 3: full properties + patterns (only for nodes that survive) ──

    let class_name = unsafe { el.CurrentClassName().ok() }
        .map(|s| s.to_string())
        .unwrap_or_default();
    let help_text = unsafe { el.CurrentHelpText().ok() }
        .map(|s| s.to_string())
        .unwrap_or_default();
    let control_type_id = unsafe { el.CurrentControlType().unwrap_or_default() };
    let is_enabled = unsafe { el.CurrentIsEnabled().ok() }
        .map(|b| b.as_bool())
        .unwrap_or(true);
    let is_focusable = unsafe { el.CurrentIsKeyboardFocusable().ok() }
        .map(|b| b.as_bool())
        .unwrap_or(false);
    let has_focus = if is_focusable {
        unsafe { el.CurrentHasKeyboardFocus().ok() }
            .map(|b| b.as_bool())
            .unwrap_or(false)
    } else {
        false
    };
    let native_hwnd = unsafe { el.CurrentNativeWindowHandle().ok() }
        .map(|h| h.0 as u64)
        .unwrap_or(0);

    // Bounds as {left, top, right, bottom, width, height}
    let bounds_json = bounds_raw
        .as_ref()
        .map(|r| {
            let left = r.left as i32;
            let top = r.top as i32;
            let right = r.right as i32;
            let bottom = r.bottom as i32;
            let width = right - left;
            let height = bottom - top;
            json!({"left": left, "top": top, "right": right, "bottom": bottom, "width": width, "height": height})
        })
        .unwrap_or(json!({"left": 0, "top": 0, "right": 0, "bottom": 0, "width": 0, "height": 0}));

    let clickable = unsafe {
        el.GetCurrentPattern(UIA_InvokePatternId).is_ok()
    };
    let (editable, value) = unsafe {
        match el.GetCurrentPattern(UIA_ValuePatternId) {
            Ok(pat) => {
                let vp: Result<IUIAutomationValuePattern, _> = pat.cast();
                match vp {
                    Ok(vp) => {
                        let v = vp.CurrentValue().ok().map(|s| s.to_string()).unwrap_or_default();
                        (true, v)
                    }
                    Err(_) => (true, String::new()),
                }
            }
            Err(_) => (false, String::new()),
        }
    };
    let (checkable, checked) = unsafe {
        match el.GetCurrentPattern(UIA_TogglePatternId) {
            Ok(pat) => {
                let tp: Result<IUIAutomationTogglePattern, _> = pat.cast();
                match tp {
                    Ok(tp) => {
                        let state = tp.CurrentToggleState().unwrap_or(ToggleState_Off);
                        (true, state == ToggleState_On)
                    }
                    Err(_) => (true, false),
                }
            }
            Err(_) => (false, false),
        }
    };
    let scrollable = unsafe {
        el.GetCurrentPattern(UIA_ScrollPatternId).is_ok()
    };

    let ct_name = control_type_name(control_type_id);

    // Sparse JSON: only include non-default / non-empty values.
    // Key insertion order = output order (preserve_order feature).
    let mut node = json!({});
    let m = node.as_object_mut().unwrap();

    // 1. Text / identity
    if !name.is_empty() { m.insert("text".into(), json!(name)); }
    if editable && !value.is_empty() { m.insert("value".into(), json!(value)); }
    m.insert("controlType".into(), json!(ct_name));
    if !class_name.is_empty() { m.insert("className".into(), json!(class_name)); }
    if !automation_id.is_empty() { m.insert("resourceId".into(), json!(automation_id)); }
    if !help_text.is_empty() { m.insert("contentDescription".into(), json!(help_text)); }

    // 2. Bounds
    m.insert("bounds".into(), bounds_json);

    // 3. State & interaction flags (only non-defaults)
    if !is_enabled { m.insert("enabled".into(), json!(false)); }
    if clickable { m.insert("clickable".into(), json!(true)); }
    if editable { m.insert("editable".into(), json!(true)); }
    if scrollable { m.insert("scrollable".into(), json!(true)); }
    if checkable {
        m.insert("checked".into(), json!(checked));
    }
    if is_focusable {
        m.insert("focused".into(), json!(has_focus));
    }
    if native_hwnd != 0 { m.insert("hwnd".into(), json!(native_hwnd)); }

    // 4. Children last
    if !child_nodes.is_empty() {
        m.insert("children".into(), json!(child_nodes));
    }

    // Register this element's rect so later siblings can be culled if fully behind it
    if let Some(ref r) = bounds_raw {
        let rect = [r.left, r.top, r.right, r.bottom];
        if rect[2] > rect[0] && rect[3] > rect[1] {
            sibling_rects.push(rect);
        }
    }

    Some(node)
}

/// Map UIA control type ID to a human-readable string.
#[cfg(windows)]
#[allow(non_upper_case_globals)]
fn control_type_name(id: windows::Win32::UI::Accessibility::UIA_CONTROLTYPE_ID) -> &'static str {
    use windows::Win32::UI::Accessibility::*;
    match id {
        UIA_ButtonControlTypeId => "Button",
        UIA_CalendarControlTypeId => "Calendar",
        UIA_CheckBoxControlTypeId => "CheckBox",
        UIA_ComboBoxControlTypeId => "ComboBox",
        UIA_EditControlTypeId => "Edit",
        UIA_HyperlinkControlTypeId => "Hyperlink",
        UIA_ImageControlTypeId => "Image",
        UIA_ListItemControlTypeId => "ListItem",
        UIA_ListControlTypeId => "List",
        UIA_MenuControlTypeId => "Menu",
        UIA_MenuBarControlTypeId => "MenuBar",
        UIA_MenuItemControlTypeId => "MenuItem",
        UIA_ProgressBarControlTypeId => "ProgressBar",
        UIA_RadioButtonControlTypeId => "RadioButton",
        UIA_ScrollBarControlTypeId => "ScrollBar",
        UIA_SliderControlTypeId => "Slider",
        UIA_SpinnerControlTypeId => "Spinner",
        UIA_StatusBarControlTypeId => "StatusBar",
        UIA_TabControlTypeId => "Tab",
        UIA_TabItemControlTypeId => "TabItem",
        UIA_TextControlTypeId => "Text",
        UIA_ToolBarControlTypeId => "ToolBar",
        UIA_ToolTipControlTypeId => "ToolTip",
        UIA_TreeControlTypeId => "Tree",
        UIA_TreeItemControlTypeId => "TreeItem",
        UIA_CustomControlTypeId => "Custom",
        UIA_GroupControlTypeId => "Group",
        UIA_ThumbControlTypeId => "Thumb",
        UIA_DataGridControlTypeId => "DataGrid",
        UIA_DataItemControlTypeId => "DataItem",
        UIA_DocumentControlTypeId => "Document",
        UIA_SplitButtonControlTypeId => "SplitButton",
        UIA_WindowControlTypeId => "Window",
        UIA_PaneControlTypeId => "Pane",
        UIA_HeaderControlTypeId => "Header",
        UIA_HeaderItemControlTypeId => "HeaderItem",
        UIA_TableControlTypeId => "Table",
        UIA_TitleBarControlTypeId => "TitleBar",
        UIA_SeparatorControlTypeId => "Separator",
        UIA_SemanticZoomControlTypeId => "SemanticZoom",
        UIA_AppBarControlTypeId => "AppBar",
        _ => "Unknown",
    }
}

#[cfg(not(windows))]
pub fn handle_ui_tree_raw() -> Result<Value, String> {
    Err("ui_tree is not supported on this platform".to_string())
}

pub fn handle_ui_tree(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let result = handle_ui_tree_raw()?;
    let (sx, sy) = get_output_scale(params, config)?;
    if sx == 1.0 && sy == 1.0 {
        return Ok(result);
    }
    Ok(scale_bounds_in_value(&result, sx, sy))
}
