// Dependency inventory and verbatim license texts from the locked Cargo sources.
// --write is an explicit maintenance operation; --check never changes the checkout.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const write = process.argv.includes('--write');
const args = ['metadata', '--format-version', '1', '--locked', '--offline'];
if (process.env.CARGO_VENDOR_CONFIG) args.push('--config', process.env.CARGO_VENDOR_CONFIG);
const result = spawnSync('cargo', args, { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
const packages = JSON.parse(result.stdout).packages.filter(p => p.source).sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
// An explicit inventory review guard, not an SPDX expression evaluator.
const reviewed = new Set([
  'MIT', 'MIT OR Apache-2.0', 'Apache-2.0 OR MIT', 'Unlicense OR MIT',
  'Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT',
  'MIT OR Apache-2.0 OR LGPL-2.1-or-later', 'Apache-2.0 OR BSL-1.0',
  '(MIT OR Apache-2.0) AND Unicode-3.0'
]);
const outputs = new Map();
let table = '# Third-party notices\n\n';
table += 'Original project code is MIT licensed. Dependencies retain their own licenses.\n';
table += 'This inventory comes from the locked Cargo metadata, including target-specific\n';
table += 'and build dependencies. Verbatim upstream license/notice files are retained in\n';
table += '`third-party-licenses/`; an OR expression offers alternatives, while AND requires\n';
table += 'both terms. No dependency is relicensed by this project.\n\n';
table += '| Crate | Locked version | Upstream license expression |\n| --- | --- | --- |\n';
for (const p of packages) {
  assert(reviewed.has(p.license), `Review the new license expression for ${p.name}: ${p.license}`);
  const dir = path.dirname(p.manifest_path);
  const names = fs.readdirSync(dir).filter(n => /^(licen[cs]e|copying|copyright|notice|authors)([._-]|$)/i.test(n) && fs.statSync(path.join(dir, n)).isFile());
  assert(names.length > 0, `No upstream license file found for ${p.name}`);
  const folder = `${p.name}-${p.version}`;
  for (const name of names) outputs.set(path.join('third-party-licenses', folder, name), fs.readFileSync(path.join(dir, name)));
  table += `| [${p.name}](https://crates.io/crates/${p.name}/${p.version}) | ${p.version} | ${p.license} |\n`;
}
const oracleLicense = require.resolve('jsonc-parser/package.json');
outputs.set('third-party-licenses/microsoft-jsonc-parser-3.3.1/LICENSE.md', fs.readFileSync(path.join(path.dirname(oracleLicense), 'LICENSE.md')));
table += '\nThe independent development oracle also uses Microsoft `jsonc-parser` 3.3.1\n';
table += '(MIT), whose license is included. It is not a Rust runtime dependency.\n\n';
table += 'Regenerate deliberately with `node scripts/notices.cjs --write` after a locked\n';
table += 'dependency update; `--check` verifies the inventory and retained texts offline.\n';
outputs.set('THIRD_PARTY.md', Buffer.from(table));
for (const [name, content] of outputs) {
  const destination = path.join(root, name);
  if (write) { fs.mkdirSync(path.dirname(destination), { recursive: true }); fs.writeFileSync(destination, content); }
  else { assert(fs.existsSync(destination), `Missing ${name}`); assert(fs.readFileSync(destination).equals(content), `Outdated ${name}`); }
}
console.log(`Dependency notices ${write ? 'generated' : 'verified'}: ${packages.length} Rust packages and one development oracle.`);
