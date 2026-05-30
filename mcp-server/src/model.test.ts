import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resolveModel, applyModelDefault, COORD_TOOLS } from './model.js';

test('resolveModel accepts known providers, rejects everything else', () => {
  assert.equal(resolveModel('claude'), 'claude');
  assert.equal(resolveModel('gemini'), 'gemini');
  assert.equal(resolveModel('chatgpt'), 'chatgpt');
  assert.equal(resolveModel('gpt-5'), null);
  assert.equal(resolveModel(null), null);
  assert.equal(resolveModel(''), null);
});

test('COORD_TOOLS covers screenshot family and pointer commands', () => {
  for (const name of ['screenshot', 'screenshot_region', 'screenshot_window', 'ui_tree',
                      'get_screen_size',
                      'click', 'long_click', 'drag', 'scroll', 'double_click',
                      'right_click', 'middle_click', 'mouse_move', 'mouse_scroll']) {
    assert.ok(COORD_TOOLS.has(name), `${name} should be in COORD_TOOLS`);
  }
  assert.equal(COORD_TOOLS.has('type'), false);
});

test('applyModelDefault injects model only for coord tools without explicit size', () => {
  assert.deepEqual(applyModelDefault('click', { x: 1 }, 'gemini'), { x: 1, model: 'gemini' });
  // explicit size present → leave untouched
  assert.deepEqual(applyModelDefault('screenshot', { max_width: 800 }, 'gemini'), { max_width: 800 });
  assert.deepEqual(applyModelDefault('screenshot', { max_height: 600 }, 'gemini'), { max_height: 600 });
  // non-coord tool → untouched
  assert.deepEqual(applyModelDefault('type', { text: 'hi' }, 'gemini'), { text: 'hi' });
  // no model → untouched
  assert.deepEqual(applyModelDefault('click', { x: 1 }, null), { x: 1 });
});

test('model is read from a URL query string', () => {
  const u = new URL('http://localhost:3000/api/mcp?model=chatgpt');
  assert.equal(resolveModel(u.searchParams.get('model')), 'chatgpt');
  const u2 = new URL('http://localhost:3000/api/mcp');
  assert.equal(resolveModel(u2.searchParams.get('model')), null);
});
