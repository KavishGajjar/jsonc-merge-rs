// Informational measurements, never timing-based pass/fail assertions.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const binary = path.resolve(process.argv[2]);
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'jsonc-measure-'));
const samples = [];
function measure(name, b, o, t, policy) {
  const input = { b, o, t };
  for (const [file, text] of Object.entries(input)) fs.writeFileSync(path.join(temp, file), text);
  const args = ['merge', 'b', 'o', 't'];
  if (policy) { fs.writeFileSync(path.join(temp, 'policy'), JSON.stringify(policy)); args.push('--policy', 'policy'); }
  const start = process.hrtime.bigint();
  const result = spawnSync(binary, args, { cwd: temp, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  const elapsedMs = Number(process.hrtime.bigint() - start) / 1e6;
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  samples.push({ name, inputBytes: b.length + o.length + t.length, outputBytes: result.stdout.length, elapsedMs });
}
try {
  const flat = Object.fromEntries(Array.from({ length: 5000 }, (_, i) => [`key_${i}`, i]));
  measure('5000 object members', JSON.stringify(flat), JSON.stringify({ ...flat, key_0: -1 }), JSON.stringify({ ...flat, key_4999: -2 }));
  const records = Array.from({ length: 1414 }, (_, i) => ({ id: `r${String(i).padStart(4, '0')}`, value: 0 }));
  const ours = records.slice(); [ours[1412], ours[1413]] = [ours[1413], ours[1412]];
  const theirs = records.map((r, i) => i === 0 ? { ...r, value: 1 } : r);
  measure('998991 ordered pairs', JSON.stringify({ items: records }), JSON.stringify({ items: ours }), JSON.stringify({ items: theirs }),
    { version: 1, arrays: [{ path: '/items', key: 'id', order: 'ordered' }] });
  const unordered = Array.from({ length: 10000 }, (_, i) => ({ id: `r${String(i).padStart(5, '0')}`, value: 0 }));
  measure('10000 unordered records', JSON.stringify({ items: unordered }), JSON.stringify({ items: unordered.slice().reverse() }),
    JSON.stringify({ items: unordered.map((r, i) => i === 0 ? { ...r, value: 1 } : r) }),
    { version: 1, arrays: [{ path: '/items', key: 'id', order: 'unordered' }] });
  const maxText = '"' + 'x'.repeat(8 * 1024 * 1024 - 2) + '"';
  measure('8 MiB input boundary (three equal snapshots)', maxText, maxText, maxText);
  console.log(JSON.stringify({ schemaVersion: 1, platform: process.platform, architecture: process.arch, measurements: samples }, null, 2));
} finally { fs.rmSync(temp, { recursive: true, force: true }); }
