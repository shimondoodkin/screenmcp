# ui_tree filters and flat output — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `window`, `region`, `region_mode`, `types`, `text_match`, `regex`, `max_depth`, `format`, and `fields` parameters to the Windows `ui_tree` command, plus a flat output mode with precomputed center coordinates and breadcrumb paths.

**Architecture:** Extract Windows `ui_tree` logic from the 1930-line `commands.rs` into a focused `ui_tree.rs` module. Add a `UiTreeOpts` struct parsed from JSON params; thread it through the walker; emit nested-with-breadcrumbs or flat-with-paths based on `format`. Mac/Linux/Android `ui_tree` keep their existing behavior; they ignore unknown params. Protocol layers (TS MCP server, cloud Rust MCP server, three SDKs, fake device, playground, docs) get extended to advertise the new params.

**Tech Stack:** Rust (windows crate, serde_json with `preserve_order`, regex), TypeScript (zod, MCP SDK), Python (asyncio), web (Next.js).

**Reference:** Spec at `screenmcp/docs/superpowers/specs/2026-05-03-ui-tree-filters-design.md`. New-command checklist at `screenmcp/docs/adding-new-command.md`.

---

### Task 1: Extract ui_tree out of commands.rs into its own module

**Files:**
- Create: `screenmcp/windows/src/ui_tree.rs`
- Modify: `screenmcp/windows/src/main.rs`
- Modify: `screenmcp/windows/src/commands.rs:1555-1897`

This task is a pure code move with zero behavior change. We extract `handle_ui_tree`, `handle_ui_tree_raw`, `walk_element`, `is_fully_enclosed`, and `control_type_name` into `ui_tree.rs` so the rest of the work stays in a focused file.

- [ ] **Step 1: Create the new module file**

Create `screenmcp/windows/src/ui_tree.rs` with the full content moved from `commands.rs:1555-1883`. The file starts:

```rust
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
    // ... (entire body of the existing handle_ui_tree_raw, unchanged)
}

#[cfg(windows)]
fn is_fully_enclosed(inner: &[i32; 4], outer: &[i32; 4]) -> bool {
    // ... (existing body)
}

#[cfg(windows)]
fn walk_element(
    el: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    depth: u32,
    max_depth: u32,
    sibling_rects: &mut Vec<[i32; 4]>,
    viewport: &[i32; 4],
) -> Option<Value> {
    // ... (existing body)
}

#[cfg(windows)]
#[allow(non_upper_case_globals)]
fn control_type_name(id: windows::Win32::UI::Accessibility::UIA_CONTROLTYPE_ID) -> &'static str {
    // ... (existing body)
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
```

Copy the actual function bodies verbatim from `commands.rs:1555-1897`. Do not modify behavior.

- [ ] **Step 2: Make `get_output_scale` and `scale_bounds_in_value` `pub(crate)` in commands.rs**

In `screenmcp/windows/src/commands.rs`, find the definitions of `get_output_scale` and `scale_bounds_in_value` and change `fn` to `pub(crate) fn`.

- [ ] **Step 3: Remove the moved code from commands.rs**

In `screenmcp/windows/src/commands.rs`, delete lines 1555-1897 (the four functions just moved). The dispatch line `"ui_tree" => handle_ui_tree(params, config)` stays.

- [ ] **Step 4: Update the dispatch to call the new module**

In `screenmcp/windows/src/commands.rs`, change the dispatch line:

```rust
        "ui_tree" => handle_ui_tree(params, config),
```

to:

```rust
        "ui_tree" => crate::ui_tree::handle_ui_tree(params, config),
```

- [ ] **Step 5: Register the module**

In `screenmcp/windows/src/main.rs`, add to the module declarations near the top:

```rust
mod ui_tree;
```

- [ ] **Step 6: Build and verify zero behavior change**

Run from project root:

```bash
cd screenmcp/windows && cargo build 2>&1 | tail -20
```

Expected: clean build, no new warnings beyond what was there before. Smoke-test by running the binary and calling `ui_tree` (via the local mode HTTP endpoint or worker); the output JSON shape should be unchanged.

- [ ] **Step 7: Commit**

```bash
git add screenmcp/windows/src/ui_tree.rs screenmcp/windows/src/main.rs screenmcp/windows/src/commands.rs
git commit -m "refactor(windows): extract ui_tree handler into its own module"
```

---

### Task 2: Define UiTreeOpts and parse_ui_tree_opts (TDD)

**Files:**
- Modify: `screenmcp/windows/src/ui_tree.rs`

- [ ] **Step 1: Add UiTreeOpts struct and stub parser**

In `screenmcp/windows/src/ui_tree.rs`, after the imports, add:

```rust
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
```

- [ ] **Step 2: Write unit tests for parse_ui_tree_opts**

Append to `screenmcp/windows/src/ui_tree.rs`:

```rust
#[cfg(test)]
mod parse_tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Result<UiTreeOpts, String> {
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
```

- [ ] **Step 3: Run the tests and verify they pass**

```bash
cd screenmcp/windows && cargo test --lib parse_tests 2>&1 | tail -40
```

Expected: 16 tests pass. (Note: Windows binary crates may need `cargo test` without `--lib` if no library target — adjust if `--lib` is rejected; use `cargo test parse_tests` instead.)

- [ ] **Step 4: Commit**

```bash
git add screenmcp/windows/src/ui_tree.rs
git commit -m "feat(windows): add UiTreeOpts and parse_ui_tree_opts with unit tests"
```

---

### Task 3: node_passes_display_filter (TDD)

**Files:**
- Modify: `screenmcp/windows/src/ui_tree.rs`

This is the per-node predicate that combines `region`, `types`, and `text_match` into one check. It returns `true` if the node should be included as a match (subject to the breadcrumb policy that's applied later in the walker).

- [ ] **Step 1: Write tests first**

Append to `ui_tree.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests and verify they fail (no implementation yet)**

```bash
cd screenmcp/windows && cargo test filter_tests 2>&1 | tail -10
```

Expected: build fails with `cannot find function 'node_passes_display_filter'`.

- [ ] **Step 3: Implement node_passes_display_filter**

Append to `ui_tree.rs` (above the test modules):

```rust
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
```

- [ ] **Step 4: Run the tests and verify they pass**

```bash
cd screenmcp/windows && cargo test filter_tests 2>&1 | tail -10
```

Expected: 8 tests pass.

- [ ] **Step 5: Commit**

```bash
git add screenmcp/windows/src/ui_tree.rs
git commit -m "feat(windows): add node_passes_display_filter with unit tests"
```

---

### Task 4: build_path helper (TDD)

**Files:**
- Modify: `screenmcp/windows/src/ui_tree.rs`

The `path` field in flat mode is built from the ancestor chain. Pure function, easy to TDD.

- [ ] **Step 1: Write tests first**

Append to `ui_tree.rs`:

```rust
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
```

- [ ] **Step 2: Run tests, verify they fail**

```bash
cd screenmcp/windows && cargo test path_tests 2>&1 | tail -10
```

Expected: build fails with `cannot find function 'build_path'`.

- [ ] **Step 3: Implement build_path**

Append to `ui_tree.rs` (above test modules):

```rust
/// Joins ancestor labels with " / " to produce the `path` field for flat-mode nodes.
/// Caller is responsible for picking the label per ancestor (text or fallback to controlType).
pub(crate) fn build_path(ancestors: &[&str]) -> String {
    ancestors.join(" / ")
}
```

- [ ] **Step 4: Run tests, verify pass**

```bash
cd screenmcp/windows && cargo test path_tests 2>&1 | tail -10
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add screenmcp/windows/src/ui_tree.rs
git commit -m "feat(windows): add build_path helper with unit tests"
```

---

### Task 5: build_node_value with fields selection (TDD)

**Files:**
- Modify: `screenmcp/windows/src/ui_tree.rs`

This builds the per-node JSON object. It honors `fields` (when set) and otherwise emits the legacy default field set. It also supports the new `cx`/`cy` and `path` fields. Pure function over a `RawNode` snapshot — easy to TDD.

- [ ] **Step 1: Add RawNode struct and emit logic**

In `ui_tree.rs`, above the test modules, add:

```rust
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
    if (wants(fields, NodeField::Text) || fields.is_empty()) && !node.text.is_empty() {
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
```

(The `(wants(...) || fields.is_empty())` guard on `text` is wrong — fields will always be non-empty since we use a default. Fix: drop the `|| fields.is_empty()` — `default_nested_fields` already includes `Text`.)

Actually, simplify: remove `|| fields.is_empty()` everywhere. `active_fields` always returns a non-empty slice.

- [ ] **Step 2: Add tests for build_node_value**

Append to `ui_tree.rs`:

```rust
#[cfg(test)]
mod build_node_tests {
    use super::*;

    fn button(text: &str, bounds: [i32; 4]) -> RawNode {
        RawNode {
            text: text.into(),
            control_type: "Button".into(),
            bounds: Some(bounds),
            clickable: true,
            enabled: true,
            ..RawNode::default()
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
        let n = RawNode { control_type: "Pane".into(), enabled: true, ..RawNode::default() };
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
            enabled: true,
            ..RawNode::default()
        };
        let mut opts = UiTreeOpts::default();
        opts.format = OutputFormat::Flat;
        let v = build_node_value(&n, &opts);
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("cx"));
        assert!(!obj.contains_key("cy"));
    }
}
```

- [ ] **Step 3: Run, expect failures**

```bash
cd screenmcp/windows && cargo test build_node_tests 2>&1 | tail -10
```

Expected: build fails until you fix `(wants(fields, NodeField::Text) || fields.is_empty())` in `build_node_value`.

- [ ] **Step 4: Fix the build_node_value text guard**

In `ui_tree.rs`, change:

```rust
    if (wants(fields, NodeField::Text) || fields.is_empty()) && !node.text.is_empty() {
```

to:

```rust
    if wants(fields, NodeField::Text) && !node.text.is_empty() {
```

- [ ] **Step 5: Run tests, verify pass**

```bash
cd screenmcp/windows && cargo test build_node_tests 2>&1 | tail -10
```

Expected: 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add screenmcp/windows/src/ui_tree.rs
git commit -m "feat(windows): add build_node_value with fields selection and unit tests"
```

---

### Task 6: Wire UiTreeOpts through the walker (nested mode with breadcrumbs)

**Files:**
- Modify: `screenmcp/windows/src/ui_tree.rs`

Now we plug the new code into the existing recursive walker. We thread `opts` through `walk_element` and apply: `max_depth` from opts (replacing the hardcoded `10`), `node_passes_display_filter` to decide pass/fail, and the breadcrumb-keep-on-descendant-match policy. We also use `build_node_value` to emit JSON.

This task involves UIA, so we test by building, running the binary, and calling `ui_tree` against a known target. Pure-function pieces are already covered by Tasks 2-5.

- [ ] **Step 1: Update handle_ui_tree_raw to take opts**

In `ui_tree.rs`, replace the existing `handle_ui_tree_raw` Windows body (the one moved in Task 1) with one that takes `opts: &UiTreeOpts` and threads it. Keep the same COM init/scope, viewport calc, and walker creation. Replace the depth constant and the per-element build with new logic:

```rust
#[cfg(windows)]
pub(crate) fn handle_ui_tree_raw(opts: &UiTreeOpts) -> Result<Value, String> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    };

    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) { unsafe { CoUninitialize(); } }
    }
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()
            .map_err(|e| format!("CoInitializeEx failed: {e}"))?;
    }
    let _com = ComGuard;

    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)
            .map_err(|e| format!("failed to create IUIAutomation: {e}"))?
    };
    let walker = unsafe {
        automation.ControlViewWalker()
            .map_err(|e| format!("ControlViewWalker failed: {e}"))?
    };
    let root = unsafe {
        automation.GetRootElement()
            .map_err(|e| format!("GetRootElement failed: {e}"))?
    };

    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    let viewport = [vx, vy, vx + vw, vy + vh];

    let mut children = Vec::new();
    let mut covered_rects: Vec<[i32; 4]> = Vec::new();

    let mut child = unsafe { walker.GetFirstChildElement(&root).ok() };
    while let Some(ref el) = child {
        // window scope filter (top-level only)
        if !top_level_passes_window_scope(opts, el) {
            child = unsafe { walker.GetNextSiblingElement(el).ok() };
            continue;
        }
        let mut ancestors: Vec<String> = Vec::new();
        if let Some(node) = walk_element(el, &walker, 1, opts, &mut covered_rects, &viewport, &mut ancestors) {
            children.push(node);
        }
        child = unsafe { walker.GetNextSiblingElement(el).ok() };
    }

    Ok(json!({ "tree": children, "os": "windows" }))
}

#[cfg(windows)]
fn top_level_passes_window_scope(
    opts: &UiTreeOpts,
    el: &windows::Win32::UI::Accessibility::IUIAutomationElement,
) -> bool {
    match &opts.window {
        None => true,
        Some(WindowSelector::TitleSubstring(needle)) => {
            let name = unsafe { el.CurrentName().ok() }.map(|s| s.to_string()).unwrap_or_default();
            name.to_lowercase().contains(needle.as_str())
        }
        Some(WindowSelector::Hwnd(target)) => {
            let h = unsafe { el.CurrentNativeWindowHandle().ok() }
                .map(|h| h.0 as u64).unwrap_or(0);
            h == *target
        }
    }
}
```

- [ ] **Step 2: Update walk_element signature and behavior**

Replace the existing `walk_element` body in `ui_tree.rs` with one that takes `opts` and `ancestors`, applies max_depth from opts, and uses the breadcrumb-keep-on-descendant-match policy:

```rust
#[cfg(windows)]
fn walk_element(
    el: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    depth: u32,
    opts: &UiTreeOpts,
    sibling_rects: &mut Vec<[i32; 4]>,
    viewport: &[i32; 4],
    ancestors: &mut Vec<String>,
) -> Option<Value> {
    use windows::Win32::UI::Accessibility::*;

    // Phase 1: cheap filters — offscreen, viewport, occlusion
    let is_offscreen = unsafe { el.CurrentIsOffscreen().ok() }
        .map(|b| b.as_bool()).unwrap_or(false);
    if is_offscreen { return None; }

    let bounds_raw = unsafe { el.CurrentBoundingRectangle().ok() };
    let has_real_bounds = bounds_raw.as_ref().map_or(false, |r| r.right > r.left && r.bottom > r.top);
    let bounds_arr = bounds_raw.as_ref().map(|r| [r.left, r.top, r.right, r.bottom]);

    if let Some(b) = bounds_arr {
        if b[2] <= viewport[0] || b[0] >= viewport[2] || b[3] <= viewport[1] || b[1] >= viewport[3] {
            return None;
        }
        if sibling_rects.iter().any(|sr| is_fully_enclosed(&b, sr)) {
            return None;
        }
    }

    // Phase 2: minimal props for filter check
    let name = unsafe { el.CurrentName().ok() }.map(|s| s.to_string()).unwrap_or_default();
    let automation_id = unsafe { el.CurrentAutomationId().ok() }.map(|s| s.to_string()).unwrap_or_default();
    let control_type_id = unsafe { el.CurrentControlType().unwrap_or_default() };
    let ct_name = control_type_name(control_type_id);

    // Recurse (max_depth from opts)
    let mut child_nodes = Vec::new();
    if depth < opts.max_depth {
        // Push self onto ancestor stack for children
        let label = if !name.is_empty() { name.clone() } else { ct_name.to_string() };
        ancestors.push(label);

        let mut child_sibling_rects: Vec<[i32; 4]> = Vec::new();
        let mut child = unsafe { walker.GetFirstChildElement(el).ok() };
        while let Some(ref c) = child {
            if let Some(node) = walk_element(c, walker, depth + 1, opts, &mut child_sibling_rects, viewport, ancestors) {
                child_nodes.push(node);
            }
            child = unsafe { walker.GetNextSiblingElement(c).ok() };
        }
        ancestors.pop();
    }

    // Noise filter (legacy): leaf with empty name AND empty automationId
    if child_nodes.is_empty() && name.is_empty() && automation_id.is_empty() {
        return None;
    }
    // Skip zero-bounds with no children
    if !has_real_bounds && child_nodes.is_empty() {
        return None;
    }

    // Display filter — pass=keep self; fail=keep only if children survive (breadcrumb)
    let self_passes = node_passes_display_filter(opts, ct_name, &name, bounds_arr.as_ref());
    if !self_passes && child_nodes.is_empty() {
        return None;
    }

    // Phase 3: full property snapshot + emit
    let raw = collect_raw_node(el, ct_name, &name, &automation_id, bounds_arr, ancestors);
    let mut node = build_node_value(&raw, opts);

    if !child_nodes.is_empty() {
        node.as_object_mut().unwrap().insert("children".into(), json!(child_nodes));
    }

    if let Some(b) = bounds_arr {
        if b[2] > b[0] && b[3] > b[1] {
            sibling_rects.push(b);
        }
    }

    Some(node)
}

#[cfg(windows)]
fn collect_raw_node(
    el: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    ct_name: &str,
    name: &str,
    automation_id: &str,
    bounds: Option<[i32; 4]>,
    ancestors: &[String],
) -> RawNode {
    use windows::core::Interface;
    use windows::Win32::UI::Accessibility::*;

    let class_name = unsafe { el.CurrentClassName().ok() }.map(|s| s.to_string()).unwrap_or_default();
    let help_text = unsafe { el.CurrentHelpText().ok() }.map(|s| s.to_string()).unwrap_or_default();
    let is_enabled = unsafe { el.CurrentIsEnabled().ok() }.map(|b| b.as_bool()).unwrap_or(true);
    let is_focusable = unsafe { el.CurrentIsKeyboardFocusable().ok() }.map(|b| b.as_bool()).unwrap_or(false);
    let has_focus = if is_focusable {
        unsafe { el.CurrentHasKeyboardFocus().ok() }.map(|b| b.as_bool()).unwrap_or(false)
    } else { false };
    let native_hwnd = unsafe { el.CurrentNativeWindowHandle().ok() }.map(|h| h.0 as u64).unwrap_or(0);

    let clickable = unsafe { el.GetCurrentPattern(UIA_InvokePatternId).is_ok() };
    let (editable, value) = unsafe {
        match el.GetCurrentPattern(UIA_ValuePatternId) {
            Ok(pat) => match pat.cast::<IUIAutomationValuePattern>() {
                Ok(vp) => (true, vp.CurrentValue().ok().map(|s| s.to_string()).unwrap_or_default()),
                Err(_) => (true, String::new()),
            },
            Err(_) => (false, String::new()),
        }
    };
    let (checkable, checked) = unsafe {
        match el.GetCurrentPattern(UIA_TogglePatternId) {
            Ok(pat) => match pat.cast::<IUIAutomationTogglePattern>() {
                Ok(tp) => (true, tp.CurrentToggleState().unwrap_or(ToggleState_Off) == ToggleState_On),
                Err(_) => (true, false),
            },
            Err(_) => (false, false),
        }
    };
    let scrollable = unsafe { el.GetCurrentPattern(UIA_ScrollPatternId).is_ok() };

    let path = if ancestors.is_empty() { String::new() } else {
        let labels: Vec<&str> = ancestors.iter().map(|s| s.as_str()).collect();
        build_path(&labels)
    };

    RawNode {
        text: name.to_string(),
        value,
        control_type: ct_name.to_string(),
        class_name,
        resource_id: automation_id.to_string(),
        content_description: help_text,
        bounds,
        enabled: is_enabled,
        clickable,
        editable,
        scrollable,
        checkable,
        checked,
        focusable: is_focusable,
        focused: has_focus,
        hwnd: native_hwnd,
        path,
    }
}
```

- [ ] **Step 3: Update handle_ui_tree to parse opts and apply scaling to cx/cy too**

Replace the existing `handle_ui_tree` in `ui_tree.rs` with:

```rust
pub fn handle_ui_tree(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let opts = parse_ui_tree_opts(params)?;
    let result = handle_ui_tree_raw(&opts)?;
    let (sx, sy) = get_output_scale(params, config)?;
    if sx == 1.0 && sy == 1.0 {
        return Ok(result);
    }
    Ok(scale_coords_in_value(&result, sx, sy))
}

/// Like the legacy `scale_bounds_in_value`, but also scales `cx` and `cy` if present.
fn scale_coords_in_value(v: &Value, sx: f64, sy: f64) -> Value {
    // Reuse the existing scale_bounds_in_value to handle bounds, then walk and
    // also rewrite cx/cy.
    let mut scaled = scale_bounds_in_value(v, sx, sy);
    scale_cx_cy_in_place(&mut scaled, sx, sy);
    scaled
}

fn scale_cx_cy_in_place(v: &mut Value, sx: f64, sy: f64) {
    match v {
        Value::Object(map) => {
            if let Some(cx) = map.get_mut("cx") {
                if let Some(n) = cx.as_i64() { *cx = json!(((n as f64) * sx).round() as i64); }
            }
            if let Some(cy) = map.get_mut("cy") {
                if let Some(n) = cy.as_i64() { *cy = json!(((n as f64) * sy).round() as i64); }
            }
            for (_, child) in map.iter_mut() {
                scale_cx_cy_in_place(child, sx, sy);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                scale_cx_cy_in_place(item, sx, sy);
            }
        }
        _ => {}
    }
}
```

- [ ] **Step 4: Build and verify**

```bash
cd screenmcp/windows && cargo build 2>&1 | tail -20
```

Expected: clean build. If `Interface` import is missing, add `use windows::core::Interface;` at the top of `collect_raw_node`.

- [ ] **Step 5: Run all unit tests still pass**

```bash
cd screenmcp/windows && cargo test 2>&1 | tail -10
```

Expected: parse_tests, filter_tests, path_tests, build_node_tests all pass (32 tests).

- [ ] **Step 6: Smoke-test against a real target**

Start the Windows client (or local-mode build) and call `ui_tree` with no params. Verify the JSON shape is unchanged from before this task. Sample call via PowerShell with local mode running:

```powershell
$body = '{"cmd":"ui_tree","params":{}}'
Invoke-RestMethod -Uri "http://127.0.0.1:6767/command" -Method POST `
  -Headers @{ "Authorization" = "Bearer $env:LOCAL_KEY" } -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 20
```

Expected: same `{tree: [...], os: "windows"}` shape as before this task. No `cx`/`cy`/`path` fields visible (default nested fields exclude them).

Then test with filters:

```powershell
$body = '{"cmd":"ui_tree","params":{"types":["button"],"window":"Notepad"}}'
```

Expected: only Notepad's subtree, with non-Button intermediate nodes appearing as breadcrumbs only when they lead to a Button.

- [ ] **Step 7: Commit**

```bash
git add screenmcp/windows/src/ui_tree.rs
git commit -m "feat(windows): wire UiTreeOpts through walker (window scope, filters, breadcrumbs, fields)"
```

---

### Task 7: Implement flat-mode walker

**Files:**
- Modify: `screenmcp/windows/src/ui_tree.rs`

Flat mode replaces the nested walker with a flatten walker that returns `{nodes: [...]}` and skips the breadcrumb policy (only matching nodes are emitted, with `path` carrying context).

- [ ] **Step 1: Add flatten_walk and dispatch in handle_ui_tree_raw**

In `ui_tree.rs`, modify `handle_ui_tree_raw` to dispatch on format:

```rust
#[cfg(windows)]
pub(crate) fn handle_ui_tree_raw(opts: &UiTreeOpts) -> Result<Value, String> {
    // ... (COM init, automation, walker, root, viewport — unchanged) ...

    match opts.format {
        OutputFormat::Nested => {
            let mut children = Vec::new();
            let mut covered_rects: Vec<[i32; 4]> = Vec::new();
            let mut child = unsafe { walker.GetFirstChildElement(&root).ok() };
            while let Some(ref el) = child {
                if !top_level_passes_window_scope(opts, el) {
                    child = unsafe { walker.GetNextSiblingElement(el).ok() };
                    continue;
                }
                let mut ancestors: Vec<String> = Vec::new();
                if let Some(node) = walk_element(el, &walker, 1, opts, &mut covered_rects, &viewport, &mut ancestors) {
                    children.push(node);
                }
                child = unsafe { walker.GetNextSiblingElement(el).ok() };
            }
            Ok(json!({ "tree": children, "os": "windows" }))
        }
        OutputFormat::Flat => {
            let mut flat: Vec<Value> = Vec::new();
            let mut covered_rects: Vec<[i32; 4]> = Vec::new();
            let mut child = unsafe { walker.GetFirstChildElement(&root).ok() };
            while let Some(ref el) = child {
                if !top_level_passes_window_scope(opts, el) {
                    child = unsafe { walker.GetNextSiblingElement(el).ok() };
                    continue;
                }
                let mut ancestors: Vec<String> = Vec::new();
                flatten_walk(el, &walker, 1, opts, &mut covered_rects, &viewport, &mut ancestors, &mut flat);
                child = unsafe { walker.GetNextSiblingElement(el).ok() };
            }
            Ok(json!({ "nodes": flat, "os": "windows" }))
        }
    }
}

#[cfg(windows)]
fn flatten_walk(
    el: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    depth: u32,
    opts: &UiTreeOpts,
    sibling_rects: &mut Vec<[i32; 4]>,
    viewport: &[i32; 4],
    ancestors: &mut Vec<String>,
    out: &mut Vec<Value>,
) {
    let is_offscreen = unsafe { el.CurrentIsOffscreen().ok() }
        .map(|b| b.as_bool()).unwrap_or(false);
    if is_offscreen { return; }

    let bounds_raw = unsafe { el.CurrentBoundingRectangle().ok() };
    let bounds_arr = bounds_raw.as_ref().map(|r| [r.left, r.top, r.right, r.bottom]);
    if let Some(b) = bounds_arr {
        if b[2] <= viewport[0] || b[0] >= viewport[2] || b[3] <= viewport[1] || b[1] >= viewport[3] {
            return;
        }
        if sibling_rects.iter().any(|sr| is_fully_enclosed(&b, sr)) {
            return;
        }
    }

    let name = unsafe { el.CurrentName().ok() }.map(|s| s.to_string()).unwrap_or_default();
    let automation_id = unsafe { el.CurrentAutomationId().ok() }.map(|s| s.to_string()).unwrap_or_default();
    let control_type_id = unsafe { el.CurrentControlType().unwrap_or_default() };
    let ct_name = control_type_name(control_type_id);

    // Emit self if it passes display filter (no breadcrumb policy in flat mode)
    let self_passes = node_passes_display_filter(opts, ct_name, &name, bounds_arr.as_ref());
    let leaf_noise = name.is_empty() && automation_id.is_empty();
    if self_passes && !leaf_noise {
        let raw = collect_raw_node(el, ct_name, &name, &automation_id, bounds_arr, ancestors);
        out.push(build_node_value(&raw, opts));
    }

    if depth < opts.max_depth {
        let label = if !name.is_empty() { name.clone() } else { ct_name.to_string() };
        ancestors.push(label);
        let mut child_sibling_rects: Vec<[i32; 4]> = Vec::new();
        let mut child = unsafe { walker.GetFirstChildElement(el).ok() };
        while let Some(ref c) = child {
            flatten_walk(c, walker, depth + 1, opts, &mut child_sibling_rects, viewport, ancestors, out);
            child = unsafe { walker.GetNextSiblingElement(c).ok() };
        }
        ancestors.pop();
    }

    if let Some(b) = bounds_arr {
        if b[2] > b[0] && b[3] > b[1] {
            sibling_rects.push(b);
        }
    }
}
```

- [ ] **Step 2: Build**

```bash
cd screenmcp/windows && cargo build 2>&1 | tail -10
```

Expected: clean build.

- [ ] **Step 3: Smoke-test flat mode**

With local mode running:

```powershell
$body = '{"cmd":"ui_tree","params":{"format":"flat","window":"Notepad","types":["button","menuitem"]}}'
Invoke-RestMethod -Uri "http://127.0.0.1:6767/command" -Method POST `
  -Headers @{ "Authorization" = "Bearer $env:LOCAL_KEY" } -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 5
```

Expected: `{nodes: [...], os: "windows"}` with each node having `controlType`, `text`, `cx`, `cy`, `hwnd`, `path` (path like `"Notepad / ..."`). No nested children.

- [ ] **Step 4: Smoke-test region inside vs intersect**

```powershell
$body = '{"cmd":"ui_tree","params":{"format":"flat","region":{"min_x":0,"min_y":0,"max_x":500,"max_y":500},"region_mode":"intersect"}}'
```

Compare counts between `inside` and `intersect` modes. Intersect should return ≥ inside.

- [ ] **Step 5: Verify no params still returns legacy nested output**

```powershell
$body = '{"cmd":"ui_tree","params":{}}'
```

Expected: `{tree: [...], os: "windows"}`, no `cx`/`cy`/`path` in any node. Compare against a snapshot taken before this work landed (if available) — should be byte-equivalent for the same desktop state.

- [ ] **Step 6: Commit**

```bash
git add screenmcp/windows/src/ui_tree.rs
git commit -m "feat(windows): add flat output mode with path breadcrumbs"
```

---

### Task 8: Update open-source MCP server (TypeScript) Zod schema

**Files:**
- Modify: `screenmcp/mcp-server/src/mcp.ts:36-44`

- [ ] **Step 1: Replace the ui_tree tool definition**

In `screenmcp/mcp-server/src/mcp.ts`, find the existing `ui_tree` tool definition (around lines 36-44). Replace it with:

```typescript
  {
    name: 'ui_tree',
    description: 'Get the accessibility tree of the current screen. Supports scoping to one window, filtering by control type / text / region, capping depth, and a flat output shape with precomputed center coordinates.',
    inputSchema: {
      device_id: deviceIdParam,
      ...scalingParams,
      window: z.union([z.string(), z.number()]).optional()
        .describe('Title substring (string) or hwnd (number). Scopes to one top-level window. Windows only.'),
      region: z.object({
        min_x: z.number().int(),
        min_y: z.number().int(),
        max_x: z.number().int(),
        max_y: z.number().int(),
      }).optional().describe('Filter to nodes whose bounds match this rect (in screenshot space). Windows only.'),
      region_mode: z.enum(['inside', 'intersect']).optional()
        .describe('"inside" (default): node bounds fully inside region. "intersect": any overlap.'),
      types: z.array(z.string()).optional()
        .describe('Whitelist of controlType values, case-insensitive (e.g. ["Button","Edit","MenuItem"]). Windows only.'),
      text_match: z.string().optional()
        .describe('Filter on text. Substring (case-insensitive) by default; regex if regex=true. Windows only.'),
      regex: z.boolean().optional()
        .describe('If true, text_match is a regex. Default false.'),
      max_depth: z.number().int().min(1).optional()
        .describe('Cap recursion depth (default 10). Windows only.'),
      format: z.enum(['nested', 'flat']).optional()
        .describe('"nested" (default): tree shape, byte-compatible with legacy output. "flat": array of {controlType,text,cx,cy,hwnd,path}.'),
      fields: z.array(z.string()).optional()
        .describe('Per-node fields to emit. Available: text, value, controlType, className, resourceId, contentDescription, bounds, cx, cy, enabled, clickable, editable, scrollable, checked, focused, hwnd, path. controlType is always included.'),
    },
    handler: async (phone: DeviceConnection, params: Record<string, unknown>) => {
      const res = await phone.sendCommand('ui_tree', params);
      return res.result;
    },
  },
```

- [ ] **Step 2: Type-check the MCP server**

```bash
cd screenmcp/mcp-server && npx tsc --noEmit 2>&1 | tail -10
```

Expected: zero errors.

- [ ] **Step 3: Build**

```bash
cd screenmcp/mcp-server && npm run build 2>&1 | tail -10
```

Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add screenmcp/mcp-server/src/mcp.ts
git commit -m "feat(mcp-server): extend ui_tree schema with window/region/types/text_match/format/fields"
```

---

### Task 9: Update cloud MCP server (Rust) tools schema

**Files:**
- Modify: `screenmcp-cloud/mcp-server/src/tools.rs`

- [ ] **Step 1: Locate the existing ui_tree ToolDef**

```bash
grep -n '"ui_tree"' screenmcp-cloud/mcp-server/src/tools.rs
```

Find the surrounding `ToolDef { name: "ui_tree", description: ..., input_schema: json!({ ... }) }`.

- [ ] **Step 2: Update the description and input_schema**

Replace the existing `ui_tree` `ToolDef` with:

```rust
ToolDef {
    name: "ui_tree",
    description: "Get the accessibility tree of the current screen. Supports scoping to one window, filtering by control type / text / region, capping depth, and a flat output shape with precomputed center coordinates.",
    input_schema: json!({
        "type": "object",
        "properties": {
            "device_id": { "type": "number", "description": "Device ID number." },
            "max_width": { "type": "number" },
            "max_height": { "type": "number" },
            "window": {
                "oneOf": [{"type": "string"}, {"type": "number"}],
                "description": "Title substring (string) or hwnd (number). Windows only."
            },
            "region": {
                "type": "object",
                "properties": {
                    "min_x": { "type": "integer" },
                    "min_y": { "type": "integer" },
                    "max_x": { "type": "integer" },
                    "max_y": { "type": "integer" }
                },
                "required": ["min_x","min_y","max_x","max_y"]
            },
            "region_mode": { "type": "string", "enum": ["inside","intersect"] },
            "types": { "type": "array", "items": {"type": "string"} },
            "text_match": { "type": "string" },
            "regex": { "type": "boolean" },
            "max_depth": { "type": "integer", "minimum": 1 },
            "format": { "type": "string", "enum": ["nested","flat"] },
            "fields": { "type": "array", "items": {"type": "string"} }
        },
        "required": ["device_id"]
    }),
},
```

- [ ] **Step 3: Build**

```bash
cd screenmcp-cloud/mcp-server && cargo build 2>&1 | tail -10
```

Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add screenmcp-cloud/mcp-server/src/tools.rs
git commit -m "feat(cloud-mcp): extend ui_tree input_schema with window/region/types/text_match/format/fields"
```

---

### Task 10: Update fake device to honor format and fields

**Files:**
- Modify: `screenmcp/fake-device/src/fake_device/commands.py:259-260`

The fake device returns a hardcoded `_UI_TREE`. We extend it so SDK tests can exercise `format="flat"` and `fields=[...]` without hitting a real Windows desktop. Filter behavior is canned (pretend it filtered).

- [ ] **Step 1: Locate the ui_tree handler**

In `screenmcp/fake-device/src/fake_device/commands.py`, find the line:

```python
    if cmd == "ui_tree":
        return {"status": "ok", "result": {"tree": _UI_TREE}}
```

- [ ] **Step 2: Replace with format-aware logic**

Replace the two lines above with:

```python
    if cmd == "ui_tree":
        p = params or {}
        fmt = p.get("format", "nested")
        if fmt == "flat":
            # Canned flat response: a few buttons with cx/cy/path.
            nodes = [
                {"controlType": "Button", "text": "Save", "cx": 100, "cy": 200, "hwnd": 1, "path": "FakeApp / File menu"},
                {"controlType": "Button", "text": "Cancel", "cx": 200, "cy": 200, "hwnd": 1, "path": "FakeApp / File menu"},
                {"controlType": "Edit", "text": "", "cx": 150, "cy": 100, "hwnd": 1, "path": "FakeApp"},
            ]
            fields = p.get("fields")
            if fields:
                always = {"controlType"}
                wanted = always | set(fields)
                nodes = [{k: v for k, v in n.items() if k in wanted} for n in nodes]
            return {"status": "ok", "result": {"nodes": nodes, "os": "fake"}}
        # Nested mode (legacy): leave existing tree shape.
        return {"status": "ok", "result": {"tree": _UI_TREE}}
```

- [ ] **Step 3: Quick smoke test**

```bash
cd screenmcp/fake-device && python -m fake_device --help 2>&1 | tail -3
```

If there's a built-in test mode, run it and call `ui_tree` with `format=flat`. Otherwise verify by importing:

```bash
python -c "from fake_device.commands import handle_command; from fake_device.config import TestModes; print(handle_command(1, 'ui_tree', {'format':'flat','fields':['text','cx']}, TestModes()))"
```

Expected: a `nodes` array with each node having only `controlType`, `text`, `cx`.

- [ ] **Step 4: Commit**

```bash
git add screenmcp/fake-device/src/fake_device/commands.py
git commit -m "feat(fake-device): respect ui_tree format and fields params"
```

---

### Task 11: Extend TypeScript SDK uiTree signature

**Files:**
- Modify: `screenmcp/sdk/typescript/src/client.ts:371-378`
- Modify: `screenmcp/sdk/typescript/src/types.ts`

Change `uiTree(maxWidth?, maxHeight?)` to take an options bag. Internal callers (`find`, `exists`, `waitFor`, `waitForGone`) call `uiTree()` with no args, which still works.

This is a **breaking change** for external callers passing `uiTree(1456, 819)` positionally. The migration is mechanical: `uiTree({ maxWidth: 1456, maxHeight: 819 })`.

- [ ] **Step 1: Add types for new options and flat result**

In `screenmcp/sdk/typescript/src/types.ts`, add:

```typescript
/** Options for ui_tree. */
export interface UiTreeOptions {
  maxWidth?: number;
  maxHeight?: number;
  window?: string | number;
  region?: { min_x: number; min_y: number; max_x: number; max_y: number };
  regionMode?: "inside" | "intersect";
  types?: string[];
  textMatch?: string;
  regex?: boolean;
  maxDepth?: number;
  format?: "nested" | "flat";
  fields?: string[];
}

/** Flat-mode node. All fields except controlType are optional (may be absent if not in `fields`). */
export interface UiTreeFlatNode {
  controlType: string;
  text?: string;
  value?: string;
  className?: string;
  resourceId?: string;
  contentDescription?: string;
  bounds?: { left: number; top: number; right: number; bottom: number; width: number; height: number };
  cx?: number;
  cy?: number;
  enabled?: boolean;
  clickable?: boolean;
  editable?: boolean;
  scrollable?: boolean;
  checked?: boolean;
  focused?: boolean;
  hwnd?: number;
  path?: string;
}

export interface UiTreeFlatResult {
  nodes: UiTreeFlatNode[];
  os?: string;
}
```

- [ ] **Step 2: Update the uiTree method**

In `screenmcp/sdk/typescript/src/client.ts`, replace the existing `uiTree` method (around lines 371-378) with:

```typescript
  /** Get the UI accessibility tree.
   *
   * Pass an options object to scope, filter, or change output shape (Windows only).
   * Default: full nested tree (legacy behavior).
   */
  async uiTree(opts?: UiTreeOptions): Promise<UiTreeResult | UiTreeFlatResult> {
    const params: Record<string, unknown> = {};
    if (opts?.maxWidth !== undefined) params.max_width = opts.maxWidth;
    if (opts?.maxHeight !== undefined) params.max_height = opts.maxHeight;
    if (opts?.window !== undefined) params.window = opts.window;
    if (opts?.region !== undefined) params.region = opts.region;
    if (opts?.regionMode !== undefined) params.region_mode = opts.regionMode;
    if (opts?.types !== undefined) params.types = opts.types;
    if (opts?.textMatch !== undefined) params.text_match = opts.textMatch;
    if (opts?.regex !== undefined) params.regex = opts.regex;
    if (opts?.maxDepth !== undefined) params.max_depth = opts.maxDepth;
    if (opts?.format !== undefined) params.format = opts.format;
    if (opts?.fields !== undefined) params.fields = opts.fields;

    const resp = await this.sendCommand("ui_tree", Object.keys(params).length > 0 ? params : undefined);
    const result = resp.result as Record<string, unknown> | undefined;
    if (opts?.format === "flat") {
      return { nodes: (result?.nodes as UiTreeFlatNode[] | undefined) ?? [], os: result?.os as string | undefined };
    }
    return { tree: (result?.tree as any[]) ?? [] };
  }
```

- [ ] **Step 3: Add the import for new types**

At the top of `screenmcp/sdk/typescript/src/client.ts`, locate the existing import line for `UiTreeResult` and add:

```typescript
// Add to existing types import:
import type { /* ...existing... */ UiTreeOptions, UiTreeFlatResult, UiTreeFlatNode } from "./types.js";
```

(Adjust to match the project's actual import style — TS or JS extension.)

- [ ] **Step 4: Type-check**

```bash
cd screenmcp/sdk/typescript && npx tsc --noEmit 2>&1 | tail -20
```

Expected: zero errors. If errors mention internal `uiTree()` callers, check that `find`/`exists`/`waitFor`/`waitForGone` still narrow the result type — they call `uiTree()` with no args so the result is `UiTreeResult` (since the union tag depends on `opts.format === "flat"`, the no-args path is `UiTreeResult`). If TS complains, add an explicit `as UiTreeResult` to those internal call sites.

- [ ] **Step 5: Build**

```bash
cd screenmcp/sdk/typescript && npm run build 2>&1 | tail -10
```

Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add screenmcp/sdk/typescript/src/client.ts screenmcp/sdk/typescript/src/types.ts
git commit -m "feat(sdk-ts): extend uiTree with options bag (window/region/types/format/fields)"
```

---

### Task 12: Extend Python SDK ui_tree signature

**Files:**
- Modify: `screenmcp/sdk/python/src/screenmcp/client.py:423-429`
- Modify: `screenmcp/sdk/python/src/screenmcp/types.py`

- [ ] **Step 1: Add type hints in types.py**

In `screenmcp/sdk/python/src/screenmcp/types.py`, append:

```python
from typing import Literal, TypedDict, NotRequired

class UiTreeRegion(TypedDict):
    min_x: int
    min_y: int
    max_x: int
    max_y: int

class UiTreeFlatNode(TypedDict, total=False):
    controlType: str
    text: str
    value: str
    className: str
    resourceId: str
    contentDescription: str
    bounds: dict
    cx: int
    cy: int
    enabled: bool
    clickable: bool
    editable: bool
    scrollable: bool
    checked: bool
    focused: bool
    hwnd: int
    path: str
```

(If the project's minimum Python doesn't support `NotRequired`/`Literal` from `typing`, use `typing_extensions`.)

- [ ] **Step 2: Update the ui_tree method**

In `screenmcp/sdk/python/src/screenmcp/client.py`, replace the existing `ui_tree` method with:

```python
    async def ui_tree(
        self,
        max_width: int = 0,
        max_height: int = 0,
        *,
        window: str | int | None = None,
        region: dict[str, int] | None = None,
        region_mode: str | None = None,        # "inside" | "intersect"
        types: list[str] | None = None,
        text_match: str | None = None,
        regex: bool | None = None,
        max_depth: int | None = None,
        format: str | None = None,             # "nested" | "flat"
        fields: list[str] | None = None,
    ) -> dict[str, Any]:
        """Get the UI accessibility tree.

        Returns a dict with ``tree`` (nested mode) or ``nodes`` (flat mode).
        Filtering and scoping params are Windows-only; other platforms ignore them.
        """
        params: dict[str, Any] = {}
        if max_width:        params["max_width"] = max_width
        if max_height:       params["max_height"] = max_height
        if window is not None: params["window"] = window
        if region is not None: params["region"] = region
        if region_mode:      params["region_mode"] = region_mode
        if types:            params["types"] = types
        if text_match is not None: params["text_match"] = text_match
        if regex is not None:    params["regex"] = regex
        if max_depth is not None: params["max_depth"] = max_depth
        if format:           params["format"] = format
        if fields:           params["fields"] = fields
        resp = await self.send_command("ui_tree", params or None)
        return resp.result
```

- [ ] **Step 3: Verify imports compile**

```bash
cd screenmcp/sdk/python && python -c "from screenmcp.client import DeviceConnection; from screenmcp.types import UiTreeFlatNode; print('ok')"
```

Expected: prints `ok`.

- [ ] **Step 4: Commit**

```bash
git add screenmcp/sdk/python/src/screenmcp/client.py screenmcp/sdk/python/src/screenmcp/types.py
git commit -m "feat(sdk-py): extend ui_tree with window/region/types/format/fields kwargs"
```

---

### Task 13: Extend Rust SDK with ui_tree_with(opts)

**Files:**
- Modify: `screenmcp/sdk/rust/src/client.rs:405-413`
- Modify: `screenmcp/sdk/rust/src/types.rs`

Keep the existing `ui_tree(&mut self)` for backward compat (calls `ui_tree_with(&UiTreeOpts::default())`); add `ui_tree_with(opts: &UiTreeOpts)` for full functionality.

- [ ] **Step 1: Add types to types.rs**

In `screenmcp/sdk/rust/src/types.rs`, append:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize)]
pub struct UiTreeOpts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<UiTreeWindowSelector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<UiTreeRegion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_mode: Option<String>, // "inside" | "intersect"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_match: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>, // "nested" | "flat"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum UiTreeWindowSelector {
    Title(String),
    Hwnd(u64),
}

#[derive(Debug, Clone, Serialize)]
pub struct UiTreeRegion {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiTreeFlatResult {
    #[serde(default)]
    pub nodes: Vec<serde_json::Value>,
    #[serde(default)]
    pub os: Option<String>,
}
```

- [ ] **Step 2: Add ui_tree_with to client.rs**

In `screenmcp/sdk/rust/src/client.rs`, replace the existing `ui_tree` block (lines 405-413) with:

```rust
    /// Get the UI accessibility tree (legacy: full nested output).
    pub async fn ui_tree(&mut self) -> Result<UiTreeResult> {
        self.ui_tree_with(&UiTreeOpts::default()).await
            .map(|either| either.into_nested().unwrap_or(UiTreeResult { tree: vec![] }))
    }

    /// Get the UI accessibility tree with full options.
    /// Returns either a nested or flat result depending on `opts.format`.
    pub async fn ui_tree_with(&mut self, opts: &UiTreeOpts) -> Result<UiTreeResultEither> {
        let params = serde_json::to_value(opts).ok();
        let send_params = match &params {
            Some(serde_json::Value::Object(m)) if !m.is_empty() => params,
            _ => None,
        };
        let resp = self.send_command("ui_tree", send_params).await?;
        let v = resp.result.unwrap_or(serde_json::Value::Null);
        Ok(if opts.format.as_deref() == Some("flat") {
            UiTreeResultEither::Flat(serde_json::from_value(v).unwrap_or_default())
        } else {
            UiTreeResultEither::Nested(serde_json::from_value(v).unwrap_or(UiTreeResult { tree: vec![] }))
        })
    }
```

- [ ] **Step 3: Add UiTreeResultEither helper to types.rs**

In `screenmcp/sdk/rust/src/types.rs`, append:

```rust
#[derive(Debug, Clone)]
pub enum UiTreeResultEither {
    Nested(UiTreeResult),
    Flat(UiTreeFlatResult),
}

impl UiTreeResultEither {
    pub fn into_nested(self) -> Option<UiTreeResult> {
        match self { Self::Nested(r) => Some(r), _ => None }
    }
    pub fn into_flat(self) -> Option<UiTreeFlatResult> {
        match self { Self::Flat(r) => Some(r), _ => None }
    }
}
```

- [ ] **Step 4: Update imports in client.rs**

Make sure the new types are imported. Add to the existing types import line:

```rust
use crate::types::{
    /* ...existing... */
    UiTreeOpts, UiTreeResultEither,
};
```

- [ ] **Step 5: Build**

```bash
cd screenmcp/sdk/rust && cargo build 2>&1 | tail -10
```

Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add screenmcp/sdk/rust/src/client.rs screenmcp/sdk/rust/src/types.rs
git commit -m "feat(sdk-rust): add ui_tree_with(opts) supporting window/region/types/format/fields"
```

---

### Task 14: Add SDK tests for new ui_tree params

**Files:**
- Modify: `screenmcp/fake-device/test_with_sdk.py`
- Modify: `screenmcp/sdk/typescript/examples/cli/test_fake_device.ts`
- Modify: `screenmcp/sdk/rust/examples/test_fake_device.rs`

- [ ] **Step 1: Locate existing ui_tree test in Python**

```bash
grep -n "ui_tree" screenmcp/fake-device/test_with_sdk.py
```

Find the existing test block (likely a try/except that calls `client.ui_tree()` and asserts shape).

- [ ] **Step 2: Add a flat-mode test in Python**

In `screenmcp/fake-device/test_with_sdk.py`, after the existing `ui_tree` test, add:

```python
        # ui_tree flat mode
        try:
            result = await dev.ui_tree(format="flat", fields=["text", "cx", "cy"])
            assert "nodes" in result
            assert isinstance(result["nodes"], list)
            assert all("controlType" in n for n in result["nodes"])  # always implicit
            results.append(("ui_tree(flat)", "PASS"))
        except Exception as e:
            results.append(("ui_tree(flat)", f"FAIL: {e}"))
```

- [ ] **Step 3: Add a flat-mode test in TypeScript**

In `screenmcp/sdk/typescript/examples/cli/test_fake_device.ts`, after the existing `uiTree` test, add:

```typescript
  // ui_tree flat mode
  try {
    const r = await dev.uiTree({ format: "flat", fields: ["text", "cx", "cy"] });
    if (!("nodes" in r)) throw new Error("expected nodes array");
    if (!Array.isArray(r.nodes)) throw new Error("nodes should be an array");
    for (const n of r.nodes) {
      if (typeof n.controlType !== "string") throw new Error("controlType missing");
    }
    results.push({ name: "uiTree(flat)", status: "PASS" });
  } catch (e) {
    results.push({ name: "uiTree(flat)", status: "FAIL", error: String(e) });
  }
```

(Adjust shape to match the existing `results` array convention in that file.)

- [ ] **Step 4: Add a flat-mode test in Rust**

In `screenmcp/sdk/rust/examples/test_fake_device.rs`, after the existing `ui_tree` test, add:

```rust
    // ui_tree flat mode
    {
        let mut opts = UiTreeOpts::default();
        opts.format = Some("flat".into());
        opts.fields = Some(vec!["text".into(), "cx".into(), "cy".into()]);
        match dev.ui_tree_with(&opts).await {
            Ok(r) => {
                let flat = r.into_flat().expect("expected flat result");
                if flat.nodes.iter().all(|n| n.get("controlType").is_some()) {
                    results.push(("ui_tree_with(flat)", "PASS"));
                } else {
                    results.push(("ui_tree_with(flat)", "FAIL: missing controlType"));
                }
            }
            Err(e) => results.push(("ui_tree_with(flat)", &format!("FAIL: {e}"))),
        }
    }
```

(Adjust shape to match existing `results` collection style.)

- [ ] **Step 5: Run tests against the fake device**

Refer to `screenmcp/docs/testing.md` for the exact test runner. Typical sequence:

```bash
# Start fake device
cd screenmcp/fake-device && python -m fake_device &
# Run Python SDK tests
cd screenmcp/fake-device && python test_with_sdk.py
# Run TS SDK tests
cd screenmcp/sdk/typescript/examples/cli && npx tsx test_fake_device.ts
# Run Rust SDK tests
cd screenmcp/sdk/rust && cargo run --example test_fake_device
```

Expected: all three SDK test runners report PASS for the new `(flat)` test cases.

- [ ] **Step 6: Commit**

```bash
git add screenmcp/fake-device/test_with_sdk.py screenmcp/sdk/typescript/examples/cli/test_fake_device.ts screenmcp/sdk/rust/examples/test_fake_device.rs
git commit -m "test(sdk): add ui_tree flat-mode tests in Python, TS, and Rust SDK runners"
```

---

### Task 15: Update documentation

**Files:**
- Modify: `screenmcp/docs/commands.md`
- Modify: `screenmcp/docs/wire-protocol.md`
- Modify: `screenmcp/docs/return-value-windows-ui-tree.md`
- Modify: `screenmcp/docs/implementations.md`

- [ ] **Step 1: Update commands.md ui_tree entry**

In `screenmcp/docs/commands.md`, find the `### ui_tree` section. Replace the parameter table with:

```markdown
| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `max_width` | integer | 0 | Scale returned bounds to match screenshot space |
| `max_height` | integer | 0 | Scale returned bounds to match screenshot space |
| `window` | string \| number | — | (Windows) Title substring or hwnd. Scopes the walk to one top-level window. |
| `region` | object | — | (Windows) `{min_x, min_y, max_x, max_y}` in screenshot space. Filter by bounds. |
| `region_mode` | string | `"inside"` | (Windows) `"inside"` (fully inside) or `"intersect"` (any overlap). |
| `types` | string[] | all | (Windows) Whitelist of `controlType` values. Case-insensitive. |
| `text_match` | string | — | (Windows) Filter on `text`. Substring (case-insensitive) by default. |
| `regex` | boolean | false | (Windows) If true, `text_match` is treated as a regex. |
| `max_depth` | integer | 10 | (Windows) Cap recursion depth. |
| `format` | string | `"nested"` | (Windows) `"nested"` (default) or `"flat"`. |
| `fields` | string[] | per-format | (Windows) Per-node fields to emit. See node fields below. |

**Returns (nested):** `{ "tree": [ ...nodes ] }`

**Returns (flat):** `{ "nodes": [ {controlType, text, cx, cy, hwnd, path}, ... ] }`

**Available `fields` values:** `text, value, controlType, className, resourceId, contentDescription, bounds, cx, cy, enabled, clickable, editable, scrollable, checked, focused, hwnd, path`. `controlType` is always emitted; `children` is always emitted in nested mode.

**Examples:**

Scope to one window, list buttons only:
```json
{"cmd":"ui_tree","params":{"window":"Notepad","types":["button"]}}
```

Flat mode, ready-to-click center coords:
```json
{"cmd":"ui_tree","params":{"format":"flat","window":"Notepad"}}
```

Region filter, intersect:
```json
{"cmd":"ui_tree","params":{"region":{"min_x":0,"min_y":0,"max_x":500,"max_y":500},"region_mode":"intersect"}}
```
```

- [ ] **Step 2: Update wire-protocol.md**

In `screenmcp/docs/wire-protocol.md`, find any existing `ui_tree` example and add a flat-mode example next to it:

```markdown
**ui_tree request (flat mode):**
```json
{"id": 1, "cmd": "ui_tree", "params": {"format": "flat", "window": "Notepad", "types": ["button"]}}
```

**ui_tree response (flat mode):**
```json
{"id": 1, "status": "ok", "result": {
  "nodes": [
    {"controlType": "Button", "text": "Save", "cx": 512, "cy": 300, "hwnd": 1234, "path": "Notepad / File"}
  ],
  "os": "windows"
}}
```
```

- [ ] **Step 3: Update return-value-windows-ui-tree.md**

In `screenmcp/docs/return-value-windows-ui-tree.md`, append a new section documenting the new fields and flat shape:

```markdown
## New optional fields

| Property | Type | Notes |
|---|---|---|
| `cx` | number | Center X coord (precomputed). Only emitted when `fields` includes `cx`. |
| `cy` | number | Center Y coord (precomputed). Only emitted when `fields` includes `cy`. |
| `path` | string | Breadcrumb of ancestor labels joined with ` / `. Only meaningful in flat mode. |

## Flat-mode response shape

When the request includes `"format": "flat"`, the response shape is:

```json
{ "nodes": [ ...nodes ], "os": "windows" }
```

Each node is the same per-node JSON object as nested mode (same field rules), but with no `children` and with `path` instead of structural ancestors. Default `fields` for flat mode: `controlType`, `text`, `cx`, `cy`, `hwnd`, `path`.
```

- [ ] **Step 4: Update implementations.md**

In `screenmcp/docs/implementations.md`, find the row for `ui_tree`. Add a footnote or column noting that the new params (`window`, `region`, `region_mode`, `types`, `text_match`, `regex`, `max_depth`, `format`, `fields`) are Windows-only; other platforms accept and ignore them.

- [ ] **Step 5: Commit**

```bash
git add screenmcp/docs/commands.md screenmcp/docs/wire-protocol.md screenmcp/docs/return-value-windows-ui-tree.md screenmcp/docs/implementations.md
git commit -m "docs(ui_tree): document new params, flat output, and field vocabulary"
```

---

### Task 16: Update cloud web playground

**Files:**
- Modify: `screenmcp-cloud/web/src/app/playground/page.tsx`

- [ ] **Step 1: Locate the existing ui_tree command UI**

```bash
grep -n "ui_tree\|uiTree" screenmcp-cloud/web/src/app/playground/page.tsx
```

Find: the `CommandType` union, the command group definition, the mock response, the `buildParams()` function, and any state vars used for ui_tree's existing `max_width`/`max_height` inputs.

- [ ] **Step 2: Add state vars for the new params**

Near the other ui_tree state vars, add:

```tsx
  const [uiTreeWindow, setUiTreeWindow] = useState("");
  const [uiTreeTypes, setUiTreeTypes] = useState("");      // comma-separated
  const [uiTreeFormat, setUiTreeFormat] = useState<"nested" | "flat">("nested");
  const [uiTreeMaxDepth, setUiTreeMaxDepth] = useState("");
  const [uiTreeFields, setUiTreeFields] = useState("");    // comma-separated
```

- [ ] **Step 3: Add inputs to the ui_tree command panel**

In the `ui_tree` command's render block, after the existing `max_width`/`max_height` inputs, add:

```tsx
        <input placeholder="window (title or hwnd)" value={uiTreeWindow}
               onChange={(e) => setUiTreeWindow(e.target.value)} />
        <input placeholder="types (comma-separated)" value={uiTreeTypes}
               onChange={(e) => setUiTreeTypes(e.target.value)} />
        <select value={uiTreeFormat} onChange={(e) => setUiTreeFormat(e.target.value as any)}>
          <option value="nested">nested</option>
          <option value="flat">flat</option>
        </select>
        <input placeholder="max_depth" type="number" value={uiTreeMaxDepth}
               onChange={(e) => setUiTreeMaxDepth(e.target.value)} />
        <input placeholder="fields (comma-separated)" value={uiTreeFields}
               onChange={(e) => setUiTreeFields(e.target.value)} />
```

(Match the existing form styling in the file.)

- [ ] **Step 4: Update buildParams() for ui_tree**

In `buildParams()`, find the `case "ui_tree":` block and extend it:

```tsx
    case "ui_tree": {
      const p: Record<string, unknown> = {};
      // ... existing max_width/max_height logic ...
      if (uiTreeWindow) {
        const num = Number(uiTreeWindow);
        p.window = !isNaN(num) && /^\d+$/.test(uiTreeWindow) ? num : uiTreeWindow;
      }
      if (uiTreeTypes) p.types = uiTreeTypes.split(",").map((s) => s.trim()).filter(Boolean);
      if (uiTreeFormat !== "nested") p.format = uiTreeFormat;
      if (uiTreeMaxDepth) p.max_depth = Number(uiTreeMaxDepth);
      if (uiTreeFields) p.fields = uiTreeFields.split(",").map((s) => s.trim()).filter(Boolean);
      return p;
    }
```

- [ ] **Step 5: Type-check**

```bash
cd screenmcp-cloud/web && npx tsc --noEmit 2>&1 | tail -10
```

Expected: zero errors.

- [ ] **Step 6: Commit**

```bash
git add screenmcp-cloud/web/src/app/playground/page.tsx
git commit -m "feat(playground): add ui_tree window/types/format/max_depth/fields inputs"
```

---

### Task 17: Verify Mac, Linux, Android clients ignore unknown params

**Files:**
- Read-only: `screenmcp/mac/src/commands.rs`
- Read-only: `screenmcp/linux/src/commands.rs`
- Read-only: `screenmcp/android/app/src/main/java/.../WebSocketClient.kt`

This task is a pure verification — no code changes expected. The new params should flow through and be silently dropped by the existing implementations.

- [ ] **Step 1: Inspect mac handle_ui_tree**

```bash
grep -nA 20 "handle_ui_tree\b" screenmcp/mac/src/commands.rs
```

Confirm that the function takes `params: Option<&Value>` and reads only `max_width`/`max_height` via `get_output_scale`. Unknown keys in `params` are ignored. No code change.

- [ ] **Step 2: Inspect linux handle_ui_tree**

```bash
grep -nA 20 "handle_ui_tree\b" screenmcp/linux/src/commands.rs
```

Same expectation: ignores unknown keys.

- [ ] **Step 3: Inspect Android dispatch**

Look at `WebSocketClient.kt` for the `"ui_tree"` case. Confirm that it extracts only the params it cares about (`max_width`/`max_height`) and ignores extras.

- [ ] **Step 4: Optional smoke-test**

If a Mac or Linux dev environment is available, send `{"cmd":"ui_tree","params":{"format":"flat","types":["button"]}}` and verify the device returns its existing nested shape (params silently ignored, no error).

- [ ] **Step 5: Note finding**

If any client crashes or errors on unknown params, file a follow-up task — do NOT change behavior in this PR. The spec promised silent ignore.

(No commit required if no code change.)

---

## Self-Review Notes

Spec coverage check (each spec section → which task implements it):

- "New parameters" table → Tasks 2 (parsing) + 6/7 (walker uses them)
- "fields vocabulary" → Tasks 2 (parsing) + 5 (build_node_value)
- "Always-implicit fields" → Task 5 (controlType always emitted) + Task 6 (children always emitted in nested)
- "Default fields" (nested matches legacy, flat = `[controlType, text, cx, cy, hwnd, path]`) → Task 5 (`default_nested_fields`, `default_flat_fields`)
- Filter pipeline (window → max_depth → region → types → text_match) → Task 6 (window scope at top-level), Task 6 (max_depth in walker), Task 3 (display filter combines region/types/text_match)
- Breadcrumb policy (nested mode) → Task 6 (`if !self_passes && child_nodes.is_empty() { return None; }`)
- Flat mode shape `{nodes: [...]}` → Task 7
- Coordinate scaling for cx/cy → Task 6 (`scale_cx_cy_in_place`)
- Backward compatibility → Tasks 1 (refactor preserves behavior), 5 (default nested fields = legacy), 6 (no params → identical output, smoke-tested)
- Error handling cases → Task 2 (every error path tested)
- Component changes 1-15 from spec → Tasks 1-17

Placeholder scan: zero `TBD`/`TODO`/"appropriate handling" patterns.

Type consistency: `UiTreeOpts` defined in Task 2; used in Tasks 3, 5, 6, 7. `RawNode` in Task 5; used in Task 6. `NodeField` enum in Task 2; used in Tasks 5 and elsewhere via `default_*_fields`. SDK `UiTreeOptions` (TS), `UiTreeOpts` (Rust SDK), and Python kwargs all map to the same wire-level params. Names verified consistent.
