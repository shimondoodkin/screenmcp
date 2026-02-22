# Selectors & Steps

Find UI elements by query instead of raw coordinates. Chain multi-step automations as reusable YAML steps.

## Quick Start

### TypeScript SDK

```typescript
import { ScreenMCPClient, StepsRunner } from "@screenmcp/sdk";

const client = new ScreenMCPClient({ apiKey: "pk_..." });
await client.connect();

// Find and click a button
await client.find("text:Save").click();

// Type into a focused input
await client.find("role:EditText[focused]").type("hello world");

// Wait for an element to appear
await client.waitFor("text:Success", { timeout: 5000 });

// Run a steps file
const runner = new StepsRunner(client, { verbose: true });
await runner.runFile("steps/send_message.yml", { contact: "Mom", message: "Hi" });

await client.disconnect();
```

### Python SDK

```python
from screenmcp import ScreenMCPClient, StepsRunner

async with ScreenMCPClient(api_key="pk_...") as client:
    # Find and click
    await client.find("text:Save").click()

    # Type into input
    await client.find("role:EditText[focused]").type("hello")

    # Check if element exists
    if await client.exists("text:Login", timeout=2.0):
        await client.find("text:Login").click()
```

### CLI

```bash
# Find elements by selector
screenmcp find "role:Button" --api-key pk_...

# Click an element by selector (interactive shell)
screenmcp shell --api-key pk_...
screenmcp> find text:Settings
screenmcp> click_on text:Settings

# Run a steps
screenmcp run send_message.yml --input '{"contact":"Mom","message":"Hi"}' --verbose --api-key pk_...
```

---

## Selector Syntax

Selectors query the accessibility tree returned by `ui_tree`. They match element properties and return coordinates for interaction.

### Basic Selectors

| Selector | Matches | Node Field |
|----------|---------|------------|
| `text:X` | Text contains X (case-insensitive) | `text` |
| `text=X` | Text exactly equals X | `text` |
| `role:X` | Class name contains X (case-insensitive) | `className` (or `title` on desktop) |
| `desc:X` | Content description contains X | `contentDescription` |
| `id:X` | Resource ID contains X | `resourceId` |

```typescript
await client.find("text:Save").click();          // Button with text "Save"
await client.find("text=OK").click();             // Exact match "OK"
await client.find("role:EditText").click();       // First EditText input
await client.find("desc:Send button").click();    // By accessibility description
await client.find("id:com.app/submit").click();   // By resource ID
```

### Boolean Operators

Combine conditions with `&&` (AND) and `||` (OR). Negate with `!`.

```typescript
// AND — must match both
await client.find("role:Button && text:Save").click();

// OR — match either
await client.find("text:OK || text:Cancel").click();

// NOT — exclude
await client.find("role:EditText && !text:Search").click();
```

### Property Filters

Filter by boolean properties. Append `[property]` to any selector.

```typescript
await client.find("role:EditText[focused]").type("hello");    // Only focused inputs
await client.find("role:Button[clickable]").click();           // Only clickable buttons
await client.find("role:CheckBox[checked]").click();           // Only checked checkboxes
```

Available properties: `focused`, `clickable`, `editable`, `scrollable`, `checkable`, `checked`.

### Index Selection

Select the Nth match with `[index]`. Supports negative indices.

```typescript
await client.find("role:Button[0]").click();     // First button
await client.find("role:Button[-1]").click();    // Last button
await client.find("role:Button[2]").click();     // Third button
```

### Combined Examples

```typescript
// First clickable button with text containing "Submit"
await client.find("role:Button && text:Submit[clickable][0]").click();

// Any EditText that is focused, or any element with text "Search"
await client.find("role:EditText[focused] || text:Search").click();

// Button without the text "Cancel"
await client.find("role:Button && !text:Cancel").click();
```

---

## SDK Methods

### `find(selector, options?)` → `ElementHandle`

Returns a fluent handle. Does NOT search immediately — search happens when you call `.click()`, `.type()`, etc.

```typescript
const handle = client.find("text:Save", { timeout: 5000 });
await handle.click();        // Find element, click its center
await handle.longClick();    // Find element, long-press its center
await handle.type("hello");  // Find element, click to focus, then type
const el = await handle.element();  // Find element, return FoundElement
```

Default timeout: 3000ms. The handle polls `ui_tree` every 500ms until the element appears or the timeout expires.

### `findAll(selector, options?)` → `Promise<FoundElement[]>`

Find all matching elements. Returns empty array if none found after timeout.

```typescript
const buttons = await client.findAll("role:Button", { timeout: 3000 });
for (const btn of buttons) {
  console.log(`${btn.text} at (${btn.x}, ${btn.y})`);
}
```

### `exists(selector, options?)` → `Promise<boolean>`

Check if an element exists. Default timeout: 0 (instant check, no polling).

```typescript
if (await client.exists("text:Login")) {
  await client.find("text:Login").click();
}

// With timeout — wait up to 2s for element to appear
const appeared = await client.exists("text:Welcome", { timeout: 2000 });
```

### `waitFor(selector, options?)` → `Promise<FoundElement>`

Wait for an element to appear. Throws on timeout.

```typescript
const el = await client.waitFor("text:Success", { timeout: 10000 });
console.log(`Found at (${el.x}, ${el.y})`);
```

### `waitForGone(selector, options?)` → `Promise<void>`

Wait for an element to disappear. Throws if still present after timeout.

```typescript
await client.waitForGone("text:Loading...", { timeout: 10000 });
```

### FoundElement Shape

```typescript
interface FoundElement {
  x: number;           // Center X coordinate
  y: number;           // Center Y coordinate
  bounds: {
    left: number;
    top: number;
    right: number;
    bottom: number;
  };
  text?: string;
  className?: string;
  resourceId?: string;
  contentDescription?: string;
  node: Record<string, unknown>;  // Raw tree node
}
```

---

## Steps

Steps are YAML (or JSON) files that define multi-step automations. The steps runner executes steps sequentially, using selectors for element finding and validation.

### Example Steps File

```yaml
name: send_whatsapp_message
description: Open WhatsApp, find a contact, send a message

input:
  contact:
    type: string
    description: Contact name to message
  message:
    type: string
    description: Message text to send

steps:
  - id: go_home
    action: home

  - id: wait_home
    wait: 500

  - id: open_app
    action: click
    selector: "text:WhatsApp"
    timeout: 3000

  - id: wait_load
    wait: 1500

  - id: find_contact
    action: click
    selector: "text:{{contact}}"
    timeout: 5000

  - id: type_message
    action: type
    selector: "role:EditText[focused]"
    params:
      text: "{{message}}"

  - id: send
    action: click
    selector: "desc:Send"

  - id: verify
    action: expect
    selector: "text:{{message}}"
    timeout: 3000
```

### Running Steps

**TypeScript:**
```typescript
import { ScreenMCPClient, StepsRunner } from "@screenmcp/sdk";

const client = new ScreenMCPClient({ apiKey: "pk_..." });
await client.connect();

const runner = new StepsRunner(client, { verbose: true });

// From a file
const result = await runner.runFile("steps/send_message.yml", {
  contact: "Mom",
  message: "Be home in 10 min",
});

// From a parsed object
const result2 = await runner.run(stepsDefinition, { contact: "Mom" });

// From a YAML string (requires js-yaml)
const result3 = await runner.runYaml(yamlString, { contact: "Mom" });

console.log(result.status);          // "ok" or "error"
console.log(result.steps_completed); // 7
console.log(result.duration_ms);     // 4230
```

**CLI:**
```bash
# Run with input variables
screenmcp run send_message.yml --input '{"contact":"Mom","message":"Hello"}' --api-key pk_...

# Verbose output (prints each step)
screenmcp run send_message.yml -v --input '{"contact":"Mom","message":"Hello"}' --api-key pk_...

# Dry run (parse and validate, don't execute)
screenmcp run send_message.yml --dry-run

# In interactive shell
screenmcp> run send_message.yml --input '{"contact":"Mom","message":"Hello"}'
```

### Step Reference

Every step requires an `id`. All other fields are optional depending on the action.

#### action

The command to execute. All device commands are supported:

| Action | Description | Requires |
|--------|-------------|----------|
| `click` | Tap on element or coordinates | `selector` or `params.x`/`params.y` |
| `long_click` | Long press | `selector` or `params.x`/`params.y` |
| `type` | Type text (clicks element first if selector given) | `params.text` |
| `scroll` | Scroll the screen | `params.direction` or step `direction` |
| `scroll_until` | Scroll repeatedly until selector found | `selector`, `direction` |
| `drag` | Drag gesture | `params.startX/Y/endX/Y` |
| `screenshot` | Take screenshot | — |
| `back` | Press Back | — |
| `home` | Press Home | — |
| `recents` | Open app switcher | — |
| `select_all` | Select all text | — |
| `copy` | Copy selection | optional `params.return_text` |
| `paste` | Paste (optionally set clipboard first) | optional `params.text` |
| `get_text` | Get focused element text | — |
| `get_clipboard` | Get clipboard contents | — |
| `set_clipboard` | Set clipboard | `params.text` |
| `expect` | Verify element exists (no interaction) | `selector` |
| `hold_key` | Hold key (desktop) | `params.key` |
| `release_key` | Release key (desktop) | `params.key` |
| `press_key` | Press key (desktop) | `params.key` |
| `camera` | Take photo | optional `params.camera` |
| `list_cameras` | List cameras | — |
| `ui_tree` | Get accessibility tree | — |

Any unknown action is passed through to `sendCommand(action, params)`.

#### selector

Find an element by selector query. The runner calls `ui_tree`, searches for the matching element, and uses its center coordinates for the action.

```yaml
- id: click_save
  action: click
  selector: "text:Save"
```

#### params

Additional parameters for the command:

```yaml
- id: type_name
  action: type
  params:
    text: "John Doe"

- id: drag_item
  action: drag
  params:
    startX: 200
    startY: 800
    endX: 200
    endY: 400
```

#### timeout

How long to wait for a selector to match (ms). Default: 3000.

```yaml
- id: wait_for_app
  action: click
  selector: "text:MyApp"
  timeout: 10000    # Wait up to 10 seconds
```

#### wait

Delay before executing the step (ms). If `wait` is the only field (no `action`), the step acts as a pure sleep.

```yaml
# Sleep step
- id: pause
  wait: 1000

# Delay before action
- id: click_after_wait
  action: click
  selector: "text:Next"
  wait: 500
```

#### expect

Verify an element exists after the action executes. If not found, the step fails.

```yaml
- id: click_submit
  action: click
  selector: "text:Submit"
  expect: "text:Success"
  expect_timeout: 5000
```

#### condition

Skip the step if the selector does NOT match. Useful for optional steps.

```yaml
- id: dismiss_popup
  action: click
  selector: "text:Dismiss"
  condition: "text:Dismiss"    # Only run if Dismiss button exists
  on_missing: skip
```

#### on_missing

What to do if the selector is not found after timeout. Default: `"error"`.

| Value | Behavior |
|-------|----------|
| `error` | Throw an error (default) |
| `skip` | Skip this step silently |
| `goto:step_id` | Jump to a different step |

```yaml
- id: check_login
  action: expect
  selector: "text:Dashboard"
  timeout: 2000
  on_missing: "goto:login_flow"

- id: main_flow
  action: click
  selector: "text:Settings"

- id: login_flow
  action: click
  selector: "text:Sign In"
```

#### save_as

Save the action's return value to a named variable. Access it in later steps with `{{variable_name}}` or `{{variable_name.field}}`.

```yaml
- id: get_field
  action: get_text
  save_as: field_text

- id: log_it
  action: type
  params:
    text: "Previous text: {{field_text.text}}"
```

#### retries / retry_delay

Retry a failed step. Delay between retries in ms (default: 1000).

```yaml
- id: flaky_click
  action: click
  selector: "text:Submit"
  retries: 3
  retry_delay: 2000    # 2 seconds between retries
```

#### scroll_until

A composite action that scrolls repeatedly until a selector matches.

```yaml
- id: find_item
  action: scroll_until
  selector: "text:Item #42"
  direction: down
  max_scrolls: 10
  scroll_amount: 500
```

### Template Variables

Use `{{name}}` in any string value. Variables resolve from steps `input` first, then from `save_as` variables.

Dot notation accesses nested fields:

```yaml
input:
  user:
    type: string

steps:
  - id: get_info
    action: get_text
    save_as: current

  - id: use_it
    action: type
    params:
      text: "Hello {{user}}, your text was: {{current.text}}"
```

### Steps Result

```typescript
interface StepsResult {
  status: "ok" | "error";
  steps_completed: number;
  steps_total: number;
  duration_ms: number;
  last_step?: string;       // ID of the last executed step
  error?: string;           // Error message (if status is "error")
  variables: Record<string, unknown>;  // All saved variables
}
```

---

## CLI Commands

### `find <selector>`

Search the device's UI tree for elements matching a selector.

```bash
screenmcp find "role:Button" --api-key pk_...
screenmcp find "text:Settings" --timeout 5000 --api-key pk_...
```

Output:
```
  (540, 1200)  android.widget.Button
    text: Settings
    role: android.widget.Button
    id: com.app:id/settings_btn
    bounds: [400, 1150, 680, 1250]
Found 1 element(s)
```

### `run <steps-file>`

Execute a steps file.

```bash
screenmcp run steps.yml --api-key pk_...
screenmcp run steps.yml --input '{"name":"John"}' --api-key pk_...
screenmcp run steps.yml --verbose --api-key pk_...
screenmcp run steps.yml --dry-run
```

| Flag | Description |
|------|-------------|
| `-i, --input <json>` | Input variables as JSON string |
| `-v, --verbose` | Print each step as it executes |
| `--dry-run` | Parse and validate without executing |

### Shell Commands

In the interactive shell (`screenmcp shell`):

| Command | Description |
|---------|-------------|
| `find <selector>` | Search UI tree, print matches |
| `click_on <selector>` | Find element and click it |
| `run <file> [--input '{...}']` | Execute a steps file |

---

## How It Works

The selector engine and steps runner are **purely client-side**. They use existing device commands (`ui_tree`, `click`, `type`, etc.) without any changes to the wire protocol, worker, or device clients.

```
Selector: client.find("text:Save").click()

  1. SDK calls ui_tree command → device returns accessibility tree
  2. Selector engine parses "text:Save" into a query
  3. DFS walks the tree, finds matching node
  4. Computes center coordinates from element bounds
  5. SDK calls click(centerX, centerY)

Steps: runner.runFile("steps.yml", input)

  1. Parse YAML into step definitions
  2. For each step:
     a. Substitute {{template}} variables
     b. If selector: call ui_tree → search → get coordinates
     c. Execute action via SDK method
     d. If expect: call ui_tree → verify element exists
     e. Save result if save_as is set
  3. Return StepsResult with status, timing, variables
```
