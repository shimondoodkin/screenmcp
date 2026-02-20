# PhoneMCP AI Agent Skill

A vision-based AI agent that controls an Android phone using natural language instructions. The agent takes screenshots, analyzes the screen with Claude, and executes UI actions in a loop until the task is complete.

## Prerequisites

- Python 3.10+
- A PhoneMCP account with an API key (get one from the dashboard)
- An Anthropic API key
- An Android phone connected to PhoneMCP (with the PhoneMCP app installed and running)

## Setup

1. Install dependencies:

```bash
cd skill
pip install -r requirements.txt
```

2. Set environment variables:

```bash
export PHONEMCP_API_KEY="pk_your_api_key_here"
export ANTHROPIC_API_KEY="sk-ant-your_key_here"

# Optional:
export PHONEMCP_API_URL="https://server10.doodkin.com"   # default
export PHONEMCP_DEVICE_ID="your-device-uuid"              # optional, uses first device
export PHONEMCP_MAX_STEPS="15"                            # default: 15
```

## Usage

Run the agent with a natural language task:

```bash
python phonemcp_agent.py "Open the Settings app and check battery level"
```

The agent will:
1. Connect to your phone via the PhoneMCP platform
2. Take a screenshot
3. Send the screenshot to Claude for analysis
4. Execute the recommended action (tap, type, scroll, etc.)
5. Repeat until the task is done or max steps are reached

## Examples

```bash
# Basic navigation
python phonemcp_agent.py "Open Chrome and go to example.com"

# App interaction
python phonemcp_agent.py "Open the calculator and compute 123 + 456"

# Settings
python phonemcp_agent.py "Go to Settings and enable dark mode"

# Messaging
python phonemcp_agent.py "Open Messages and send 'Hello' to the first conversation"
```

## How It Works

The agent uses a simple loop architecture:

```
Screenshot -> Claude Vision Analysis -> Action Decision -> Execute -> Repeat
```

Each iteration, Claude receives:
- The current screenshot of the phone
- The original task instruction
- A history of all previous actions and their results

Claude responds with a structured JSON action, which the agent executes on the phone. This continues until Claude signals the task is complete or the maximum number of steps is reached.

## Troubleshooting

- **"Phone is not currently connected"**: Make sure the PhoneMCP Android app is running and connected on your phone.
- **"Screenshot failed"**: The phone may be unresponsive. Try restarting the PhoneMCP app.
- **Agent gets stuck in a loop**: The max steps limit (default 15) will stop it. You can adjust with `PHONEMCP_MAX_STEPS`.
- **Authentication errors**: Verify your `PHONEMCP_API_KEY` is correct and active.

## Claude Code Integration

To use this as a Claude Code skill, you can invoke it directly:

```bash
python /path/to/skill/phonemcp_agent.py "your instruction here"
```

Or reference the `SKILL.md` file for the full skill definition.
