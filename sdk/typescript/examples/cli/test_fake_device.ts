/**
 * Integration test: TypeScript SDK -> MCP Server -> Worker -> Fake Device.
 *
 * Tests all major SDK methods against the running fake device.
 *
 * Prerequisites:
 *   1. Worker running on ws://localhost:8080
 *   2. MCP server running on http://localhost:3000
 *   3. Fake device running and connected
 *
 * Setup:
 *   # Ensure config:
 *   mkdir -p ~/.screenmcp
 *   cat > ~/.screenmcp/worker.toml << 'EOF'
 *   [user]
 *   id = "local-user"
 *   [auth]
 *   api_keys = ["pk_test123"]
 *   [devices]
 *   allowed = []
 *   [server]
 *   port = 3000
 *   worker_url = "ws://localhost:8080"
 *   EOF
 *
 *   # Start worker, mcp-server, fake-device, then:
 *   cd sdk/typescript/examples/cli
 *   npx tsx test_fake_device.ts [--api-url URL] [--api-key KEY] [--device-id ID]
 */

import { ScreenMCPClient, findElements } from "@screenmcp/sdk";
import type { FoundElement } from "@screenmcp/sdk";

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------
const args = process.argv.slice(2);
function getArg(name: string, defaultVal: string): string {
  const idx = args.indexOf(`--${name}`);
  if (idx >= 0 && args[idx + 1]) return args[idx + 1];
  return defaultVal;
}

const API_URL = getArg("api-url", "http://localhost:3000");
const API_KEY = getArg("api-key", "pk_test123");
const DEVICE_ID = getArg("device-id", "test-device-001");

// ---------------------------------------------------------------------------
// Test tracking
// ---------------------------------------------------------------------------
let passed = 0;
let failed = 0;
let skipped = 0;
const failures: { name: string; reason: string }[] = [];

function pass(name: string) {
  console.log(`  PASS  ${name}`);
  passed++;
}

function fail(name: string, reason: string) {
  console.error(`  FAIL  ${name}: ${reason}`);
  failed++;
  failures.push({ name, reason });
}

function skip(name: string, reason: string) {
  console.warn(`  SKIP  ${name}: ${reason}`);
  skipped++;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
async function runTests() {
  console.log("=".repeat(60));
  console.log("ScreenMCP TypeScript SDK Integration Test");
  console.log(`  API URL:    ${API_URL}`);
  console.log(`  API Key:    ${API_KEY}`);
  console.log(`  Device ID:  ${DEVICE_ID}`);
  console.log("=".repeat(60));

  const client = new ScreenMCPClient({
    apiKey: API_KEY,
    apiUrl: API_URL,
    deviceId: DEVICE_ID,
    commandTimeout: 10_000,
    autoReconnect: false,
  });

  // Connect
  try {
    await client.connect();
    pass(`connect (worker=${client.workerUrl}, phone=${client.phoneConnected})`);
  } catch (e) {
    fail("connect", (e as Error).message);
    process.exit(1);
  }

  // Wait for phone to connect (fake device connects via SSE -> WS after discover)
  if (!client.phoneConnected) {
    console.log("  Waiting for phone to connect...");
    for (let i = 0; i < 30; i++) {
      await new Promise((r) => setTimeout(r, 500));
      if (client.phoneConnected) break;
    }
    if (client.phoneConnected) {
      pass("phone connected");
    } else {
      fail("phone_connect", "Phone did not connect within 15s");
      await client.disconnect();
      process.exit(1);
    }
  }

  // screenshot
  try {
    const result = await client.screenshot();
    if (!result.image) {
      fail("screenshot", "No image returned");
    } else {
      const buf = Buffer.from(result.image, "base64");
      const isPng = buf[0] === 0x89 && buf[1] === 0x50;
      pass(`screenshot (${buf.length} bytes, PNG=${isPng})`);
    }
  } catch (e) {
    fail("screenshot", (e as Error).message);
  }

  // click
  try {
    await client.click(540, 960);
    pass("click(540, 960)");
  } catch (e) {
    fail("click", (e as Error).message);
  }

  // type
  try {
    await client.type("hello world");
    pass("type('hello world')");
  } catch (e) {
    fail("type", (e as Error).message);
  }

  // uiTree
  try {
    const result = await client.uiTree();
    if (!result.tree || result.tree.length === 0) {
      fail("uiTree", "Empty tree");
    } else {
      const root = result.tree[0] as Record<string, unknown>;
      const children = root.children as unknown[] | undefined;
      pass(`uiTree (root className=${root.className}, ${children?.length ?? 0} children)`);
    }
  } catch (e) {
    fail("uiTree", (e as Error).message);
  }

  // back
  try {
    await client.back();
    pass("back()");
  } catch (e) {
    fail("back", (e as Error).message);
  }

  // home
  try {
    await client.home();
    pass("home()");
  } catch (e) {
    fail("home", (e as Error).message);
  }

  // recents
  try {
    await client.recents();
    pass("recents()");
  } catch (e) {
    fail("recents", (e as Error).message);
  }

  // longClick
  try {
    await client.longClick(100, 200);
    pass("longClick(100, 200)");
  } catch (e) {
    fail("longClick", (e as Error).message);
  }

  // scroll
  try {
    await client.scroll("down", 500);
    pass("scroll('down', 500)");
  } catch (e) {
    fail("scroll", (e as Error).message);
  }

  // getText
  try {
    const result = await client.getText();
    if (result.text) {
      pass(`getText() -> '${result.text}'`);
    } else {
      fail("getText", "No text returned");
    }
  } catch (e) {
    fail("getText", (e as Error).message);
  }

  // copy
  try {
    const result = await client.copy({ returnText: true });
    pass(`copy({returnText: true}) -> text='${result.text ?? "(none)"}'`);
  } catch (e) {
    fail("copy", (e as Error).message);
  }

  // getClipboard
  try {
    const result = await client.getClipboard();
    pass(`getClipboard() -> '${result.text}'`);
  } catch (e) {
    fail("getClipboard", (e as Error).message);
  }

  // setClipboard
  try {
    await client.setClipboard("test content");
    pass("setClipboard('test content')");
  } catch (e) {
    fail("setClipboard", (e as Error).message);
  }

  // paste
  try {
    await client.paste();
    pass("paste()");
  } catch (e) {
    fail("paste", (e as Error).message);
  }

  // selectAll
  try {
    await client.selectAll();
    pass("selectAll()");
  } catch (e) {
    fail("selectAll", (e as Error).message);
  }

  // drag
  try {
    await client.drag(100, 200, 500, 600);
    pass("drag(100, 200, 500, 600)");
  } catch (e) {
    fail("drag", (e as Error).message);
  }

  // listCameras
  try {
    const result = await client.listCameras();
    pass(`listCameras() -> ${result.cameras.length} cameras`);
  } catch (e) {
    fail("listCameras", (e as Error).message);
  }

  // camera
  try {
    const result = await client.camera("0");
    if (result.image) {
      pass(`camera('0') -> ${result.image.length} base64 chars`);
    } else {
      fail("camera", "No image returned");
    }
  } catch (e) {
    fail("camera", (e as Error).message);
  }

  // pressKey
  try {
    await client.pressKey("Enter");
    pass("pressKey('Enter')");
  } catch (e) {
    fail("pressKey", (e as Error).message);
  }

  // holdKey + releaseKey
  try {
    await client.holdKey("Shift");
    await client.releaseKey("Shift");
    pass("holdKey('Shift') + releaseKey('Shift')");
  } catch (e) {
    fail("holdKey/releaseKey", (e as Error).message);
  }

  // ── Selector engine tests ─────────────────────────────────────────
  try {
    const { tree } = await client.uiTree();

    // text selector
    const settingsEls = findElements(tree, "text:Settings");
    if (settingsEls.length > 0 && settingsEls[0].text === "Settings") {
      pass(`findElements(text:Settings) -> (${settingsEls[0].x}, ${settingsEls[0].y})`);
    } else {
      fail("findElements text:Settings", `Expected Settings, got ${settingsEls.length} results`);
    }

    // role selector
    const editTexts = findElements(tree, "role:EditText");
    if (editTexts.length > 0) {
      pass(`findElements(role:EditText) -> ${editTexts[0].className}`);
    } else {
      fail("findElements role:EditText", "No EditText found");
    }

    // desc selector
    const homeEls = findElements(tree, "desc:Home");
    if (homeEls.length > 0) {
      pass(`findElements(desc:Home) -> (${homeEls[0].x}, ${homeEls[0].y})`);
    } else {
      fail("findElements desc:Home", "No element found");
    }

    // id selector
    const backEls = findElements(tree, "id:com.android.systemui:id/back");
    if (backEls.length > 0) {
      pass(`findElements(id:...back) -> (${backEls[0].x}, ${backEls[0].y})`);
    } else {
      fail("findElements id:...back", "No element found");
    }

  } catch (e) {
    fail("selector engine", (e as Error).message);
  }

  // find() fluent API
  try {
    const el = await client.find("text:Chrome", { timeout: 2000 }).element();
    if (el.text === "Chrome") {
      pass(`find('text:Chrome').element() -> (${el.x}, ${el.y})`);
    } else {
      fail("find fluent", `Expected Chrome, got ${el.text}`);
    }
  } catch (e) {
    fail("find fluent", (e as Error).message);
  }

  // exists()
  try {
    const exists = await client.exists("text:Settings", { timeout: 1000 });
    if (exists) {
      pass("exists('text:Settings') -> true");
    } else {
      fail("exists", "Expected true");
    }
  } catch (e) {
    fail("exists", (e as Error).message);
  }

  // exists() for non-existent element
  try {
    const exists = await client.exists("text:NonExistentElement123", { timeout: 500 });
    if (!exists) {
      pass("exists('text:NonExistentElement123') -> false");
    } else {
      fail("exists non-existent", "Expected false");
    }
  } catch (e) {
    fail("exists non-existent", (e as Error).message);
  }

  // find().click() via selector
  try {
    await client.find("text:Settings", { timeout: 2000 }).click();
    pass("find('text:Settings').click()");
  } catch (e) {
    fail("find.click", (e as Error).message);
  }

  // Unknown command should error
  try {
    await client.sendCommand("totally_fake_command_xyz");
    fail("unknown_command", "Expected error but got success");
  } catch (e) {
    pass(`unknown command raises error: ${(e as Error).message}`);
  }

  // Disconnect
  await client.disconnect();

  // Summary
  const total = passed + failed + skipped;
  console.log("\n" + "=".repeat(60));
  let summary = `Test Results: ${passed}/${total} passed`;
  if (failed) summary += `, ${failed} FAILED`;
  if (skipped) summary += `, ${skipped} skipped`;
  console.log(summary);

  if (failures.length > 0) {
    console.log("\nFailures:");
    for (const f of failures) {
      console.log(`  - ${f.name}: ${f.reason}`);
    }
  }

  console.log("=".repeat(60));
  process.exit(failed > 0 ? 1 : 0);
}

runTests().catch((e) => {
  console.error("Unhandled error:", e);
  process.exit(1);
});
