// Cross-platform test runner. cmd.exe doesn't expand `test/*.mjs`, so we
// can't put a glob in package.json. Discover the .mjs files here and
// hand them to `node --test` directly.

'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');

const dir = __dirname;
const files = fs
  .readdirSync(dir)
  .filter((f) => f.endsWith('.mjs'))
  .sort()
  .map((f) => path.join(dir, f));

if (files.length === 0) {
  console.error('no test files found in', dir);
  process.exit(1);
}

spawn(process.execPath, ['--test', '--test-force-exit', ...files], {
  stdio: 'inherit',
}).on('exit', (code) => process.exit(code ?? 1));
