// ---------------------------------------------------------------------------
// Steps runner: execute YAML/JSON steps definitions against a ScreenMCP client
// ---------------------------------------------------------------------------

import type { DeviceConnection } from "./client.js";
import type { FoundElement } from "./selector.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface StepDefinition {
  id: string;
  description?: string;
  action?: string;
  selector?: string;
  params?: Record<string, unknown>;
  timeout?: number;
  wait?: number;
  expect?: string;
  expect_timeout?: number;
  condition?: string;
  on_missing?: string;
  save_as?: string;
  retries?: number;
  retry_delay?: number;
  // scroll_until specific
  direction?: string;
  max_scrolls?: number;
  scroll_amount?: number;
}

export interface StepsDefinition {
  name: string;
  description?: string;
  input?: Record<string, { type: string; description?: string }>;
  steps: StepDefinition[];
}

export interface StepsResult {
  status: "ok" | "error";
  steps_completed: number;
  steps_total: number;
  duration_ms: number;
  last_step?: string;
  error?: string;
  variables: Record<string, unknown>;
}

export interface StepsRunnerOptions {
  verbose?: boolean;
}

// ---------------------------------------------------------------------------
// Template substitution
// ---------------------------------------------------------------------------

function substituteTemplates(
  value: string,
  input: Record<string, unknown>,
  variables: Record<string, unknown>,
): string {
  return value.replace(/\{\{(\w+(?:\.\w+)*)\}\}/g, (_, key: string) => {
    const parts = key.split(".");
    let val: unknown = input[parts[0]] ?? variables[parts[0]];
    for (let i = 1; i < parts.length && val != null; i++) {
      val = (val as Record<string, unknown>)[parts[i]];
    }
    return val != null ? String(val) : `{{${key}}}`;
  });
}

function substituteInParams(
  params: Record<string, unknown>,
  input: Record<string, unknown>,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(params)) {
    if (typeof v === "string") {
      result[k] = substituteTemplates(v, input, variables);
    } else {
      result[k] = v;
    }
  }
  return result;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

class ElementNotFoundError extends Error {
  constructor(selector: string) {
    super(`Element not found: ${selector}`);
    this.name = "ElementNotFoundError";
  }
}

// ---------------------------------------------------------------------------
// StepsRunner
// ---------------------------------------------------------------------------

export class StepsRunner {
  private verbose: boolean;

  constructor(
    private client: DeviceConnection,
    options?: StepsRunnerOptions,
  ) {
    this.verbose = options?.verbose ?? false;
  }

  /** Run steps from a YAML string (requires js-yaml to be installed). */
  async runYaml(
    yaml: string,
    input?: Record<string, unknown>,
  ): Promise<StepsResult> {
    let def: StepsDefinition;
    try {
      const jsYaml = await import("js-yaml");
      def = jsYaml.load(yaml) as StepsDefinition;
    } catch (e) {
      if (
        e instanceof Error &&
        (e.message.includes("Cannot find module") ||
          e.message.includes("MODULE_NOT_FOUND"))
      ) {
        throw new Error(
          "js-yaml is required for YAML steps parsing. Install it: npm install js-yaml",
        );
      }
      throw e;
    }
    return this.run(def, input);
  }

  /** Run steps from a file path (JSON or YAML). */
  async runFile(
    filePath: string,
    input?: Record<string, unknown>,
  ): Promise<StepsResult> {
    const fs = await import("fs");
    const content = fs.readFileSync(filePath, "utf-8");

    let def: StepsDefinition;
    if (filePath.endsWith(".json")) {
      def = JSON.parse(content) as StepsDefinition;
    } else {
      // Assume YAML (.yaml, .yml, or anything else)
      try {
        const jsYaml = await import("js-yaml");
        def = jsYaml.load(content) as StepsDefinition;
      } catch (e) {
        if (
          e instanceof Error &&
          (e.message.includes("Cannot find module") ||
            e.message.includes("MODULE_NOT_FOUND"))
        ) {
          throw new Error(
            "js-yaml is required for YAML steps files. Install it: npm install js-yaml",
          );
        }
        throw e;
      }
    }

    return this.run(def, input);
  }

  /** Run steps from a parsed definition. */
  async run(
    def: StepsDefinition,
    input?: Record<string, unknown>,
  ): Promise<StepsResult> {
    const startTime = Date.now();
    const variables: Record<string, unknown> = {};
    const inputValues = input ?? {};
    const stepIndex = new Map<string, number>();

    // Build step index for goto support
    for (let i = 0; i < def.steps.length; i++) {
      stepIndex.set(def.steps[i].id, i);
    }

    this.log(`Running: ${def.name} (${def.steps.length} steps)`);

    let stepsCompleted = 0;
    let lastStep: string | undefined;
    let i = 0;

    try {
      while (i < def.steps.length) {
        const step = def.steps[i];
        lastStep = step.id;

        const result = await this.executeStepWithRetries(
          step,
          inputValues,
          variables,
          stepIndex,
        );

        if (result === "skip") {
          this.log(`  [${step.id}] skipped`);
          i++;
          continue;
        }

        if (typeof result === "string" && result.startsWith("goto:")) {
          const targetId = result.slice(5);
          const targetIdx = stepIndex.get(targetId);
          if (targetIdx === undefined) {
            throw new Error(`goto target not found: ${targetId}`);
          }
          this.log(`  [${step.id}] jumping to ${targetId}`);
          i = targetIdx;
          continue;
        }

        stepsCompleted++;
        i++;
      }

      this.log(
        `Completed: ${stepsCompleted}/${def.steps.length} steps in ${Date.now() - startTime}ms`,
      );

      return {
        status: "ok",
        steps_completed: stepsCompleted,
        steps_total: def.steps.length,
        duration_ms: Date.now() - startTime,
        last_step: lastStep,
        variables,
      };
    } catch (err) {
      const errorMsg =
        err instanceof Error ? err.message : String(err);
      this.log(`Failed at step [${lastStep}]: ${errorMsg}`);

      return {
        status: "error",
        steps_completed: stepsCompleted,
        steps_total: def.steps.length,
        duration_ms: Date.now() - startTime,
        last_step: lastStep,
        error: errorMsg,
        variables,
      };
    }
  }

  // -------------------------------------------------------------------------
  // Step execution with retries
  // -------------------------------------------------------------------------

  private async executeStepWithRetries(
    step: StepDefinition,
    input: Record<string, unknown>,
    variables: Record<string, unknown>,
    stepIndex: Map<string, number>,
  ): Promise<"skip" | string | void> {
    const maxAttempts = (step.retries ?? 0) + 1;
    const retryDelay = step.retry_delay ?? 1000;

    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      try {
        return await this.executeStep(step, input, variables);
      } catch (err) {
        const isElementNotFound =
          err instanceof ElementNotFoundError ||
          (err instanceof Error &&
            err.message.startsWith("Element not found"));

        // Handle on_missing for element-not-found errors
        if (isElementNotFound && step.on_missing) {
          if (step.on_missing === "skip") {
            return "skip";
          }
          if (step.on_missing.startsWith("goto:")) {
            return step.on_missing;
          }
          // "error" — fall through to throw
        }

        if (attempt < maxAttempts) {
          this.log(
            `  [${step.id}] attempt ${attempt} failed, retrying in ${retryDelay}ms...`,
          );
          await sleep(retryDelay);
        } else {
          throw err;
        }
      }
    }
  }

  // -------------------------------------------------------------------------
  // Single step execution
  // -------------------------------------------------------------------------

  private async executeStep(
    step: StepDefinition,
    input: Record<string, unknown>,
    variables: Record<string, unknown>,
  ): Promise<"skip" | void> {
    // 1. Wait
    if (step.wait != null && step.wait > 0) {
      this.log(`  [${step.id}] waiting ${step.wait}ms`);
      await sleep(step.wait);
    }

    // Pure sleep step (no action)
    if (!step.action && !step.selector && !step.expect && step.wait != null) {
      return;
    }

    // 2. Condition check
    if (step.condition) {
      const condSelector = substituteTemplates(
        step.condition,
        input,
        variables,
      );
      const found = await this.client.exists(condSelector, { timeout: 0 });
      if (!found) {
        return "skip";
      }
    }

    // 3. Template substitution
    const resolvedSelector = step.selector
      ? substituteTemplates(step.selector, input, variables)
      : undefined;

    const resolvedParams = step.params
      ? substituteInParams(step.params, input, variables)
      : undefined;

    const timeout = step.timeout ?? 3000;

    // 4. Resolve selector to element
    let element: FoundElement | undefined;
    if (resolvedSelector && step.action !== "expect" && step.action !== "scroll_until") {
      element = await this.resolveElement(resolvedSelector, timeout);
    }

    // 5. Execute action
    const action = step.action;
    let result: unknown;

    if (action) {
      this.log(`  [${step.id}] ${action}${resolvedSelector ? ` (${resolvedSelector})` : ""}`);
      result = await this.executeAction(
        action,
        element,
        resolvedSelector,
        resolvedParams,
        step,
        input,
        variables,
      );
    }

    // 6. Inline expect
    if (step.expect) {
      const expectSelector = substituteTemplates(
        step.expect,
        input,
        variables,
      );
      const expectTimeout = step.expect_timeout ?? 3000;
      const found = await this.client.exists(expectSelector, {
        timeout: expectTimeout,
      });
      if (!found) {
        throw new Error(
          `Expect failed: element not found: ${expectSelector}`,
        );
      }
      this.log(`  [${step.id}] expect passed: ${expectSelector}`);
    }

    // 7. Save result
    if (step.save_as && result !== undefined) {
      variables[step.save_as] = result;
      this.log(`  [${step.id}] saved to {{${step.save_as}}}`);
    }
  }

  // -------------------------------------------------------------------------
  // Action dispatch
  // -------------------------------------------------------------------------

  private async executeAction(
    action: string,
    element: FoundElement | undefined,
    selector: string | undefined,
    params: Record<string, unknown> | undefined,
    step: StepDefinition,
    input: Record<string, unknown>,
    variables: Record<string, unknown>,
  ): Promise<unknown> {
    switch (action) {
      case "click": {
        if (element) {
          await this.client.click(element.x, element.y);
        } else if (params?.x != null && params?.y != null) {
          await this.client.click(Number(params.x), Number(params.y));
        } else {
          throw new Error("click requires a selector or params.x/params.y");
        }
        return;
      }

      case "long_click": {
        if (element) {
          await this.client.longClick(element.x, element.y);
        } else if (params?.x != null && params?.y != null) {
          await this.client.longClick(Number(params.x), Number(params.y));
        } else {
          throw new Error(
            "long_click requires a selector or params.x/params.y",
          );
        }
        return;
      }

      case "type": {
        const text = params?.text;
        if (typeof text !== "string") {
          throw new Error("type requires params.text");
        }
        if (element) {
          // Click element to focus, then type
          await this.client.click(element.x, element.y);
          await sleep(300);
        }
        await this.client.type(text);
        return;
      }

      case "screenshot": {
        const result = await this.client.screenshot();
        return result;
      }

      case "scroll": {
        const direction = (params?.direction ?? step.direction ?? "down") as
          | "up"
          | "down"
          | "left"
          | "right";
        const amount =
          params?.amount != null
            ? Number(params.amount)
            : step.scroll_amount ?? undefined;
        await this.client.scroll(direction, amount);
        return;
      }

      case "scroll_until": {
        if (!selector) {
          throw new Error("scroll_until requires a selector");
        }
        const dir = (step.direction ?? "down") as
          | "up"
          | "down"
          | "left"
          | "right";
        const maxScrolls = step.max_scrolls ?? 10;
        const scrollAmount = step.scroll_amount ?? 500;

        for (let i = 0; i < maxScrolls; i++) {
          const found = await this.client.exists(selector, { timeout: 0 });
          if (found) {
            return;
          }
          await this.client.scroll(dir, scrollAmount);
          await sleep(300);
        }
        throw new ElementNotFoundError(
          `${selector} (after ${maxScrolls} scrolls)`,
        );
      }

      case "back":
        await this.client.back();
        return;

      case "home":
        await this.client.home();
        return;

      case "recents":
        await this.client.recents();
        return;

      case "select_all":
        await this.client.selectAll();
        return;

      case "copy": {
        const returnText = params?.return_text === true;
        const result = await this.client.copy({ returnText });
        return result;
      }

      case "paste": {
        const text = params?.text as string | undefined;
        await this.client.paste(text);
        return;
      }

      case "get_text": {
        const result = await this.client.getText();
        return result;
      }

      case "get_clipboard": {
        const result = await this.client.getClipboard();
        return result;
      }

      case "set_clipboard": {
        if (typeof params?.text !== "string") {
          throw new Error("set_clipboard requires params.text");
        }
        await this.client.setClipboard(params.text);
        return;
      }

      case "expect": {
        if (!selector) {
          throw new Error("expect requires a selector");
        }
        const timeout = step.timeout ?? 3000;
        const found = await this.client.exists(selector, { timeout });
        if (!found) {
          throw new ElementNotFoundError(selector);
        }
        return;
      }

      case "drag": {
        if (params?.startX != null && params?.startY != null && params?.endX != null && params?.endY != null) {
          await this.client.drag(
            Number(params.startX),
            Number(params.startY),
            Number(params.endX),
            Number(params.endY),
          );
        } else {
          throw new Error(
            "drag requires params.startX, params.startY, params.endX, params.endY",
          );
        }
        return;
      }

      case "hold_key": {
        if (typeof params?.key !== "string") {
          throw new Error("hold_key requires params.key");
        }
        await this.client.holdKey(params.key);
        return;
      }

      case "release_key": {
        if (typeof params?.key !== "string") {
          throw new Error("release_key requires params.key");
        }
        await this.client.releaseKey(params.key);
        return;
      }

      case "press_key": {
        if (typeof params?.key !== "string") {
          throw new Error("press_key requires params.key");
        }
        await this.client.pressKey(params.key);
        return;
      }

      case "ui_tree": {
        const result = await this.client.uiTree();
        return result;
      }

      case "camera": {
        const cameraId = params?.camera as string | undefined;
        const result = await this.client.camera(cameraId);
        return result;
      }

      case "list_cameras": {
        const result = await this.client.listCameras();
        return result;
      }

      default: {
        // Pass through to sendCommand for any unknown action
        const resp = await this.client.sendCommand(action, params);
        return resp.result;
      }
    }
  }

  // -------------------------------------------------------------------------
  // Element resolution
  // -------------------------------------------------------------------------

  private async resolveElement(
    selector: string,
    timeout: number,
  ): Promise<FoundElement> {
    const deadline = Date.now() + timeout;
    while (true) {
      const { tree } = await this.client.uiTree();
      const { findElements } = await import("./selector.js");
      const found = findElements(tree, selector);
      if (found.length > 0) return found[0];
      if (Date.now() >= deadline) {
        throw new ElementNotFoundError(selector);
      }
      await sleep(500);
    }
  }

  // -------------------------------------------------------------------------
  // Logging
  // -------------------------------------------------------------------------

  private log(message: string): void {
    if (this.verbose) {
      console.log(`[steps] ${message}`);
    }
  }
}
