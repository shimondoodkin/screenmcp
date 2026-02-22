// ---------------------------------------------------------------------------
// Selector engine: parse selector strings and match against UI tree nodes
// ---------------------------------------------------------------------------

export type SelectorOp = "contains" | "equals";

export interface SelectorAtom {
  field: string; // 'text', 'className', 'contentDescription', 'resourceId'
  value: string;
  op: SelectorOp;
  negated: boolean;
}

export interface PropertyFilter {
  prop: string; // 'focused', 'clickable', 'checked', etc.
}

export interface ParsedSelector {
  atoms: SelectorAtom[];
  properties: PropertyFilter[];
  index?: number;
}

/** Top-level: OR groups */
export type SelectorQuery = ParsedSelector[];

export interface FoundElement {
  x: number;
  y: number;
  bounds: { left: number; top: number; right: number; bottom: number };
  text?: string;
  className?: string;
  resourceId?: string;
  contentDescription?: string;
  node: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

const FIELD_MAP: Record<string, string> = {
  text: "text",
  role: "className",
  desc: "contentDescription",
  id: "resourceId",
};

function parseAtom(raw: string): {
  atom: SelectorAtom;
  rest: string;
} {
  const negated = raw.startsWith("!");
  const s = negated ? raw.slice(1) : raw;

  // Try field=value (equals)
  const eqIdx = s.indexOf("=");
  const colonIdx = s.indexOf(":");

  let field: string;
  let value: string;
  let op: SelectorOp;
  let consumed: number;

  if (eqIdx !== -1 && (colonIdx === -1 || eqIdx < colonIdx)) {
    const key = s.slice(0, eqIdx);
    field = FIELD_MAP[key] ?? key;
    op = "equals";
    // Value goes until && or || or [ or end
    const rest = s.slice(eqIdx + 1);
    const endIdx = findValueEnd(rest);
    value = rest.slice(0, endIdx);
    consumed = (negated ? 1 : 0) + eqIdx + 1 + endIdx;
  } else if (colonIdx !== -1) {
    const key = s.slice(0, colonIdx);
    field = FIELD_MAP[key] ?? key;
    op = "contains";
    const rest = s.slice(colonIdx + 1);
    const endIdx = findValueEnd(rest);
    value = rest.slice(0, endIdx);
    consumed = (negated ? 1 : 0) + colonIdx + 1 + endIdx;
  } else {
    throw new Error(`Invalid selector atom: ${raw}`);
  }

  return {
    atom: { field, value, op, negated },
    rest: raw.slice(consumed),
  };
}

/** Find where the value ends: at &&, ||, [, or end of string */
function findValueEnd(s: string): number {
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "[") return i;
    if (s[i] === "&" && s[i + 1] === "&") return i;
    if (s[i] === "|" && s[i + 1] === "|") return i;
  }
  return s.length;
}

function parseBrackets(s: string): {
  properties: PropertyFilter[];
  index?: number;
  rest: string;
} {
  const properties: PropertyFilter[] = [];
  let index: number | undefined;
  let rest = s;

  while (rest.startsWith("[")) {
    const close = rest.indexOf("]");
    if (close === -1) throw new Error(`Unclosed bracket in selector: ${s}`);
    const inner = rest.slice(1, close).trim();
    rest = rest.slice(close + 1);

    // Check if it's a numeric index
    const num = Number(inner);
    if (!isNaN(num) && inner !== "") {
      index = num;
    } else {
      properties.push({ prop: inner });
    }
  }

  return { properties, index, rest };
}

function parseGroup(s: string): ParsedSelector {
  const atoms: SelectorAtom[] = [];
  let remaining = s.trim();

  while (remaining.length > 0) {
    // Skip whitespace around &&
    remaining = remaining.replace(/^\s*&&\s*/, "");
    if (remaining.length === 0) break;

    // If starts with [, parse brackets
    if (remaining.startsWith("[")) {
      const { properties, index, rest } = parseBrackets(remaining);
      const result: ParsedSelector = { atoms, properties, index };
      // There might be more after brackets
      if (rest.trim().length > 0) {
        const continued = parseGroup(rest.trim());
        result.atoms.push(...continued.atoms);
        result.properties.push(...continued.properties);
        if (continued.index !== undefined) result.index = continued.index;
      }
      return result;
    }

    const { atom, rest } = parseAtom(remaining);
    atoms.push(atom);
    remaining = rest;

    // Parse any brackets attached to this atom
    if (remaining.startsWith("[")) {
      const { properties, index, rest: afterBrackets } =
        parseBrackets(remaining);
      remaining = afterBrackets;
      // If there's more with &&, continue
      if (remaining.startsWith("&&")) {
        remaining = remaining.slice(2).trim();
        const continued = parseGroup(remaining);
        return {
          atoms: [...atoms, ...continued.atoms],
          properties: [...properties, ...continued.properties],
          index: index ?? continued.index,
        };
      }
      return { atoms, properties, index };
    }
  }

  return { atoms, properties: [] };
}

/** Parse a selector string into a query (array of OR groups). */
export function parseSelector(selector: string): SelectorQuery {
  const groups = selector.split("||").map((s) => s.trim());
  return groups.map(parseGroup);
}

// ---------------------------------------------------------------------------
// Matcher
// ---------------------------------------------------------------------------

function getNodeField(
  node: Record<string, unknown>,
  field: string,
): string | undefined {
  // Direct field lookup
  if (field in node && node[field] != null) {
    return String(node[field]);
  }
  // For 'className', also try 'title' (desktop nodes)
  if (field === "className" && "title" in node && node.title != null) {
    return String(node.title);
  }
  return undefined;
}

/** Test whether a single node matches a parsed selector group. */
export function matchNode(
  node: Record<string, unknown>,
  query: ParsedSelector,
): boolean {
  // Check all atoms (ANDed)
  for (const atom of query.atoms) {
    const fieldVal = getNodeField(node, atom.field);
    let matches: boolean;

    if (fieldVal === undefined) {
      matches = false;
    } else if (atom.op === "equals") {
      matches = fieldVal === atom.value;
    } else {
      // contains, case-insensitive
      matches = fieldVal.toLowerCase().includes(atom.value.toLowerCase());
    }

    if (atom.negated) matches = !matches;
    if (!matches) return false;
  }

  // Check property filters (ANDed)
  for (const pf of query.properties) {
    if (!node[pf.prop]) return false;
  }

  return true;
}

function extractBounds(node: Record<string, unknown>): FoundElement | null {
  // Android format: bounds object
  const bounds = node.bounds as
    | { left: number; top: number; right: number; bottom: number }
    | undefined;
  if (bounds && typeof bounds.left === "number") {
    return {
      x: Math.round((bounds.left + bounds.right) / 2),
      y: Math.round((bounds.top + bounds.bottom) / 2),
      bounds,
      text: node.text as string | undefined,
      className: node.className as string | undefined,
      resourceId: node.resourceId as string | undefined,
      contentDescription: node.contentDescription as string | undefined,
      node,
    };
  }

  // Desktop format: x, y, width, height
  if (
    typeof node.x === "number" &&
    typeof node.y === "number" &&
    typeof node.width === "number" &&
    typeof node.height === "number"
  ) {
    const left = node.x as number;
    const top = node.y as number;
    const width = node.width as number;
    const height = node.height as number;
    return {
      x: Math.round(left + width / 2),
      y: Math.round(top + height / 2),
      bounds: { left, top, right: left + width, bottom: top + height },
      text: node.text as string | undefined,
      className: (node.className ?? node.title) as string | undefined,
      resourceId: node.resourceId as string | undefined,
      contentDescription: node.contentDescription as string | undefined,
      node,
    };
  }

  return null;
}

function collectMatches(
  nodes: unknown[],
  query: ParsedSelector,
  results: FoundElement[],
): void {
  for (const raw of nodes) {
    if (typeof raw !== "object" || raw === null) continue;
    const node = raw as Record<string, unknown>;

    if (matchNode(node, query)) {
      const el = extractBounds(node);
      if (el) results.push(el);
    }

    // Recurse into children
    const children = node.children as unknown[] | undefined;
    if (Array.isArray(children)) {
      collectMatches(children, query, results);
    }
  }
}

/** Walk the tree and return all matching elements. */
export function findElements(
  tree: unknown[],
  selector: string,
): FoundElement[] {
  const query = parseSelector(selector);

  // Collect matches for each OR group, then merge
  const allMatches: FoundElement[] = [];
  const seen = new Set<Record<string, unknown>>();

  for (const group of query) {
    const groupMatches: FoundElement[] = [];
    collectMatches(tree, group, groupMatches);

    // Apply index if present
    if (group.index !== undefined) {
      const idx =
        group.index >= 0
          ? group.index
          : groupMatches.length + group.index;
      if (idx >= 0 && idx < groupMatches.length) {
        const el = groupMatches[idx];
        if (!seen.has(el.node)) {
          seen.add(el.node);
          allMatches.push(el);
        }
      }
    } else {
      for (const el of groupMatches) {
        if (!seen.has(el.node)) {
          seen.add(el.node);
          allMatches.push(el);
        }
      }
    }
  }

  return allMatches;
}

// ---------------------------------------------------------------------------
// ElementHandle — fluent API for interacting with found elements
// ---------------------------------------------------------------------------

import type { ScreenMCPClient } from "./client.js";

export class ElementHandle {
  constructor(
    private client: ScreenMCPClient,
    private selector: string,
    private timeout: number = 3000,
  ) {}

  private async resolve(): Promise<FoundElement> {
    const deadline = Date.now() + this.timeout;
    while (true) {
      const { tree } = await this.client.uiTree();
      const found = findElements(tree, this.selector);
      if (found.length > 0) return found[0];
      if (Date.now() >= deadline) {
        throw new Error(`Element not found: ${this.selector}`);
      }
      await new Promise((r) => setTimeout(r, 500));
    }
  }

  async click(): Promise<void> {
    const el = await this.resolve();
    await this.client.click(el.x, el.y);
  }

  async longClick(): Promise<void> {
    const el = await this.resolve();
    await this.client.longClick(el.x, el.y);
  }

  async type(text: string): Promise<void> {
    const el = await this.resolve();
    await this.client.click(el.x, el.y);
    await new Promise((r) => setTimeout(r, 300));
    await this.client.type(text);
  }

  async element(): Promise<FoundElement> {
    return this.resolve();
  }
}
