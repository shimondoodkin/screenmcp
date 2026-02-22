"""Selector engine for finding UI elements by query."""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any


@dataclass
class FoundElement:
    """A UI element found by a selector query."""

    x: int
    y: int
    bounds: dict
    text: str | None = None
    class_name: str | None = None
    resource_id: str | None = None
    content_description: str | None = None
    node: dict = field(default_factory=dict)


# Maps selector prefix to (node field, operation)
_FIELD_MAP = {
    "text": "text",
    "role": "className",
    "desc": "contentDescription",
    "id": "resourceId",
}

# Regex for a single selector atom: optional !, prefix:value or prefix=value,
# optional property filters [prop], optional index [n]
_ATOM_RE = re.compile(
    r"(?P<neg>!)?"
    r"(?P<prefix>text|role|desc|id)"
    r"(?P<op>[:=])"
    r"(?P<value>[^\[]+?)"
    r"(?P<filters>(?:\[[a-zA-Z_]+\])*)"
    r"(?:\[(?P<index>-?\d+)\])?"
    r"$"
)


def parse_selector(selector: str) -> list[dict]:
    """Parse selector string into query structure (list of OR groups).

    Each OR group is a list of condition dicts (AND-ed together).
    Returns a list of OR groups.
    """
    or_groups = [g.strip() for g in selector.split("||")]
    parsed: list[dict] = []

    for group in or_groups:
        and_parts = [p.strip() for p in group.split("&&")]
        conditions: list[dict[str, Any]] = []
        group_index: int | None = None

        for part in and_parts:
            m = _ATOM_RE.match(part)
            if not m:
                raise ValueError(f"Invalid selector: {part!r}")

            prefix = m.group("prefix")
            op = "equals" if m.group("op") == "=" else "contains"
            value = m.group("value")
            negated = m.group("neg") == "!"

            node_field = _FIELD_MAP[prefix]

            # Parse property filters like [focused][clickable]
            prop_filters: list[str] = []
            if m.group("filters"):
                prop_filters = re.findall(r"\[([a-zA-Z_]+)\]", m.group("filters"))

            # Parse index
            if m.group("index") is not None:
                group_index = int(m.group("index"))

            conditions.append({
                "field": node_field,
                "value": value,
                "op": op,
                "negated": negated,
                "prop_filters": prop_filters,
                "is_role": prefix == "role",
            })

        parsed.append({
            "conditions": conditions,
            "index": group_index,
        })

    return parsed


def match_node(node: dict, query: dict) -> bool:
    """Check if a single node matches a parsed query group.

    ``query`` is one element from the list returned by ``parse_selector``.
    Only the conditions are checked here; index filtering is done externally.
    """
    for cond in query["conditions"]:
        node_field: str = cond["field"]
        target: str = cond["value"]
        op: str = cond["op"]
        negated: bool = cond["negated"]
        prop_filters: list[str] = cond["prop_filters"]
        is_role: bool = cond["is_role"]

        # Get the field value from the node.
        # For role selectors, also check 'title' (desktop nodes).
        node_value = node.get(node_field)
        if is_role and node_value is None:
            node_value = node.get("title")

        # Perform the match
        if op == "equals":
            matched = node_value is not None and str(node_value) == target
        else:  # contains, case-insensitive
            matched = (
                node_value is not None
                and target.lower() in str(node_value).lower()
            )

        if negated:
            matched = not matched

        if not matched:
            return False

        # Check property filters
        for prop in prop_filters:
            if not node.get(prop):
                return False

    return True


def _compute_bounds(node: dict) -> tuple[int, int, dict] | None:
    """Compute center coordinates and bounds dict from a node.

    Returns (center_x, center_y, bounds_dict) or None if no geometry.
    """
    bounds = node.get("bounds")
    if bounds and "left" in bounds:
        # Android-style bounds
        left = bounds["left"]
        top = bounds["top"]
        right = bounds["right"]
        bottom = bounds["bottom"]
        cx = (left + right) // 2
        cy = (top + bottom) // 2
        return cx, cy, {"left": left, "top": top, "right": right, "bottom": bottom}

    # Desktop-style bounds
    if "x" in node and "y" in node and "width" in node and "height" in node:
        x = node["x"]
        y = node["y"]
        w = node["width"]
        h = node["height"]
        cx = x + w // 2
        cy = y + h // 2
        return cx, cy, {"left": x, "top": y, "right": x + w, "bottom": y + h}

    return None


def _walk_tree(tree: list | dict, query: dict) -> list[FoundElement]:
    """Recursively walk the tree and collect matching elements."""
    results: list[FoundElement] = []

    if isinstance(tree, dict):
        nodes = [tree]
    elif isinstance(tree, list):
        nodes = tree
    else:
        return results

    for node in nodes:
        if match_node(node, query):
            geo = _compute_bounds(node)
            if geo is not None:
                cx, cy, bounds = geo
                results.append(FoundElement(
                    x=cx,
                    y=cy,
                    bounds=bounds,
                    text=node.get("text"),
                    class_name=node.get("className"),
                    resource_id=node.get("resourceId"),
                    content_description=node.get("contentDescription"),
                    node=node,
                ))

        # Recurse into children
        children = node.get("children")
        if children:
            results.extend(_walk_tree(children, query))

    return results


def find_elements(tree: list, selector: str) -> list[FoundElement]:
    """Walk tree recursively, return all matching nodes.

    Supports OR groups (``||``), AND conditions (``&&``),
    negation (``!``), property filters (``[focused]``),
    and index selection (``[0]``, ``[-1]``).
    """
    parsed = parse_selector(selector)
    all_results: list[FoundElement] = []

    for group in parsed:
        matches = _walk_tree(tree, group)
        idx = group.get("index")
        if idx is not None and matches:
            try:
                matches = [matches[idx]]
            except IndexError:
                matches = []
        all_results.extend(matches)

    return all_results


class ElementHandle:
    """Fluent handle for interacting with a found element."""

    def __init__(
        self, client: Any, selector: str, timeout: float = 3.0
    ) -> None:
        self._client = client
        self._selector = selector
        self._timeout = timeout

    async def _resolve(self) -> FoundElement:
        """Poll ui_tree until element is found or timeout."""
        import asyncio
        import time

        deadline = time.monotonic() + self._timeout
        while True:
            result = await self._client.ui_tree()
            tree = result.get("tree", [])
            found = find_elements(tree, self._selector)
            if found:
                return found[0]
            if time.monotonic() >= deadline:
                from .client import ScreenMCPError

                raise ScreenMCPError(f"Element not found: {self._selector}")
            await asyncio.sleep(0.5)

    async def click(self) -> dict[str, Any]:
        """Click the found element."""
        el = await self._resolve()
        return await self._client.click(el.x, el.y)

    async def long_click(self) -> dict[str, Any]:
        """Long-click the found element."""
        el = await self._resolve()
        return await self._client.long_click(el.x, el.y)

    async def type(self, text: str) -> dict[str, Any]:
        """Click the element to focus it, then type text."""
        import asyncio

        el = await self._resolve()
        await self._client.click(el.x, el.y)
        await asyncio.sleep(0.3)
        return await self._client.type_text(text)

    async def element(self) -> FoundElement:
        """Resolve and return the FoundElement."""
        return await self._resolve()
