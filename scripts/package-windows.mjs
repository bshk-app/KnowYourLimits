import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { readReleaseInfo, root } from './release-common.mjs';

if (process.platform !== 'win32') throw new Error('Windows packaging requires Windows.');
if (process.argv.length !== 2) throw new Error('Usage: bun run package:windows');
const info = readReleaseInfo();
execFileSync('bun', ['run', 'tauri', 'build', '--bundles', 'nsis'], {
  cwd: root,
  stdio: 'inherit',
});
const release = join(root, 'src-tauri/target/release');
const bundles = join(release, 'bundle/nsis');
const installers = readdirSync(bundles).filter(
  (name) => name.startsWith(`${info.name}_${info.version}_`) && name.endsWith('-setup.exe'),
);
if (installers.length !== 1) throw new Error('Expected exactly one Windows NSIS installer.');
const output = join(root, 'artifacts/windows', info.version);
mkdirSync(output, { recursive: true });
const files = [
  [join(release, 'knowyourlimits.exe'), `${info.name}-${info.version}-windows-x64.exe`],
  [join(bundles, installers[0]), `${info.name}-${info.version}-windows-x64-setup.exe`],
].map(([source, name]) => {
  copyFileSync(source, join(output, name));
  const bytes = readFileSync(join(output, name));
  return {
    name,
    sizeBytes: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
});
writeFileSync(
  join(output, 'manifest.json'),
  `${JSON.stringify({ version: info.version, platform: 'windows', arch: 'x64', signed: false, files }, null, 2)}\n`,
);
console.log(`Windows installer and executable: ${output}`);
