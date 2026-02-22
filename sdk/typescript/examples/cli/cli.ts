#!/usr/bin/env node
import { Command } from "commander";
import { ScreenMCPClient } from "@screenmcp/sdk";
import * as fs from "fs";
import * as readline from "readline";

const program = new Command();

program
  .name("screenmcp")
  .description("CLI example using @screenmcp/sdk")
  .option("--api-url <url>", "API server URL")
  .option("--api-key <key>", "API key (pk_... format)")
  .option("--device-id <id>", "Target device ID (32 hex chars)");

function createClient(): ScreenMCPClient {
  const opts = program.opts<{ apiUrl?: string; apiKey?: string; deviceId?: string }>();
  if (!opts.apiKey) {
    console.error("Error: --api-key is required");
    process.exit(1);
  }
  return new ScreenMCPClient({
    apiKey: opts.apiKey,
    apiUrl: opts.apiUrl,
    deviceId: opts.deviceId,
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
      console.log(`Connected to ${client.workerUrl}. Phone: ${client.phoneConnected ? "online" : "offline"}`);

      const params: Record<string, unknown> = {};
      if (opts.quality !== undefined) params.quality = opts.quality;
      if (opts.maxWidth !== undefined) params.max_width = opts.maxWidth;
      if (opts.maxHeight !== undefined) params.max_height = opts.maxHeight;

      const resp = Object.keys(params).length > 0
        ? await client.sendCommand("screenshot", params)
        : await client.sendCommand("screenshot");
      const image = (resp.result as { image?: string })?.image ?? "";
      const buf = Buffer.from(image, "base64");
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
      const params: Record<string, unknown> = { x: parseFloat(x), y: parseFloat(y) };
      if (duration) params.duration = parseInt(duration);
      await client.sendCommand("click", params);
      console.log(`Clicked at (${x}, ${y})${duration ? ` for ${duration}ms` : ""}`);
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
      const { tree } = await client.uiTree();
      console.log(JSON.stringify(tree, null, 2));
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("scroll <direction> [amount]")
  .description('Scroll the screen (direction: up, down, left, right)')
  .action(async (direction: string, amount?: string) => {
    const client = createClient();
    try {
      await client.connect();
      const dir = direction as "up" | "down" | "left" | "right";
      await client.scroll(dir, amount ? parseInt(amount) : undefined);
      console.log(`Scrolled ${direction}${amount ? ` by ${amount}px` : ""}`);
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("camera [camera-id]")
  .description("Capture photo (camera ID, e.g. 0, 1, front, rear)")
  .option("-q, --quality <n>", "WebP quality (1-100)", parseInt)
  .option("--max-width <n>", "Max width in pixels", parseInt)
  .option("--max-height <n>", "Max height in pixels", parseInt)
  .option("-o, --output <file>", "Output filename")
  .action(async (cameraId: string | undefined, opts: { quality?: number; maxWidth?: number; maxHeight?: number; output?: string }) => {
    const client = createClient();
    try {
      await client.connect();
      const params: Record<string, unknown> = {};
      if (cameraId !== undefined) params.camera = cameraId;
      if (opts.quality !== undefined) params.quality = opts.quality;
      if (opts.maxWidth !== undefined) params.max_width = opts.maxWidth;
      if (opts.maxHeight !== undefined) params.max_height = opts.maxHeight;

      const resp = await client.sendCommand("camera", Object.keys(params).length > 0 ? params : undefined);
      const image = (resp.result as { image?: string })?.image;
      if (!image) {
        console.log("No image returned (camera may not be available)");
        return;
      }
      const buf = Buffer.from(image, "base64");
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
  .command("find <selector>")
  .description("Find UI elements matching a selector query")
  .option("-t, --timeout <ms>", "Timeout in ms (default: 3000)", parseInt)
  .action(async (selector: string, opts: { timeout?: number }) => {
    const client = createClient();
    try {
      await client.connect();
      const elements = await client.findAll(selector, { timeout: opts.timeout ?? 3000 });
      if (elements.length === 0) {
        console.log("No elements found");
      } else {
        for (const el of elements) {
          console.log(`  (${el.x}, ${el.y})  ${el.className ?? el.text ?? el.resourceId ?? "(unknown)"}`);
          if (el.text) console.log(`    text: ${el.text}`);
          if (el.className) console.log(`    role: ${el.className}`);
          if (el.resourceId) console.log(`    id: ${el.resourceId}`);
          if (el.contentDescription) console.log(`    desc: ${el.contentDescription}`);
          console.log(`    bounds: [${el.bounds.left}, ${el.bounds.top}, ${el.bounds.right}, ${el.bounds.bottom}]`);
        }
        console.log(`Found ${elements.length} element(s)`);
      }
    } catch (e) {
      console.error("Error:", (e as Error).message);
      process.exit(1);
    } finally {
      await client.disconnect();
    }
  });

program
  .command("run <steps-file>")
  .description("Execute a steps file (YAML or JSON)")
  .option("-i, --input <json>", "Input variables as JSON string")
  .option("-v, --verbose", "Print step-by-step execution")
  .option("--from <step-id>", "Start execution from this step")
  .option("--dry-run", "Parse and validate without executing")
  .action(async (stepsFile: string, opts: { input?: string; verbose?: boolean; from?: string; dryRun?: boolean }) => {
    const { StepsRunner } = await import("@screenmcp/sdk");
    const client = createClient();
    try {
      let inputData: Record<string, unknown> | undefined;
      if (opts.input) {
        inputData = JSON.parse(opts.input);
      }

      if (opts.dryRun) {
        // Just parse and validate the steps file
        const content = fs.readFileSync(stepsFile, "utf-8");
        let def;
        if (stepsFile.endsWith(".json")) {
          def = JSON.parse(content);
        } else {
          const yaml = await import("js-yaml");
          def = yaml.load(content);
        }
        console.log(`Name: ${def.name}`);
        console.log(`Steps: ${def.steps.length}`);
        if (def.input) {
          console.log(`Input schema:`);
          for (const [k, v] of Object.entries(def.input)) {
            const info = v as { type: string; description?: string };
            console.log(`  ${k}: ${info.type}${info.description ? ` — ${info.description}` : ""}`);
          }
        }
        console.log("\nDry run — no commands executed.");
        return;
      }

      await client.connect();
      console.log(`Connected to ${client.workerUrl}. Phone: ${client.phoneConnected ? "online" : "offline"}`);

      const runner = new StepsRunner(client, { verbose: opts.verbose ?? false });
      const result = await runner.runFile(stepsFile, inputData);

      if (result.status === "ok") {
        console.log(`\nCompleted: ${result.steps_completed}/${result.steps_total} steps in ${result.duration_ms}ms`);
      } else {
        console.error(`\nFailed at step [${result.last_step}]: ${result.error}`);
        console.error(`Completed: ${result.steps_completed}/${result.steps_total} steps in ${result.duration_ms}ms`);
        process.exit(1);
      }
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

    try {
      await client.connect();
      console.log(`Connected to ${client.workerUrl}. Phone: ${client.phoneConnected ? "online" : "offline"}`);

      client.on("phone_status", (connected: boolean) => {
        console.log(`\n[phone ${connected ? "connected" : "disconnected"}]`);
      });

      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
        prompt: "screenmcp> ",
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
              const ssParams: Record<string, unknown> = {};
              let ssFile: string | undefined;
              for (let i = 1; i < parts.length; i++) {
                if (parts[i] === "--quality" || parts[i] === "-q") ssParams.quality = parseInt(parts[++i]);
                else if (parts[i] === "--max-width") ssParams.max_width = parseInt(parts[++i]);
                else if (parts[i] === "--max-height") ssParams.max_height = parseInt(parts[++i]);
                else if (!ssFile) ssFile = parts[i];
              }
              const ssResp = await client.sendCommand("screenshot", Object.keys(ssParams).length > 0 ? ssParams : undefined);
              const ssImage = (ssResp.result as { image?: string })?.image ?? "";
              const buf = Buffer.from(ssImage, "base64");
              const filename = ssFile || `screenshot_${Date.now()}.webp`;
              fs.writeFileSync(filename, buf);
              console.log(`Saved ${filename} (${buf.length} bytes)`);
              break;
            }
            case "click": {
              const cx = parseFloat(parts[1]);
              const cy = parseFloat(parts[2]);
              const dur = parts[3] ? parseInt(parts[3]) : undefined;
              const clickParams: Record<string, unknown> = { x: cx, y: cy };
              if (dur) clickParams.duration = dur;
              await client.sendCommand("click", clickParams);
              console.log(`Clicked at (${cx}, ${cy})${dur ? ` for ${dur}ms` : ""}`);
              break;
            }
            case "long_click": {
              const lx = parseFloat(parts[1]);
              const ly = parseFloat(parts[2]);
              await client.longClick(lx, ly);
              console.log(`Long-clicked at (${lx}, ${ly})`);
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
              const { text } = await client.getText();
              console.log(`Text: ${text}`);
              break;
            }
            case "tree": {
              const { tree } = await client.uiTree();
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
            case "select_all":
              await client.selectAll();
              console.log("Selected all");
              break;
            case "copy": {
              const returnText = parts.includes("--return-text");
              const copyResult = await client.copy({ returnText });
              if (returnText && copyResult.text !== undefined) {
                console.log(`Copied: ${copyResult.text}`);
              } else {
                console.log("Copied");
              }
              break;
            }
            case "paste": {
              const pasteText = parts.length > 1 ? parts.slice(1).join(" ") : undefined;
              await client.paste(pasteText);
              console.log(pasteText ? `Pasted: ${pasteText}` : "Pasted");
              break;
            }
            case "get_clipboard": {
              const clip = await client.getClipboard();
              console.log(`Clipboard: ${clip.text}`);
              break;
            }
            case "set_clipboard": {
              const clipText = parts.slice(1).join(" ");
              if (!clipText) { console.log("Usage: set_clipboard <text>"); break; }
              await client.setClipboard(clipText);
              console.log(`Clipboard set to: ${clipText}`);
              break;
            }
            case "scroll": {
              const dir = parts[1] as "up" | "down" | "left" | "right";
              const amt = parts[2] ? parseInt(parts[2]) : undefined;
              await client.scroll(dir, amt);
              console.log(`Scrolled ${dir}${amt ? ` by ${amt}px` : ""}`);
              break;
            }
            case "right_click": {
              const rx = parseFloat(parts[1]);
              const ry = parseFloat(parts[2]);
              const rResp = await client.sendCommand("right_click", { x: rx, y: ry });
              console.log(`Right-click at (${rx}, ${ry})`, (rResp.result as Record<string, unknown>)?.unsupported ? "(unsupported on this device)" : "");
              break;
            }
            case "middle_click": {
              const mx = parseFloat(parts[1]);
              const my = parseFloat(parts[2]);
              const mResp = await client.sendCommand("middle_click", { x: mx, y: my });
              console.log(`Middle-click at (${mx}, ${my})`, (mResp.result as Record<string, unknown>)?.unsupported ? "(unsupported on this device)" : "");
              break;
            }
            case "mouse_scroll": {
              const msx = parseFloat(parts[1]);
              const msy = parseFloat(parts[2]);
              const msdx = parseFloat(parts[3]);
              const msdy = parseFloat(parts[4]);
              const msResp = await client.sendCommand("mouse_scroll", { x: msx, y: msy, dx: msdx, dy: msdy });
              console.log(`Mouse scroll at (${msx}, ${msy}) by (${msdx}, ${msdy})`, (msResp.result as Record<string, unknown>)?.unsupported ? "(unsupported on this device)" : "");
              break;
            }
            case "list_cameras": {
              const lcResult = await client.listCameras();
              if (lcResult.cameras.length === 0) {
                console.log("No cameras available");
              } else {
                for (const cam of lcResult.cameras) {
                  console.log(`  ${cam.id}: ${cam.facing}`);
                }
              }
              break;
            }
            case "camera": {
              const camParams: Record<string, unknown> = {};
              let camFile: string | undefined;
              const startIdx = (parts[1] && !parts[1].startsWith("-")) ? (camParams.camera = parts[1], 2) : 1;
              for (let i = startIdx; i < parts.length; i++) {
                if (parts[i] === "--quality" || parts[i] === "-q") camParams.quality = parseInt(parts[++i]);
                else if (parts[i] === "--max-width") camParams.max_width = parseInt(parts[++i]);
                else if (parts[i] === "--max-height") camParams.max_height = parseInt(parts[++i]);
                else if (parts[i] === "-o" || parts[i] === "--output") camFile = parts[++i];
              }
              const camResp = await client.sendCommand("camera", Object.keys(camParams).length > 0 ? camParams : undefined);
              const camImage = (camResp.result as { image?: string })?.image;
              if (!camImage) {
                console.log("No image returned (camera may not be available)");
              } else {
                const camBuf = Buffer.from(camImage, "base64");
                const camFilename = camFile || `camera_${Date.now()}.webp`;
                fs.writeFileSync(camFilename, camBuf);
                console.log(`Saved ${camFilename} (${camBuf.length} bytes)`);
              }
              break;
            }
            case "find": {
              const findSelector = parts.slice(1).join(" ");
              if (!findSelector) { console.log("Usage: find <selector>"); break; }
              const findTimeout = 3000;
              const elements = await client.findAll(findSelector, { timeout: findTimeout });
              if (elements.length === 0) {
                console.log("No elements found");
              } else {
                for (const el of elements) {
                  console.log(`  (${el.x}, ${el.y})  ${el.className ?? el.text ?? "(unknown)"}`);
                  if (el.text) console.log(`    text: ${el.text}`);
                  if (el.resourceId) console.log(`    id: ${el.resourceId}`);
                }
                console.log(`Found ${elements.length} element(s)`);
              }
              break;
            }
            case "click_on": {
              // Click on element by selector (syntactic sugar)
              const clickSelector = parts.slice(1).join(" ");
              if (!clickSelector) { console.log("Usage: click_on <selector>"); break; }
              try {
                await client.find(clickSelector, { timeout: 3000 }).click();
                console.log(`Clicked on: ${clickSelector}`);
              } catch (e) {
                console.error("Error:", (e as Error).message);
              }
              break;
            }
            case "run": {
              const runFile = parts[1];
              if (!runFile) { console.log("Usage: run <steps.yml> [--input '{...}']"); break; }
              const { StepsRunner } = await import("@screenmcp/sdk");
              let runInput: Record<string, unknown> | undefined;
              const inputIdx = parts.indexOf("--input");
              if (inputIdx !== -1 && parts[inputIdx + 1]) {
                runInput = JSON.parse(parts.slice(inputIdx + 1).join(" "));
              }
              const runner = new StepsRunner(client, { verbose: true });
              const runResult = await runner.runFile(runFile, runInput);
              if (runResult.status === "ok") {
                console.log(`Completed: ${runResult.steps_completed}/${runResult.steps_total} steps in ${runResult.duration_ms}ms`);
              } else {
                console.log(`Failed at [${runResult.last_step}]: ${runResult.error}`);
              }
              break;
            }
            case "help":
              console.log(
                "Commands: screenshot [file] [--quality N] [--max-width N] [--max-height N], " +
                "click <x> <y> [duration], long_click <x> <y>, drag <sx> <sy> <ex> <ey>, " +
                "scroll <direction> [amount], type <text>, get_text, select_all, " +
                "copy [--return-text], paste [text], get_clipboard, set_clipboard <text>, " +
                "tree, back, home, recents, right_click <x> <y>, middle_click <x> <y>, " +
                "mouse_scroll <x> <y> <dx> <dy>, list_cameras, camera [id] [--quality N] " +
                "[--max-width N] [--max-height N] [-o file], find <selector>, " +
                "click_on <selector>, run <steps.yml> [--input '{...}'], quit"
              );
              break;
            case "quit":
            case "exit":
              rl.close();
              await client.disconnect();
              process.exit(0);
              break;
            default:
              console.log(`Unknown command: ${cmd}. Type 'help' for commands.`);
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
