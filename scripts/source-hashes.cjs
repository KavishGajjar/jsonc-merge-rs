// Portable source manifests: no dependency on GNU find/sort/sha256sum.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const root = path.resolve(process.argv[2] || path.join(__dirname, '..'));
const outputs = [process.env.CARGO_TARGET_DIR, process.env.EVIDENCE_DIR].filter(Boolean).map(p => path.resolve(root, p));
if (outputs.includes(root)) throw new Error('Build and evidence directories must not be the source root.');
const files = [];
function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (['.git', 'target', 'node_modules'].includes(entry.name) || outputs.includes(file)) continue;
      walk(file);
    } else if (entry.isFile() || entry.isSymbolicLink()) files.push(file);
  }
}
walk(root);
files.sort((a, b) => Buffer.compare(Buffer.from(path.relative(root, a)), Buffer.from(path.relative(root, b))));
for (const file of files) {
  const content = fs.lstatSync(file).isSymbolicLink() ? Buffer.from(`symlink:${fs.readlinkSync(file)}`) : fs.readFileSync(file);
  const digest = crypto.createHash('sha256').update(content).digest('hex');
  const relative = path.relative(root, file).split(path.sep).join('/');
  console.log(`${digest}  ./${relative}`);
}
