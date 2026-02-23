#!/usr/bin/env node
/**
 * ScreenMCP Timing Client
 *
 * Connects to a ScreenMCP server and measures timing at every step.
 * Useful for diagnosing latency issues.
 *
 * Usage:
 *   node timing.mjs <api-url> <api-key> <device-id> [--loop N]
 *
 * Example:
 *   node timing.mjs http://localhost:3000 pk_abc123 a1b2c3d4
 *   node timing.mjs http://localhost:3000 pk_abc123 a1b2c3d4 --loop 5
 */

import WebSocket from "ws";

const API_URL = process.argv[2];
const API_KEY = process.argv[3];
const DEVICE_ID = process.argv[4];
const loopIdx = process.argv.indexOf("--loop");
const LOOP_COUNT = loopIdx !== -1 ? parseInt(process.argv[loopIdx + 1]) || 3 : 1;

if (!API_URL || !API_KEY || !DEVICE_ID) {
  console.error("Usage: node timing.mjs <api-url> <api-key> <device-id> [--loop N]");
  console.error("Example: node timing.mjs http://localhost:3000 pk_abc123 a1b2c3d4");
  process.exit(1);
}

function ts() {
  return new Date().toISOString().slice(11, 23);
}

function log(msg) {
  console.log(`[${ts()}] ${msg}`);
}

function elapsed(startMs) {
  return `${Date.now() - startMs}ms`;
}

async function discover() {
  const t0 = Date.now();
  log(`DISCOVER: POST ${API_URL}/api/discover`);

  const resp = await fetch(`${API_URL}/api/discover`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${API_KEY}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ device_id: DEVICE_ID }),
  });

  if (!resp.ok) {
    const body = await resp.text();
    throw new Error(`Discovery failed (${resp.status}): ${body}`);
  }

  const data = await resp.json();
  log(`DISCOVER: got wsUrl=${data.wsUrl} (${elapsed(t0)})`);
  return data.wsUrl;
}

function connectWs(wsUrl) {
  return new Promise((resolve, reject) => {
    const t0 = Date.now();
    log(`WS_CONNECT: opening ${wsUrl}`);

    const ws = new WebSocket(wsUrl);

    ws.on("open", () => {
      log(`WS_OPEN: connected (${elapsed(t0)})`);
      const auth = {
        type: "auth",
        key: API_KEY,
        role: "controller",
        target_device_id: DEVICE_ID,
        last_ack: 0,
      };
      log(`AUTH: sending auth message`);
      ws.send(JSON.stringify(auth));
    });

    // One-shot handler for auth response
    const onMessage = (raw) => {
      const msg = JSON.parse(raw.toString());

      if (msg.type === "auth_ok") {
        log(`AUTH_OK: phone_connected=${msg.phone_connected} (${elapsed(t0)})`);
        ws.removeListener("message", onMessage);
        resolve({ ws, phoneConnected: msg.phone_connected });
      } else if (msg.type === "auth_fail") {
        ws.removeListener("message", onMessage);
        reject(new Error(`Auth failed: ${msg.error}`));
      } else if (msg.type === "phone_status") {
        log(`PHONE_STATUS: connected=${msg.connected} (${elapsed(t0)})`);
      } else if (msg.type === "ping") {
        ws.send(JSON.stringify({ type: "pong" }));
      }
    };

    ws.on("message", onMessage);

    ws.on("error", (err) => {
      reject(err);
    });

    ws.on("close", (code, reason) => {
      log(`WS_CLOSE: ${code} ${reason}`);
    });
  });
}

function waitForPhone(ws, timeoutMs = 30000) {
  return new Promise((resolve, reject) => {
    const t0 = Date.now();
    log(`WAIT_PHONE: waiting for phone to connect (timeout ${timeoutMs}ms)...`);

    const timeout = setTimeout(() => {
      ws.removeListener("message", onMessage);
      reject(new Error("Timed out waiting for phone"));
    }, timeoutMs);

    const onMessage = (raw) => {
      const msg = JSON.parse(raw.toString());
      if (msg.type === "phone_status" && msg.connected) {
        clearTimeout(timeout);
        ws.removeListener("message", onMessage);
        log(`PHONE_CONNECTED: phone came online (${elapsed(t0)})`);
        resolve();
      } else if (msg.type === "ping") {
        ws.send(JSON.stringify({ type: "pong" }));
      }
    };

    ws.on("message", onMessage);
  });
}

function sendScreenshot(ws) {
  return new Promise((resolve, reject) => {
    const t0 = Date.now();
    log(`CMD_SEND: screenshot (quality=80, max_width=720)`);
    ws.send(
      JSON.stringify({
        cmd: "screenshot",
        params: { quality: 80, max_width: 720 },
      })
    );

    const timeout = setTimeout(() => {
      ws.removeListener("message", onMessage);
      reject(new Error("Screenshot timed out after 30s"));
    }, 30000);

    const onMessage = (raw) => {
      const msg = JSON.parse(raw.toString());

      if (msg.type === "cmd_accepted") {
        log(`CMD_ACCEPTED: id=${msg.id} (${elapsed(t0)})`);
      } else if (
        msg.id !== undefined &&
        msg.status !== undefined &&
        !msg.type
      ) {
        clearTimeout(timeout);
        ws.removeListener("message", onMessage);
        const imageSize = msg.result?.image
          ? Math.round((msg.result.image.length * 3) / 4 / 1024)
          : 0;
        log(
          `CMD_RESPONSE: status=${msg.status} (${elapsed(t0)}, ${imageSize}KB)`
        );
        resolve(msg);
      } else if (msg.type === "phone_status") {
        log(`PHONE_STATUS: connected=${msg.connected} (${elapsed(t0)})`);
      } else if (msg.type === "ping") {
        ws.send(JSON.stringify({ type: "pong" }));
      }
    };

    ws.on("message", onMessage);
  });
}

async function main() {
  const totalStart = Date.now();
  log(`=== ScreenMCP Timing Client ===`);
  log(`API: ${API_URL}`);
  log(`Key: ${API_KEY.slice(0, 8)}...`);
  log(`Device: ${DEVICE_ID}`);
  log(`Loop: ${LOOP_COUNT} iterations`);
  console.log("");

  // Step 1: Discover
  const wsUrl = await discover();
  console.log("");

  // Step 2: Connect WebSocket
  const { ws, phoneConnected } = await connectWs(wsUrl);
  console.log("");

  // Step 3: Wait for phone if needed
  if (!phoneConnected) {
    await waitForPhone(ws);
    console.log("");
  }

  // Step 4: Send screenshot command(s)
  const times = [];
  for (let i = 0; i < LOOP_COUNT; i++) {
    if (LOOP_COUNT > 1) {
      log(`--- Iteration ${i + 1}/${LOOP_COUNT} ---`);
    }
    const cmdStart = Date.now();
    await sendScreenshot(ws);
    times.push(Date.now() - cmdStart);
    console.log("");
  }

  // Summary
  log(`=== Summary ===`);
  log(`Total time: ${elapsed(totalStart)}`);
  if (times.length > 1) {
    const avg = Math.round(times.reduce((a, b) => a + b, 0) / times.length);
    const min = Math.min(...times);
    const max = Math.max(...times);
    log(`Screenshot times: avg=${avg}ms, min=${min}ms, max=${max}ms`);
    log(`Individual: ${times.map((t) => `${t}ms`).join(", ")}`);
  } else {
    log(`Screenshot time: ${times[0]}ms`);
  }

  ws.close();
  process.exit(0);
}

main().catch((err) => {
  console.error(`\nERROR: ${err.message}`);
  process.exit(1);
});
