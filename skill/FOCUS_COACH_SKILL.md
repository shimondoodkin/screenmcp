---
name: screenmcp-focus-coach
description: Adaptive focus coaching using periodic ScreenMCP screenshots to compare current activity against known user plans and work patterns. Use when the assistant should monitor plan adherence and send gentle accountability nudges, with temporary pause support and safe voice notifications.
---

# ScreenMCP Focus Coach (OpenClaw-Oriented)

## Objective
Keep the user aligned with planned work using screenshot-based feedback and gentle nudges.

## Core Assumptions
- Agent already knows user plans and work style from memory/context.
- No extra setup questions required before running.
- Coaching is proactive but non-aggressive.

## Monitoring Cadence (Adaptive)

### Base cadence
- Start with checks every **5 minutes** during active focus block.
- Run only in short bursts (not continuous all day).

### Adaptive reduction when on-track
If confidence is high that user is on-plan for multiple checks:
- Skip some checks, or
- Expand interval to 10–15 min.

### Adaptive tightening when drift appears
If off-plan signals appear:
- Return to 5-minute cadence temporarily.
- If user returns on-plan, relax cadence again.

## Decision States
- **ON_PLAN**: current screen likely matches planned task/context.
- **OFF_PLAN**: clear mismatch with planned task.
- **UNCERTAIN**: ambiguous screenshot or mixed context.

## Confidence-Gated Responses

### ON_PLAN (high confidence)
- No interruption.
- Optionally reduce frequency.

### UNCERTAIN
- Use soft check-in question:
  - “Quick check — are you intentionally switching tasks?”
  - “Do you want to stay on this or return to your planned task?”

### OFF_PLAN (repeat/high confidence)
- Gentle accountability nudge:
  - “You planned to work on X. Want to jump back now?”
  - “Looks like drift from your planned block — should I help you return to X?”

## Tone Rules
- Prefer questioning language over directives.
- Never shame, pressure, or guilt.
- Escalate firmness only after repeated drift and only mildly.

## Temporary Pause Commands
Support temporary opt-out only:
- `pause focus coach 30m`
- `pause focus coach 2h`
- `pause focus coach today`
- `resume focus coach`

Behavior:
- Pause expires automatically.
- Resume restores adaptive cadence.

## Voice Nudge Policy (Critical)
If sending spoken alerts via `play_audio`:
- Keep spoken text generic and discreet.
- **Never read embarrassing or sensitive on-screen content aloud.**

Good spoken example:
- “Quick focus check: do you want to return to your planned task?”

Bad spoken example:
- Any message that reveals private/sensitive visible content.

## Minimal Loop
1. Capture screenshot (`screenshot`).
2. Compare with known current plan/work block.
3. Classify as ON_PLAN / OFF_PLAN / UNCERTAIN with confidence.
4. Decide whether to nudge (respect cooldown + adaptive cadence).
5. Log event briefly for later summary.

## Suggested Guardrails
- Nudge cooldown (e.g., 20 min between nudges).
- Max nudges per block/day.
- Silent mode during user-defined quiet windows.

## Daily Summary (Optional)
- Planned blocks covered
- Estimated on-plan rate
- Drift detections and recoveries
- Coaching pauses used
