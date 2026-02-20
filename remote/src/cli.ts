#!/usr/bin/env node
import { Command } from "commander";
import { PhoneClient } from "./client.js";
import * as fs from "fs";
import * as readline from "readline";

const DEFAULT_API_URL = "https://phonemcp-api.ngrok-free.app";

const program = new Command();

program
  .name("phonemcp")
  .description("PhoneMCP remote client CLI")
  .option("--api <url>", "API server URL for discovery", DEFAULT_API_URL)
  .requiredOption("--token <token>", "Auth token (API key or Firebase ID token)")
  .requiredOption("--device-id <deviceId>", "Target device ID");

function createClient(): PhoneClient {
  const opts = program.opts<{ api: string; token: string; deviceId: string }>();
  return new PhoneClient({
    apiUrl: opts.api,
    token: opts.token,
    targetDeviceId: opts.deviceId,
  });
}

program
  .command("screenshot [outfile]")
  .description("Take a screenshot and save as WebP")
  .option("-q, --quality <n>", "WebP quality (1-99 lossy, 100+ lossless)", parseInt)
  .option("--max-width <n>", "Max width in pixels", parseInt)
  .option("--max-height <n>", "Max height in pixels", parseInt)
  .action(async (outfile: string | undefined, opts: { quality?: number; maxWidth?: number; maxHeight?: number }) => {
    const client = createClient();
    try {
      await client.connect();
      console.log(
        `Connected to ${client.workerUrl}. Phone: ${client.phoneConnected ? "online" : "offline"}`
      );

      const base64 = await client.screenshot({
        quality: opts.quality,
        max_width: opts.maxWidth,
        max_height: opts.maxHeight,
      });
      const buf = Buffer.from(base64, "base64");
      const filename = outfile || `screenshot_${Date.now()}.webp`;
      fs.writeFileSync(filename, buf);
      console.log(`Saved ${filename} (${buf.length} bytes)`);
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("click <x> <y> [duration]")
  .description("Tap at screen coordinates (optional duration in ms)")
  .action(async (x: string, y: string, duration?: string) => {
    const client = createClient();
    try {
      await client.connect();
      const dur = duration ? parseInt(duration) : undefined;
      await client.click(parseFloat(x), parseFloat(y), dur);
      console.log(`Clicked at (${x}, ${y})${dur ? ` for ${dur}ms` : ""}`);
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("type <text>")
  .description("Type text into focused input")
  .action(async (text: string) => {
    const client = createClient();
    try {
      await client.connect();
      await client.type(text);
      console.log(`Typed: ${text}`);
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("tree")
  .description("Get the UI accessibility tree")
  .action(async () => {
    const client = createClient();
    try {
      await client.connect();
      const tree = await client.getUiTree();
      console.log(JSON.stringify(tree, null, 2));
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("scroll <x> <y> <dx> <dy>")
  .description("Finger scroll at (x,y) by (dx,dy)")
  .action(async (x: string, y: string, dx: string, dy: string) => {
    const client = createClient();
    try {
      await client.connect();
      await client.scroll(parseFloat(x), parseFloat(y), parseFloat(dx), parseFloat(dy));
      console.log(`Scrolled at (${x}, ${y}) by (${dx}, ${dy})`);
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("camera [cameraId]")
  .description("Capture photo from camera (0=rear, 1=front)")
  .option("-q, --quality <n>", "WebP quality (1-100)", parseInt)
  .option("--max-width <n>", "Max width in pixels", parseInt)
  .option("--max-height <n>", "Max height in pixels", parseInt)
  .option("-o, --output <file>", "Output filename")
  .action(async (cameraId: string | undefined, opts: { quality?: number; maxWidth?: number; maxHeight?: number; output?: string }) => {
    const client = createClient();
    try {
      await client.connect();
      const base64 = await client.camera({
        camera: cameraId !== undefined ? parseInt(cameraId) : undefined,
        quality: opts.quality,
        max_width: opts.maxWidth,
        max_height: opts.maxHeight,
      });
      if (!base64) {
        console.log("No image returned (camera may not be available)");
        return;
      }
      const buf = Buffer.from(base64, "base64");
      const filename = opts.output || `camera_${Date.now()}.webp`;
      fs.writeFileSync(filename, buf);
      console.log(`Saved ${filename} (${buf.length} bytes)`);
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("shell")
  .description("Interactive REPL for sending commands")
  .action(async () => {
    const client = createClient();

    client.on("reconnecting", () => {
      console.log("\n[worker disconnected, rediscovering...]");
    });
    client.on("reconnected", (url: string) => {
      console.log(`[reconnected to ${url}]`);
    });
    client.on("reconnect_exhausted", () => {
      console.log("[failed to reconnect after all retries]");
      process.exit(1);
    });

    try {
      await client.connect();
      console.log(
        `Connected to ${client.workerUrl}. Phone: ${client.phoneConnected ? "online" : "offline"}`
      );

      client.on("phone_status", (connected: boolean) => {
        console.log(
          `\n[phone ${connected ? "connected" : "disconnected"}]`
        );
      });

      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
        prompt: "phonemcp> ",
      });

      rl.prompt();

      rl.on("line", async (line: string) => {
        const parts = line.trim().split(/\s+/);
        const cmd = parts[0];

        if (!cmd) {
          rl.prompt();
          return;
        }

        try {
          switch (cmd) {
            case "screenshot": {
              const ssOpts: { quality?: number; max_width?: number; max_height?: number } = {};
              let ssFile: string | undefined;
              for (let i = 1; i < parts.length; i++) {
                if (parts[i] === "--quality" || parts[i] === "-q") ssOpts.quality = parseInt(parts[++i]);
                else if (parts[i] === "--max-width") ssOpts.max_width = parseInt(parts[++i]);
                else if (parts[i] === "--max-height") ssOpts.max_height = parseInt(parts[++i]);
                else if (!ssFile) ssFile = parts[i];
              }
              const base64 = await client.screenshot(Object.keys(ssOpts).length > 0 ? ssOpts : undefined);
              const buf = Buffer.from(base64, "base64");
              const filename = ssFile || `screenshot_${Date.now()}.webp`;
              fs.writeFileSync(filename, buf);
              console.log(`Saved ${filename} (${buf.length} bytes)`);
              break;
            }
            case "click": {
              const x = parseFloat(parts[1]);
              const y = parseFloat(parts[2]);
              const dur = parts[3] ? parseInt(parts[3]) : undefined;
              await client.click(x, y, dur);
              console.log(`Clicked at (${x}, ${y})${dur ? ` for ${dur}ms` : ""}`);
              break;
            }
            case "long_click": {
              const x = parseFloat(parts[1]);
              const y = parseFloat(parts[2]);
              await client.longClick(x, y);
              console.log(`Long-clicked at (${x}, ${y})`);
              break;
            }
            case "drag": {
              const sx = parseFloat(parts[1]);
              const sy = parseFloat(parts[2]);
              const ex = parseFloat(parts[3]);
              const ey = parseFloat(parts[4]);
              await client.drag(sx, sy, ex, ey);
              console.log(`Dragged from (${sx},${sy}) to (${ex},${ey})`);
              break;
            }
            case "type": {
              const text = parts.slice(1).join(" ");
              await client.type(text);
              console.log(`Typed: ${text}`);
              break;
            }
            case "get_text": {
              const text = await client.getText();
              console.log(`Text: ${text}`);
              break;
            }
            case "tree": {
              const tree = await client.getUiTree();
              console.log(JSON.stringify(tree, null, 2));
              break;
            }
            case "back":
              await client.back();
              console.log("Back");
              break;
            case "home":
              await client.home();
              console.log("Home");
              break;
            case "recents":
              await client.recents();
              console.log("Recents");
              break;
            case "scroll": {
              const sx = parseFloat(parts[1]);
              const sy = parseFloat(parts[2]);
              const sdx = parseFloat(parts[3]);
              const sdy = parseFloat(parts[4]);
              await client.scroll(sx, sy, sdx, sdy);
              console.log(`Scrolled at (${sx}, ${sy}) by (${sdx}, ${sdy})`);
              break;
            }
            case "right_click": {
              const rx = parseFloat(parts[1]);
              const ry = parseFloat(parts[2]);
              const rResp = await client.rightClick(rx, ry);
              console.log(`Right-click at (${rx}, ${ry})`, (rResp.result as Record<string, unknown>)?.unsupported ? "(unsupported on this device)" : "");
              break;
            }
            case "middle_click": {
              const mx = parseFloat(parts[1]);
              const my = parseFloat(parts[2]);
              const mResp = await client.middleClick(mx, my);
              console.log(`Middle-click at (${mx}, ${my})`, (mResp.result as Record<string, unknown>)?.unsupported ? "(unsupported on this device)" : "");
              break;
            }
            case "mouse_scroll": {
              const msx = parseFloat(parts[1]);
              const msy = parseFloat(parts[2]);
              const msdx = parseFloat(parts[3]);
              const msdy = parseFloat(parts[4]);
              const msResp = await client.mouseScroll(msx, msy, msdx, msdy);
              console.log(`Mouse scroll at (${msx}, ${msy}) by (${msdx}, ${msdy})`, (msResp.result as Record<string, unknown>)?.unsupported ? "(unsupported on this device)" : "");
              break;
            }
            case "camera": {
              const camOpts: { camera?: number; quality?: number; max_width?: number; max_height?: number } = {};
              let camFile: string | undefined;
              if (parts[1] && !parts[1].startsWith("-")) camOpts.camera = parseInt(parts[1]);
              for (let i = camOpts.camera !== undefined ? 2 : 1; i < parts.length; i++) {
                if (parts[i] === "--quality" || parts[i] === "-q") camOpts.quality = parseInt(parts[++i]);
                else if (parts[i] === "--max-width") camOpts.max_width = parseInt(parts[++i]);
                else if (parts[i] === "--max-height") camOpts.max_height = parseInt(parts[++i]);
                else if (parts[i] === "-o" || parts[i] === "--output") camFile = parts[++i];
              }
              const camBase64 = await client.camera(Object.keys(camOpts).length > 0 ? camOpts : undefined);
              if (!camBase64) {
                console.log("No image returned (camera may not be available)");
              } else {
                const camBuf = Buffer.from(camBase64, "base64");
                const camFilename = camFile || `camera_${Date.now()}.webp`;
                fs.writeFileSync(camFilename, camBuf);
                console.log(`Saved ${camFilename} (${camBuf.length} bytes)`);
              }
              break;
            }
            case "help":
              console.log(
                "Commands: screenshot [file] [--quality N] [--max-width N] [--max-height N], click <x> <y> [duration], long_click <x> <y>, drag <sx> <sy> <ex> <ey>, scroll <x> <y> <dx> <dy>, type <text>, get_text, tree, back, home, recents, right_click <x> <y>, middle_click <x> <y>, mouse_scroll <x> <y> <dx> <dy>, camera [id] [--quality N] [--max-width N] [--max-height N], quit"
              );
              break;
            case "quit":
            case "exit":
              rl.close();
              await client.disconnect();
              process.exit(0);
              break;
            default:
              console.log(
                `Unknown command: ${cmd}. Type 'help' for commands.`
              );
          }
        } catch (e) {
          console.error("Error:", (e as Error).message);
        }

        rl.prompt();
      });

      rl.on("close", async () => {
        await client.disconnect();
        process.exit(0);
      });
    } catch (e) {
      console.error("Connection error:", (e as Error).message);
      process.exit(1);
    }
  });

program.parse();
