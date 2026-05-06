//! Windows UIAutomation accessibility tree extraction.
//!
//! Walks the UIA control view starting from the desktop root and produces
//! a sparse JSON tree compatible with the Android `ui_tree` command output.

use serde_json::{json, Value};

use crate::config::Config;
use crate::commands::{get_output_scale, scale_bounds_in_value};

// ── Option types for parse_ui_tree_opts ──────────────────────────────────────

/// Parsed options for the ui_tree command.
#[derive(Debug, Clone)]
pub(crate) struct UiTreeOpts {
    pub window: Option<WindowSelector>,
    pub region: Option<Region>,
    pub region_mode: RegionMode,
    pub types: Option<Vec<String>>, // lowercased for case-insensitive match
    pub text_match: Option<TextMatcher>,
    pub max_depth: u32,
    pub format: OutputFormat,
    pub fields: Option<Vec<NodeField>>, // None = format default
}

#[derive(Debug, Clone)]
pub(crate) enum WindowSelector {
    TitleSubstring(String), // lowercased
    Hwnd(u64),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Region {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegionMode {
    Inside,
    Intersect,
}

#[derive(Debug, Clone)]
pub(crate) enum TextMatcher {
    Substring(String),     // lowercased; matched case-insensitively
    Regex(regex::Regex),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Nested,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum NodeField {
    Text, Value, ControlType, ClassName, ResourceId, ContentDescription,
    Bounds, Cx, Cy,
    Enabled, Clickable, Editable, Scrollable, Checked, Focused,
    Hwnd, Path,
}

impl Default for UiTreeOpts {
    fn default() -> Self {
        Self {
            window: None,
            region: None,
            region_mode: RegionMode::Inside,
            types: None,
            text_match: None,
            max_depth: 10,
            format: OutputFormat::Nested,
            fields: None,
        }
    }
}

pub(crate) fn parse_ui_tree_opts(params: Option<&Value>) -> Result<UiTreeOpts, String> {
    let mut opts = UiTreeOpts::default();
    let p = match params {
        Some(Value::Object(_)) => params.unwrap(),
        Some(_) => return Err("params must be an object".into()),
        None => return Ok(opts),
    };

    // window: string or number
    if let Some(v) = p.get("window") {
        opts.window = Some(match v {
            Value::String(s) if !s.is_empty() => WindowSelector::TitleSubstring(s.to_lowercase()),
            Value::Number(n) => {
                let h = n.as_u64().ok_or_else(|| "window: hwnd must be a non-negative integer".to_string())?;
                WindowSelector::Hwnd(h)
            }
            Value::String(_) => return Err("window: empty string".into()),
            _ => return Err("window: must be string (title) or number (hwnd)".into()),
        });
    }

    // region: { min_x, min_y, max_x, max_y }
    if let Some(v) = p.get("region") {
        let obj = v.as_object().ok_or_else(|| "region: must be an object".to_string())?;
        let g = |k: &str| -> Result<i32, String> {
            obj.get(k).and_then(|x| x.as_i64()).map(|x| x as i32)
                .ok_or_else(|| format!("region.{k}: missing or not an integer"))
        };
        let r = Region { min_x: g("min_x")?, min_y: g("min_y")?, max_x: g("max_x")?, max_y: g("max_y")? };
        if r.max_x <= r.min_x || r.max_y <= r.min_y {
            return Err("region: max_x/max_y must be greater than min_x/min_y".into());
        }
        opts.region = Some(r);
    }

    // region_mode
    if let Some(v) = p.get("region_mode") {
        opts.region_mode = match v.as_str() {
            Some("inside") => RegionMode::Inside,
            Some("intersect") => RegionMode::Intersect,
            _ => return Err("region_mode: must be 'inside' or 'intersect'".into()),
        };
    }

    // types: case-insensitive whitelist
    if let Some(v) = p.get("types") {
        let arr = v.as_array().ok_or_else(|| "types: must be an array of strings".to_string())?;
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            let s = item.as_str().ok_or_else(|| "types: each entry must be a string".to_string())?;
            out.push(s.to_lowercase());
        }
        if !out.is_empty() {
            opts.types = Some(out);
        }
    }

    // text_match + regex
    let regex_flag = p.get("regex").and_then(|v| v.as_bool()).unwrap_or(false);
    if let Some(v) = p.get("text_match") {
        let s = v.as_str().ok_or_else(|| "text_match: must be a string".to_string())?;
        opts.text_match = Some(if regex_flag {
            TextMatcher::Regex(regex::Regex::new(s).map_err(|e| format!("text_match: invalid regex: {e}"))?)
        } else {
            TextMatcher::Substring(s.to_lowercase())
        });
    }

    // max_depth
    if let Some(v) = p.get("max_depth") {
        let n = v.as_i64().ok_or_else(|| "max_depth: must be an integer".to_string())?;
        if n < 1 {
            return Err("max_depth: must be >= 1".into());
        }
        opts.max_depth = n as u32;
    }

    // format
    if let Some(v) = p.get("format") {
        opts.format = match v.as_str() {
            Some("nested") => OutputFormat::Nested,
            Some("flat") => OutputFormat::Flat,
            _ => return Err("format: must be 'nested' or 'flat'".into()),
        };
    }

    // fields
    if let Some(v) = p.get("fields") {
        let arr = v.as_array().ok_or_else(|| "fields: must be an array of strings".to_string())?;
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            let s = item.as_str().ok_or_else(|| "fields: each entry must be a string".to_string())?;
            out.push(parse_node_field(s)?);
        }
        opts.fields = Some(out);
    }

    Ok(opts)
}

fn parse_node_field(name: &str) -> Result<NodeField, String> {
    Ok(match name {
        "text" => NodeField::Text,
        "value" => NodeField::Value,
        "controlType" => NodeField::ControlType,
        "className" => NodeField::ClassName,
        "resourceId" => NodeField::ResourceId,
        "contentDescription" => NodeField::ContentDescription,
        "bounds" => NodeField::Bounds,
        "cx" => NodeField::Cx,
        "cy" => NodeField::Cy,
        "enabled" => NodeField::Enabled,
        "clickable" => NodeField::Clickable,
        "editable" => NodeField::Editable,
        "scrollable" => NodeField::Scrollable,
        "checked" => NodeField::Checked,
        "focused" => NodeField::Focused,
        "hwnd" => NodeField::Hwnd,
        "path" => NodeField::Path,
        other => return Err(format!("fields: unknown field '{other}'")),
    })
}

// ── End option types ──────────────────────────────────────────────────────────

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

/// Returns true if the node passes all per-node display filters
/// (`types`, `text_match`, `region`).
///
/// `bounds` is `[left, top, right, bottom]` in viewport coords (pre-scale).
/// `text` and `control_type` are the raw UIA values (PascalCase for control_type).
pub(crate) fn node_passes_display_filter(
    opts: &UiTreeOpts,
    control_type: &str,
    text: &str,
    bounds: Option<&[i32; 4]>,
) -> bool {
    if let Some(ref types) = opts.types {
        let lc = control_type.to_lowercase();
        if !types.iter().any(|t| t == &lc) {
            return false;
        }
    }
    if let Some(ref tm) = opts.text_match {
        match tm {
            TextMatcher::Substring(needle) => {
                if !text.to_lowercase().contains(needle.as_str()) {
                    return false;
                }
            }
            TextMatcher::Regex(re) => {
                if !re.is_match(text) {
                    return false;
                }
            }
        }
    }
    if let Some(ref r) = opts.region {
        let b = match bounds {
            Some(b) => b,
            None => return false,
        };
        match opts.region_mode {
            RegionMode::Inside => {
                if !(b[0] >= r.min_x && b[1] >= r.min_y && b[2] <= r.max_x && b[3] <= r.max_y) {
                    return false;
                }
            }
            RegionMode::Intersect => {
                if b[2] <= r.min_x || b[0] >= r.max_x || b[3] <= r.min_y || b[1] >= r.max_y {
                    return false;
                }
            }
        }
    }
    true
}

/// Joins ancestor labels with " / " to produce the `path` field for flat-mode nodes.
/// Caller is responsible for picking the label per ancestor (text or fallback to controlType).
pub(crate) fn build_path(ancestors: &[&str]) -> String {
    ancestors.join(" / ")
}

/// Snapshot of a UIA element's properties — the raw input to `build_node_value`.
/// Walker fills this from COM calls; tests construct it directly.
#[derive(Debug, Default, Clone)]
pub(crate) struct RawNode {
    pub text: String,
    pub value: String,
    pub control_type: String, // PascalCase, e.g. "Button"
    pub class_name: String,
    pub resource_id: String,
    pub content_description: String,
    pub bounds: Option<[i32; 4]>, // [left, top, right, bottom]
    pub enabled: bool,            // default true
    pub clickable: bool,
    pub editable: bool,
    pub scrollable: bool,
    pub checkable: bool,
    pub checked: bool,
    pub focusable: bool,
    pub focused: bool,
    pub hwnd: u64,
    pub path: String, // pre-built breadcrumb (flat mode)
}

impl RawNode {
    pub fn for_test() -> Self {
        Self { enabled: true, ..Default::default() }
    }
}

/// Default field set when `fields` is unset and format is Nested.
/// This must match the legacy output exactly.
fn default_nested_fields() -> &'static [NodeField] {
    use NodeField::*;
    &[Text, Value, ControlType, ClassName, ResourceId, ContentDescription,
      Bounds, Enabled, Clickable, Editable, Scrollable, Checked, Focused, Hwnd]
}

/// Default field set when `fields` is unset and format is Flat.
fn default_flat_fields() -> &'static [NodeField] {
    use NodeField::*;
    &[ControlType, Text, Cx, Cy, Hwnd, Path]
}

/// Returns the active field set: explicit `opts.fields` if set, else format default.
pub(crate) fn active_fields<'a>(opts: &'a UiTreeOpts) -> &'a [NodeField] {
    if let Some(ref f) = opts.fields {
        f.as_slice()
    } else {
        match opts.format {
            OutputFormat::Nested => default_nested_fields(),
            OutputFormat::Flat => default_flat_fields(),
        }
    }
}

/// Returns true if the field should be emitted given the active set.
fn wants(fields: &[NodeField], f: NodeField) -> bool {
    fields.iter().any(|x| *x == f)
}

/// Build the per-node JSON object honoring `fields` selection.
/// `controlType` is always emitted (filter logic depends on it).
/// `children` is appended by the caller in nested mode (not handled here).
pub(crate) fn build_node_value(node: &RawNode, opts: &UiTreeOpts) -> Value {
    let fields = active_fields(opts);
    let mut out = serde_json::Map::new();

    // 1. text / identity
    if wants(fields, NodeField::Text) && !node.text.is_empty() {
        out.insert("text".into(), json!(node.text));
    }
    if wants(fields, NodeField::Value) && node.editable && !node.value.is_empty() {
        out.insert("value".into(), json!(node.value));
    }
    // controlType: always emitted
    out.insert("controlType".into(), json!(node.control_type));
    if wants(fields, NodeField::ClassName) && !node.class_name.is_empty() {
        out.insert("className".into(), json!(node.class_name));
    }
    if wants(fields, NodeField::ResourceId) && !node.resource_id.is_empty() {
        out.insert("resourceId".into(), json!(node.resource_id));
    }
    if wants(fields, NodeField::ContentDescription) && !node.content_description.is_empty() {
        out.insert("contentDescription".into(), json!(node.content_description));
    }

    // 2. bounds + cx/cy
    if wants(fields, NodeField::Bounds) {
        if let Some(b) = node.bounds {
            out.insert("bounds".into(), json!({
                "left": b[0], "top": b[1], "right": b[2], "bottom": b[3],
                "width": b[2] - b[0], "height": b[3] - b[1],
            }));
        } else {
            out.insert("bounds".into(), json!({"left":0,"top":0,"right":0,"bottom":0,"width":0,"height":0}));
        }
    }
    if wants(fields, NodeField::Cx) {
        if let Some(b) = node.bounds {
            out.insert("cx".into(), json!((b[0] + b[2]) / 2));
        }
    }
    if wants(fields, NodeField::Cy) {
        if let Some(b) = node.bounds {
            out.insert("cy".into(), json!((b[1] + b[3]) / 2));
        }
    }

    // 3. state flags (sparse — only non-defaults)
    if wants(fields, NodeField::Enabled) && !node.enabled {
        out.insert("enabled".into(), json!(false));
    }
    if wants(fields, NodeField::Clickable) && node.clickable {
        out.insert("clickable".into(), json!(true));
    }
    if wants(fields, NodeField::Editable) && node.editable {
        out.insert("editable".into(), json!(true));
    }
    if wants(fields, NodeField::Scrollable) && node.scrollable {
        out.insert("scrollable".into(), json!(true));
    }
    if wants(fields, NodeField::Checked) && node.checkable {
        out.insert("checked".into(), json!(node.checked));
    }
    if wants(fields, NodeField::Focused) && node.focusable {
        out.insert("focused".into(), json!(node.focused));
    }
    if wants(fields, NodeField::Hwnd) && node.hwnd != 0 {
        out.insert("hwnd".into(), json!(node.hwnd));
    }

    // 4. path (only meaningful in flat mode; emitted if requested)
    if wants(fields, NodeField::Path) {
        out.insert("path".into(), json!(node.path));
    }

    Value::Object(out)
}

#[cfg(test)]
mod parse_tests {
    use super::*;
    use serde_json::json;

    fn parse(v: serde_json::Value) -> Result<UiTreeOpts, String> {
        parse_ui_tree_opts(Some(&v))
    }

    #[test]
    fn no_params_returns_defaults() {
        let opts = parse_ui_tree_opts(None).unwrap();
        assert!(matches!(opts.format, OutputFormat::Nested));
        assert_eq!(opts.max_depth, 10);
        assert!(matches!(opts.region_mode, RegionMode::Inside));
        assert!(opts.window.is_none());
        assert!(opts.region.is_none());
        assert!(opts.types.is_none());
        assert!(opts.text_match.is_none());
        assert!(opts.fields.is_none());
    }

    #[test]
    fn empty_object_returns_defaults() {
        let opts = parse(json!({})).unwrap();
        assert_eq!(opts.max_depth, 10);
    }

    #[test]
    fn window_string_lowercased() {
        let opts = parse(json!({"window": "Notepad"})).unwrap();
        match opts.window.unwrap() {
            WindowSelector::TitleSubstring(s) => assert_eq!(s, "notepad"),
            _ => panic!("expected title substring"),
        }
    }

    #[test]
    fn window_number_is_hwnd() {
        let opts = parse(json!({"window": 12345})).unwrap();
        match opts.window.unwrap() {
            WindowSelector::Hwnd(h) => assert_eq!(h, 12345),
            _ => panic!("expected hwnd"),
        }
    }

    #[test]
    fn region_parsed() {
        let opts = parse(json!({"region": {"min_x": 10, "min_y": 20, "max_x": 100, "max_y": 200}})).unwrap();
        let r = opts.region.unwrap();
        assert_eq!(r.min_x, 10);
        assert_eq!(r.max_x, 100);
    }

    #[test]
    fn region_invalid_when_max_le_min() {
        let err = parse(json!({"region": {"min_x": 100, "min_y": 0, "max_x": 50, "max_y": 200}}));
        assert!(err.is_err());
    }

    #[test]
    fn region_mode_intersect() {
        let opts = parse(json!({"region_mode": "intersect"})).unwrap();
        assert_eq!(opts.region_mode, RegionMode::Intersect);
    }

    #[test]
    fn region_mode_invalid() {
        assert!(parse(json!({"region_mode": "outside"})).is_err());
    }

    #[test]
    fn types_lowercased() {
        let opts = parse(json!({"types": ["Button", "EDIT", "menuItem"]})).unwrap();
        assert_eq!(opts.types.unwrap(), vec!["button", "edit", "menuitem"]);
    }

    #[test]
    fn text_match_substring_default() {
        let opts = parse(json!({"text_match": "Save"})).unwrap();
        match opts.text_match.unwrap() {
            TextMatcher::Substring(s) => assert_eq!(s, "save"),
            _ => panic!("expected substring"),
        }
    }

    #[test]
    fn text_match_regex_when_flag_set() {
        let opts = parse(json!({"text_match": "^Sa.+$", "regex": true})).unwrap();
        match opts.text_match.unwrap() {
            TextMatcher::Regex(_) => {},
            _ => panic!("expected regex"),
        }
    }

    #[test]
    fn invalid_regex_errors() {
        assert!(parse(json!({"text_match": "[", "regex": true})).is_err());
    }

    #[test]
    fn max_depth_zero_errors() {
        assert!(parse(json!({"max_depth": 0})).is_err());
    }

    #[test]
    fn format_flat() {
        let opts = parse(json!({"format": "flat"})).unwrap();
        assert_eq!(opts.format, OutputFormat::Flat);
    }

    #[test]
    fn format_invalid_errors() {
        assert!(parse(json!({"format": "tabular"})).is_err());
    }

    #[test]
    fn fields_recognized() {
        let opts = parse(json!({"fields": ["text", "cx", "cy", "hwnd", "path"]})).unwrap();
        let fs = opts.fields.unwrap();
        assert_eq!(fs.len(), 5);
        assert!(fs.contains(&NodeField::Cx));
        assert!(fs.contains(&NodeField::Path));
    }

    #[test]
    fn unknown_field_errors() {
        let err = parse(json!({"fields": ["text", "blorp"]})).unwrap_err();
        assert!(err.contains("blorp"));
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    fn opts_with_types(types: Vec<&str>) -> UiTreeOpts {
        let mut o = UiTreeOpts::default();
        o.types = Some(types.into_iter().map(|s| s.to_lowercase()).collect());
        o
    }

    fn opts_with_text(s: &str) -> UiTreeOpts {
        let mut o = UiTreeOpts::default();
        o.text_match = Some(TextMatcher::Substring(s.to_lowercase()));
        o
    }

    fn opts_with_regex(re: &str) -> UiTreeOpts {
        let mut o = UiTreeOpts::default();
        o.text_match = Some(TextMatcher::Regex(regex::Regex::new(re).unwrap()));
        o
    }

    fn opts_with_region(r: Region, mode: RegionMode) -> UiTreeOpts {
        let mut o = UiTreeOpts::default();
        o.region = Some(r);
        o.region_mode = mode;
        o
    }

    #[test]
    fn no_filters_passes() {
        let opts = UiTreeOpts::default();
        assert!(node_passes_display_filter(&opts, "Button", "Save", Some(&[10, 10, 50, 50])));
    }

    #[test]
    fn types_match_case_insensitive() {
        let opts = opts_with_types(vec!["button", "edit"]);
        assert!(node_passes_display_filter(&opts, "Button", "Save", None));
        assert!(node_passes_display_filter(&opts, "BUTTON", "Save", None));
        assert!(!node_passes_display_filter(&opts, "Pane", "Save", None));
    }

    #[test]
    fn text_substring_case_insensitive() {
        let opts = opts_with_text("save");
        assert!(node_passes_display_filter(&opts, "Button", "Save As...", None));
        assert!(node_passes_display_filter(&opts, "Button", "save", None));
        assert!(!node_passes_display_filter(&opts, "Button", "Cancel", None));
    }

    #[test]
    fn text_regex_matches_anchored_pattern() {
        let opts = opts_with_regex(r"^Save( As)?$");
        assert!(node_passes_display_filter(&opts, "Button", "Save", None));
        assert!(node_passes_display_filter(&opts, "Button", "Save As", None));
        assert!(!node_passes_display_filter(&opts, "Button", "Save As...", None));
    }

    #[test]
    fn region_inside_strict() {
        let r = Region { min_x: 0, min_y: 0, max_x: 100, max_y: 100 };
        let opts = opts_with_region(r, RegionMode::Inside);
        assert!(node_passes_display_filter(&opts, "Button", "X", Some(&[10, 10, 50, 50])));
        assert!(!node_passes_display_filter(&opts, "Button", "X", Some(&[10, 10, 150, 50])));
        assert!(!node_passes_display_filter(&opts, "Button", "X", Some(&[-10, 10, 50, 50])));
    }

    #[test]
    fn region_intersect_partial_overlap() {
        let r = Region { min_x: 0, min_y: 0, max_x: 100, max_y: 100 };
        let opts = opts_with_region(r, RegionMode::Intersect);
        assert!(node_passes_display_filter(&opts, "Button", "X", Some(&[10, 10, 50, 50])));
        assert!(node_passes_display_filter(&opts, "Button", "X", Some(&[80, 80, 200, 200])));
        assert!(!node_passes_display_filter(&opts, "Button", "X", Some(&[200, 200, 300, 300])));
    }

    #[test]
    fn region_with_no_bounds_fails() {
        let r = Region { min_x: 0, min_y: 0, max_x: 100, max_y: 100 };
        let opts = opts_with_region(r, RegionMode::Inside);
        assert!(!node_passes_display_filter(&opts, "Button", "X", None));
    }

    #[test]
    fn all_filters_combined_must_all_match() {
        let mut opts = opts_with_types(vec!["button"]);
        opts.text_match = Some(TextMatcher::Substring("save".into()));
        opts.region = Some(Region { min_x: 0, min_y: 0, max_x: 100, max_y: 100 });
        opts.region_mode = RegionMode::Inside;
        assert!(node_passes_display_filter(&opts, "Button", "Save", Some(&[10, 10, 50, 50])));
        assert!(!node_passes_display_filter(&opts, "Pane", "Save", Some(&[10, 10, 50, 50])));
        assert!(!node_passes_display_filter(&opts, "Button", "Cancel", Some(&[10, 10, 50, 50])));
        assert!(!node_passes_display_filter(&opts, "Button", "Save", Some(&[10, 10, 200, 50])));
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn empty_when_no_ancestors() {
        assert_eq!(build_path(&[]), "");
    }

    #[test]
    fn single_ancestor_just_its_label() {
        assert_eq!(build_path(&["Notepad"]), "Notepad");
    }

    #[test]
    fn joins_with_slash_space() {
        assert_eq!(build_path(&["Notepad", "Document area", "File menu"]),
                   "Notepad / Document area / File menu");
    }
}

#[cfg(test)]
mod build_node_tests {
    use super::*;

    fn button(text: &str, bounds: [i32; 4]) -> RawNode {
        RawNode {
            text: text.into(),
            control_type: "Button".into(),
            bounds: Some(bounds),
            clickable: true,
            ..RawNode::for_test()
        }
    }

    #[test]
    fn nested_default_emits_legacy_fields() {
        let n = button("Save", [10, 20, 110, 60]);
        let opts = UiTreeOpts::default();
        let v = build_node_value(&n, &opts);
        let obj = v.as_object().unwrap();
        assert_eq!(obj.get("text").unwrap(), "Save");
        assert_eq!(obj.get("controlType").unwrap(), "Button");
        assert_eq!(obj.get("clickable").unwrap(), true);
        assert!(obj.contains_key("bounds"));
        // legacy field set excludes cx/cy/path
        assert!(!obj.contains_key("cx"));
        assert!(!obj.contains_key("cy"));
        assert!(!obj.contains_key("path"));
    }

    #[test]
    fn flat_default_includes_cx_cy_path_drops_bounds() {
        let mut n = button("Save", [10, 20, 110, 60]);
        n.path = "Notepad / File menu".into();
        n.hwnd = 7777;
        let mut opts = UiTreeOpts::default();
        opts.format = OutputFormat::Flat;
        let v = build_node_value(&n, &opts);
        let obj = v.as_object().unwrap();
        assert_eq!(obj.get("controlType").unwrap(), "Button");
        assert_eq!(obj.get("text").unwrap(), "Save");
        assert_eq!(obj.get("cx").unwrap(), 60);
        assert_eq!(obj.get("cy").unwrap(), 40);
        assert_eq!(obj.get("hwnd").unwrap(), 7777);
        assert_eq!(obj.get("path").unwrap(), "Notepad / File menu");
        assert!(!obj.contains_key("bounds"));
    }

    #[test]
    fn explicit_fields_only_emits_requested() {
        let n = button("Save", [10, 20, 110, 60]);
        let mut opts = UiTreeOpts::default();
        opts.fields = Some(vec![NodeField::Text, NodeField::Cx, NodeField::Cy]);
        let v = build_node_value(&n, &opts);
        let obj = v.as_object().unwrap();
        assert_eq!(obj.get("text").unwrap(), "Save");
        assert_eq!(obj.get("cx").unwrap(), 60);
        assert_eq!(obj.get("cy").unwrap(), 40);
        // controlType is always implicit
        assert_eq!(obj.get("controlType").unwrap(), "Button");
        // Everything else absent
        assert!(!obj.contains_key("bounds"));
        assert!(!obj.contains_key("clickable"));
        assert!(!obj.contains_key("hwnd"));
    }

    #[test]
    fn sparse_rule_drops_empty_text() {
        let n = RawNode { control_type: "Pane".into(), ..RawNode::for_test() };
        let opts = UiTreeOpts::default();
        let v = build_node_value(&n, &opts);
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("text"));
        assert_eq!(obj.get("controlType").unwrap(), "Pane");
    }

    #[test]
    fn cx_cy_absent_when_no_bounds() {
        let n = RawNode {
            text: "Save".into(),
            control_type: "Button".into(),
            ..RawNode::for_test()
        };
        let mut opts = UiTreeOpts::default();
        opts.format = OutputFormat::Flat;
        let v = build_node_value(&n, &opts);
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("cx"));
        assert!(!obj.contains_key("cy"));
    }
}
