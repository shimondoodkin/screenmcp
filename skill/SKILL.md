# ScreenMCP Agent Skill

Control an Android phone remotely using natural language instructions. This skill connects to a real Android device via the ScreenMCP platform and executes UI automation tasks through a vision-based agent loop.

## Configuration

Set these environment variables before using the skill:

| Variable | Required | Description |
|---|---|---|
| `SCREENMCP_API_KEY` | Yes | Your ScreenMCP API key (starts with `pk_`) |
| `ANTHROPIC_API_KEY` | Yes | Your Anthropic API key for Claude vision |
| `SCREENMCP_API_URL` | No | ScreenMCP server URL (default: `https://server10.doodkin.com`) |
| `SCREENMCP_DEVICE_ID` | No | Target device UUID (default: first registered device) |

## Available Phone Commands

| Command | Description | Parameters |
|---|---|---|
| `screenshot` | Capture the current screen | quality, max_width, max_height |
| `click` | Tap at coordinates | x, y |
| `long_click` | Long-press at coordinates | x, y |
| `drag` | Drag between two points | start_x, start_y, end_x, end_y, duration |
| `scroll` | Scroll the screen | direction (up/down/left/right), amount |
| `type` | Type text into focused input | text |
| `get_text` | Get text from focused element | - |
| `select_all` | Select all text in focused input | - |
| `copy` | Copy selection to clipboard | - |
| `paste` | Paste from clipboard | - |
| `back` | Press Back button | - |
| `home` | Press Home button | - |
| `recents` | Open app switcher | - |
| `ui_tree` | Get accessibility tree | - |
| `camera` | Take a photo | facing (rear/front), quality |

## Usage

Run the agent with a natural language instruction:

```bash
cd skill
pip install -r requirements.txt
python screenmcp_agent.py "Open the Settings app and turn on Wi-Fi"
```

## Example Prompts

- "Open Chrome and search for weather in New York"
- "Take a photo with the rear camera"
- "Open Settings and check the battery level"
- "Send a text message to John saying I'll be late"
- "Open the calculator and compute 42 * 17"
- "Scroll down in the current app to find the Save button and tap it"

## How It Works

The agent runs a loop (up to 15 steps by default):

1. Takes a screenshot of the phone screen
2. Sends the screenshot to Claude with the current instruction and action history
3. Claude analyzes the screen and decides the next action
4. The agent executes that action on the phone
5. Repeats until Claude determines the task is complete or max steps are reached

## Integration with Claude Code

To use this as a Claude Code skill, add the following to your project's `.claude/settings.json`:

```json
{
  "permissions": {
    "allow": ["Bash(python /path/to/skill/screenmcp_agent.py *)"]
  }
}
```

Then invoke from Claude Code:

```
/skill screenmcp-agent "Open Settings and enable dark mode"
```
