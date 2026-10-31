// Independent development oracle; never part of the Rust library or CLI runtime.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const parser = require('jsonc-parser');
const binary = path.resolve(process.argv[2] || 'target/debug/jsonc-merge');
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'jsonc-cross-'));
let cases = 0;
function run(base, ours, theirs, expected, policy) {
  for (const [name, text] of Object.entries({ base, ours, theirs })) fs.writeFileSync(path.join(temp, name), text);
  const args = ['merge', 'base', 'ours', 'theirs'];
  if (policy) { fs.writeFileSync(path.join(temp, 'policy'), JSON.stringify(policy)); args.push('--policy', 'policy'); }
  const result = spawnSync(binary, args, { cwd: temp, encoding: 'utf8' });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  const errors = [];
  const actual = parser.parse(result.stdout, errors, { allowTrailingComma: true, disallowComments: false });
  assert.deepEqual(errors, [], result.stdout);
  assert.deepEqual(actual, expected, result.stdout);
  cases++;
  return result.stdout;
}
function comments(text) {
  const tokens = [];
  parser.visit(text, { onComment: (offset, length) => tokens.push(text.slice(offset, offset + length)) }, { allowTrailingComma: true });
  return tokens.sort();
}
let seed = 0x78929111;
function next() { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed; }
function value(depth) {
  if (!depth) return next() % 7;
  switch (next() % 4) {
    case 0: return [value(depth - 1), value(depth - 1)];
    case 1: {
      const result = {};
      for (const key of ['a', 'b', '雪']) if (next() % 3) result[key] = value(depth - 1);
      return result;
    }
    default: return ['雪', '/* literal */', 'line\nbreak'][next() % 3];
  }
}
function source(v, label = 'root') {
  if (v === null || typeof v !== 'object') return JSON.stringify(v);
  const array = Array.isArray(v);
  const newline = ['\n', '\r\n', '\r'][next() % 3];
  const entries = Object.entries(v).map(([key, child]) => {
    const mark = `${label}.${key}`;
    const prefix = array ? '' : `${JSON.stringify(key)} /* internal:${mark} */ : `;
    return `// leading:${mark}${newline}${prefix}${source(child, mark)}, // trailing:${mark}${newline}`;
  }).join('');
  return `${array ? '[' : '{'}${newline}${entries}/* closing:${label} */${newline}${array ? ']' : '}'}`;
}
try {
  for (let i = 0; i < 128; i++) {
    const base = `// header\n{\n "a":${i}, // a\n "b":"雪",\n}\n// footer\n`;
    const ours = base.replace(`"a":${i}`, `"a":${i + 1}`);
    const theirs = base.replace('"b":"雪"', '"b":"雲"');
    run(base, ours, theirs, { a: i + 1, b: '雲' });
  }
  const policy = { version: 1, arrays: [{ path: '/items', key: 'name', order: 'ordered' }] };
  run('{"items":[{"name":"A","x":1},{"name":"B"}]}',
      '{"items":[\n/* B */{"name":"B"},\n/* A */{"name":"A","x":1}]}',
      '{"items":[{"name":"A","x":2},{"name":"B"}]}',
      { items: [{ name: 'B' }, { name: 'A', x: 2 }] }, policy);
  // The previous case changes comments on both records only in ours.
  run('{"a":1 // keep\n}', '{"a":1 // keep\n}', '{"a":1, // keep\n"b":2}', { a: 1, b: 2 });
  const root = path.resolve(__dirname, '..');
  run(...['base.jsonc', 'ours.jsonc', 'theirs.jsonc'].map(name => fs.readFileSync(path.join(root, 'examples/launch', name), 'utf8')),
      JSON.parse(fs.readFileSync(path.join(root, 'examples/launch/expected.json'), 'utf8')),
      JSON.parse(fs.readFileSync(path.join(root, 'examples/launch/policy.json'), 'utf8')));
  // An independent lexer verifies comment token preservation, including strings
  // containing comment-looking text and all supported line-ending conventions.
  for (let i = 0; i < 128; i++) {
    const base = source({ data: value(3) });
    const theirsValue = { data: value(3) };
    const theirs = source(theirsValue);
    const output = run(base, base, theirs, theirsValue);
    assert.deepEqual(comments(output), comments(theirs), `comment loss in generated case ${i}`);
  }
  console.log(`Independent Microsoft JSONC parser: ${cases} merge outputs verified.`);
} finally { fs.rmSync(temp, { recursive: true, force: true }); }
