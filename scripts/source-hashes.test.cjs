const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'jsonc-hashes-'));
try {
  for (const name of ['.git', 'target', 'node_modules', 'custom-build', 'evidence', 'src']) fs.mkdirSync(path.join(root, name));
  fs.writeFileSync(path.join(root, 'src', 'a file.txt'), '雪');
  const env = { ...process.env, CARGO_TARGET_DIR: path.join(root, 'custom-build'), EVIDENCE_DIR: path.join(root, 'evidence') };
  const run = () => {
    const output = spawnSync(process.execPath, [path.join(__dirname, 'source-hashes.cjs'), root], { encoding: 'utf8', env });
    assert.ifError(output.error); assert.equal(output.status, 0, output.stderr);
    return output.stdout;
  };
  const before = run();
  assert.match(before, /^[a-f0-9]{64}  \.\/src\/a file\.txt\n$/);
  for (const name of ['.git', 'target', 'node_modules', 'custom-build', 'evidence']) fs.writeFileSync(path.join(root, name, 'ignored'), 'output');
  assert.equal(run(), before);
  fs.writeFileSync(path.join(root, 'src', 'a file.txt'), '雲');
  assert.notEqual(run(), before);
  console.log('Portable source-manifest regression checks passed.');
} finally { fs.rmSync(root, { recursive: true, force: true }); }
