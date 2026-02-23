# Windows ui_tree Response Structure

The `ui_tree` command returns a JSON object with a single key `tree` containing an array of top-level window nodes.

```json
{
  "tree": [ ...nodes ]
}
```

## Node Properties (sparse — only non-default/non-empty values included)

| Property | Type | Notes |
|---|---|---|
| `text` | string | Element name (omitted if empty) |
| `value` | string | Current value for editable elements (omitted if empty) |
| `controlType` | string | Always present. One of: Window, Pane, Button, Edit, Text, CheckBox, ComboBox, List, ListItem, Menu, MenuBar, MenuItem, Tab, TabItem, Tree, TreeItem, Hyperlink, Image, ProgressBar, RadioButton, ScrollBar, Slider, Spinner, StatusBar, ToolBar, ToolTip, Group, Thumb, DataGrid, DataItem, Document, SplitButton, Header, HeaderItem, Table, TitleBar, Separator, SemanticZoom, AppBar, Custom, Unknown |
| `className` | string | Win32 class name (omitted if empty) |
| `resourceId` | string | UIA AutomationId (omitted if empty) |
| `contentDescription` | string | Help text (omitted if empty) |
| `bounds` | `[[x, y], [w, h]]` | Always present. Position and size in screen pixels |
| `enabled` | bool | Only present when `false` (default is true) |
| `clickable` | bool | Only present when `true` (has InvokePattern) |
| `editable` | bool | Only present when `true` (has ValuePattern) |
| `scrollable` | bool | Only present when `true` (has ScrollPattern) |
| `checked` | bool | Only present when element has TogglePattern (is checkable) |
| `focused` | bool | Only present when element is focusable |
| `hwnd` | number | Native window handle (omitted if 0) |
| `children` | array | Array of child nodes (omitted if empty) |

## Key behaviors

- **Sparse JSON**: Only non-default values are emitted. A leaf button might just be `{"text": "OK", "controlType": "Button", "bounds": [[100, 200], [80, 30]], "clickable": true}`.
- **Occlusion culling**: Windows occluded by windows in front are skipped.
- **Offscreen filtering**: Elements flagged as offscreen by UIA are excluded.
- **Max depth**: 10 levels.
- **Property order**: text, value, controlType, className, resourceId, contentDescription, bounds, state flags, hwnd, children (uses serde_json `preserve_order` feature).
