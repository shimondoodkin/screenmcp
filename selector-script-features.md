# Selectors & Steps — Feature Design

Two layers of abstraction on top of ScreenMCP's existing command protocol. Zero changes to the wire protocol, worker, or device clients — everything runs client-side in the SDKs.

---

## Layer 1: Selectors

### Problem

Users pass raw `x, y` coordinates to click/drag/scroll. But `ui_tree` already returns the full accessibility tree with element bounds, roles, text, and properties. There's no way to say "click the Save button" — you have to manually inspect the tree, find the element, compute center coordinates, and pass them.

### Solution

A selector engine that runs in the SDK (TypeScript and Python). It calls `ui_tree`, searches the tree with a query, extracts bounds, and computes center coordinates.

```typescript
// Before — manual coordinates
await client.click(540, 1847);

// After — selector-based
await client.find('text:Save').click();
```

### Selector Syntax

Simple, flat query language. No nesting needed — ScreenMCP trees are shallower than desktop accessibility trees.

**Basic selectors:**

| Selector | Matches | Example |
|----------|---------|---------|
| `text:X` | Element with displayed text containing X | `text:Save` |
| `text=X` | Element with text exactly equal to X | `text=OK` |
| `role:X` | Element with accessibility role/className | `role:EditText` |
| `desc:X` | Element with contentDescription containing X | `desc:Send button` |
| `id:X` | Element with resourceId containing X | `id:com.app/submit` |
| `pos:X,Y` | Exact coordinates (passthrough, no tree search) | `pos:540,960` |

**Boolean operators:**

```
role:Button && text:Save        # AND — must match both
text:OK || text:Cancel          # OR — match either
role:EditText && !text:Search   # NOT — exclude
```

**Index selection:**

```
role:Button[0]      # First button
role:Button[-1]     # Last button
role:Button[2]      # Third button
```

**Property filters:**

```
role:EditText[focused]          # Only focused edit fields
role:Button[clickable]          # Only clickable buttons
role:CheckBox[checked]          # Only checked checkboxes
```

### Selector API (TypeScript SDK)

```typescript
// Find element and get its bounds
const element = await client.find('text:Save');
// Returns: { x, y, bounds: { left, top, right, bottom }, text, role, ... }

// Find and interact
await client.find('text:Save').click();
await client.find('role:EditText[focused]').type('hello');
await client.find('text:Submit').longClick();

// Find with timeout (poll ui_tree every 500ms until found or timeout)
await client.find('text:Success', { timeout: 5000 }).click();

// Find all matching elements
const buttons = await client.findAll('role:Button');

// Check if element exists (no error if not found)
const exists = await client.exists('text:Login', { timeout: 2000 });

// Wait for element to appear
await client.waitFor('text:Welcome', { timeout: 10000 });

// Wait for element to disappear
await client.waitForGone('text:Loading...', { timeout: 10000 });
```

### Selector API (Python SDK)

```python
# Find and interact
await client.find('text:Save').click()
await client.find('role:EditText[focused]').type('hello')

# With timeout
await client.find('text:Success', timeout=5000).click()

# Check existence
exists = await client.exists('text:Login', timeout=2000)

# Wait
await client.wait_for('text:Welcome', timeout=10000)
await client.wait_for_gone('text:Loading...', timeout=10000)
```

### How It Works (internals)

```
client.find('role:Button && text:Save').click()

  1. Call ui_tree command via WebSocket
  2. Parse selector string into query
  3. Walk the tree recursively, match each node against query
  4. If not found and timeout > 0: sleep 500ms, call ui_tree again, repeat
  5. If found: compute center = ((left+right)/2, (top+bottom)/2)
  6. Call click(centerX, centerY) via WebSocket
  7. Return result
```

The selector engine is a pure function: `matchNode(node, query) → boolean`. Tree walking is recursive DFS. No new wire protocol messages needed.

---

## Layer 2: Steps

### Problem

Useful automations require chaining multiple commands in sequence — open app, find element, click, type text, verify result. Today users write custom scripts for every task. There's no reusable format.

### Solution

Steps are YAML files that define multi-step automations. The steps runner executes them sequentially, using selectors for element finding and validation.

### Steps Format

```yaml
name: send_whatsapp_message
description: Open WhatsApp, find a contact, and send a message

input:
  contact:
    type: string
    description: Contact name to message
  message:
    type: string
    description: Message text to send

steps:
  - id: open_whatsapp
    description: Open WhatsApp from home screen
    action: home

  - id: wait_home
    wait: 500

  - id: launch
    action: click
    selector: "text:WhatsApp"
    timeout: 3000

  - id: wait_app
    wait: 1500

  - id: find_contact
    action: click
    selector: "text:{{contact}}"
    timeout: 5000

  - id: type_message
    action: type
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

### Step Fields

| Field | Required | Description |
|-------|----------|-------------|
| `id` | yes | Unique step identifier |
| `description` | no | Human-readable description |
| `action` | yes* | Command to execute: `click`, `type`, `screenshot`, `scroll`, etc. |
| `selector` | no | Selector query to find the element. Replaces `x,y` in params. |
| `params` | no | Command parameters (for commands that need more than just coordinates) |
| `timeout` | no | How long to wait for selector to match (ms). Default: 3000 |
| `wait` | no* | Delay in ms before this step. If only field present, acts as a sleep step. |
| `expect` | no | Alias for `action: expect` — verify an element exists without interacting. |
| `condition` | no | Selector query — skip this step if the element is NOT found |
| `on_missing` | no | What to do if selector not found: `error` (default), `skip`, `goto:step_id` |
| `save_as` | no | Save step result to a variable (for use in later steps) |

*`action` is required unless `wait` is the only field (pure delay step).

### Template Variables

Use `{{variable_name}}` in any string value. Variables come from:

1. **Steps input** — defined in the `input:` section, passed at runtime
2. **Step results** — saved with `save_as`, accessed as `{{step_id.field}}`

```yaml
steps:
  - id: get_field
    action: get_text
    save_as: current_text

  - id: log_it
    action: type
    params:
      text: "Previous text was: {{get_field.text}}"
```

### Conditional Steps

```yaml
# Skip step if element doesn't exist
- id: dismiss_popup
  action: click
  selector: "text:Dismiss"
  condition: "text:Dismiss"    # Only run if Dismiss button exists
  on_missing: skip             # Don't error if not found

# Branch to different step
- id: check_login
  action: expect
  selector: "text:Dashboard"
  timeout: 2000
  on_missing: "goto:login_flow"

- id: continue
  action: click
  selector: "text:Settings"
  # ... normal flow continues

- id: login_flow
  action: click
  selector: "text:Sign In"
  # ... login steps
```

### Expect (Validation)

Verify that an action had the expected result:

```yaml
# Standalone expect step
- id: verify_sent
  action: expect
  selector: "text:Message sent"
  timeout: 5000

# Inline expect on an action step
- id: click_save
  action: click
  selector: "text:Save"
  expect: "text:Saved successfully"
  expect_timeout: 3000
```

If an expect fails, the steps stops with an error (unless `on_missing: skip` is set).

### Screenshot Capture in Steps

```yaml
- id: capture_before
  action: screenshot
  save_as: before_image

- id: do_something
  action: click
  selector: "text:Process"

- id: capture_after
  action: screenshot
  save_as: after_image
```

### Scroll Until Found

A common pattern: scroll down until an element appears.

```yaml
- id: find_item
  action: scroll_until
  selector: "text:{{item_name}}"
  direction: down
  max_scrolls: 10
  scroll_amount: 500
```

This is a composite action in the steps runner: repeatedly scroll + ui_tree until the selector matches or max_scrolls is reached.

### Retry on Error

```yaml
- id: flaky_step
  action: click
  selector: "text:Submit"
  retries: 3
  retry_delay: 1000    # 1s between retries
```

---

## Steps Runner

### TypeScript SDK

```typescript
import { ScreenMCPClient, StepsRunner } from 'screenmcp';

const client = new ScreenMCPClient({ apiKey: 'pk_...' });
await client.connect();

const runner = new StepsRunner(client);

const result = await runner.run('steps/send_whatsapp.yml', {
  contact: 'Mom',
  message: 'Hi, calling you in 5 minutes',
});

console.log(result);
// {
//   status: 'ok',
//   steps_completed: 7,
//   steps_total: 7,
//   duration_ms: 4230,
//   last_step: 'verify',
//   variables: { ... }
// }
```

### Python SDK

```python
from screenmcp import ScreenMCPClient, StepsRunner

async with ScreenMCPClient(api_key='pk_...') as client:
    runner = StepsRunner(client)
    result = await runner.run('steps/send_whatsapp.yml', {
        'contact': 'Mom',
        'message': 'Hi, calling you in 5 minutes',
    })
```

### CLI

```bash
# Run a steps
screenmcp run send_whatsapp.yml --input '{"contact":"Mom","message":"Hello"}'

# Dry run — parse and validate without executing
screenmcp run send_whatsapp.yml --dry-run

# Verbose — print each step as it executes
screenmcp run send_whatsapp.yml --verbose --input '{"contact":"Mom","message":"Hello"}'

# Start from a specific step (for debugging)
screenmcp run send_whatsapp.yml --from find_contact --input '{"contact":"Mom","message":"Hello"}'
```

### MCP Tool

```json
{
  "name": "run_steps",
  "description": "Execute a predefined automation steps",
  "inputSchema": {
    "steps": "string — steps name or path",
    "input": "object — steps input variables"
  }
}
```

This lets Claude/Cursor call steps directly: "Run the send_whatsapp steps with contact=Mom and message=Hello".

---

## Execution Model

```
Steps YAML
    │
    ▼
Steps Runner (SDK, client-side)
    │
    ├── Parse YAML, validate input schema
    ├── For each step:
    │     ├── Check condition (if present): call ui_tree, search for selector
    │     ├── If selector: call ui_tree → search tree → get coordinates
    │     │     └── If not found: retry until timeout, then on_missing behavior
    │     ├── Execute command via WebSocket (click, type, screenshot, etc.)
    │     ├── If expect: call ui_tree → search → verify element exists
    │     ├── If save_as: store result in variables
    │     └── Apply wait delay
    └── Return execution result
```

**Zero protocol changes.** The steps runner uses existing commands: `ui_tree`, `click`, `type`, `screenshot`, etc. It's purely a client-side orchestration layer.

---

## Build Order

### Phase 1: Selector Engine
- `findElement(tree, query)` — parse selector, walk tree, return matching nodes
- `find()`, `findAll()`, `exists()`, `waitFor()` methods on the SDK client
- Selector syntax: `text:`, `role:`, `desc:`, `id:`, `&&`, `||`, `!`, `[0]`, `[focused]`
- Add to both TypeScript and Python SDKs

### Phase 2: Steps Runner
- YAML parser with input validation
- Step executor with selector integration
- Template variable substitution
- `wait`, `timeout`, `on_missing`, `expect` support
- `StepsRunner` class in both SDKs

### Phase 3: CLI Integration
- `screenmcp run <steps.yml>` command
- `--input`, `--verbose`, `--dry-run`, `--from` flags
- Bundled example steps

### Phase 4: Advanced Features
- `scroll_until` composite action
- `save_as` / step variable references
- `condition` / `on_missing: goto:step_id` branching
- `retries` / `retry_delay`
- MCP `run_steps` tool

### Phase 5: Recorder (future)
- Record a manual CLI session (commands + ui_tree snapshots)
- Generate YAML steps from recorded session
- Replace coordinates with selectors where possible
