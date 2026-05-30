"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const node_test_1 = require("node:test");
const strict_1 = __importDefault(require("node:assert/strict"));
const model_js_1 = require("./model.js");
(0, node_test_1.test)('resolveModel accepts known providers, rejects everything else', () => {
    strict_1.default.equal((0, model_js_1.resolveModel)('claude'), 'claude');
    strict_1.default.equal((0, model_js_1.resolveModel)('gemini'), 'gemini');
    strict_1.default.equal((0, model_js_1.resolveModel)('chatgpt'), 'chatgpt');
    strict_1.default.equal((0, model_js_1.resolveModel)('gpt-5'), null);
    strict_1.default.equal((0, model_js_1.resolveModel)(null), null);
    strict_1.default.equal((0, model_js_1.resolveModel)(''), null);
});
(0, node_test_1.test)('COORD_TOOLS covers screenshot family and pointer commands', () => {
    for (const name of ['screenshot', 'screenshot_region', 'screenshot_window', 'ui_tree',
        'get_screen_size',
        'click', 'long_click', 'drag', 'scroll', 'double_click',
        'right_click', 'middle_click', 'mouse_move', 'mouse_scroll']) {
        strict_1.default.ok(model_js_1.COORD_TOOLS.has(name), `${name} should be in COORD_TOOLS`);
    }
    strict_1.default.equal(model_js_1.COORD_TOOLS.has('type'), false);
});
(0, node_test_1.test)('applyModelDefault injects model only for coord tools without explicit size', () => {
    strict_1.default.deepEqual((0, model_js_1.applyModelDefault)('click', { x: 1 }, 'gemini'), { x: 1, model: 'gemini' });
    // explicit size present → leave untouched
    strict_1.default.deepEqual((0, model_js_1.applyModelDefault)('screenshot', { max_width: 800 }, 'gemini'), { max_width: 800 });
    strict_1.default.deepEqual((0, model_js_1.applyModelDefault)('screenshot', { max_height: 600 }, 'gemini'), { max_height: 600 });
    // non-coord tool → untouched
    strict_1.default.deepEqual((0, model_js_1.applyModelDefault)('type', { text: 'hi' }, 'gemini'), { text: 'hi' });
    // no model → untouched
    strict_1.default.deepEqual((0, model_js_1.applyModelDefault)('click', { x: 1 }, null), { x: 1 });
});
(0, node_test_1.test)('model is read from a URL query string', () => {
    const u = new URL('http://localhost:3000/api/mcp?model=chatgpt');
    strict_1.default.equal((0, model_js_1.resolveModel)(u.searchParams.get('model')), 'chatgpt');
    const u2 = new URL('http://localhost:3000/api/mcp');
    strict_1.default.equal((0, model_js_1.resolveModel)(u2.searchParams.get('model')), null);
});
