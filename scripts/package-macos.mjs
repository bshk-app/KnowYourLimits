import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { notaryOptions, readReleaseInfo, root, verifyManifest } from './release-common.mjs';

const config = JSON.parse(readFileSync(join(root, 'packaging/macos.json'), 'utf8'));
const identity = process.env.KYL_SIGNING_IDENTITY_SHA1 || config.signingIdentitySha1;
const args = process.argv.slice(2);
if (args.length > 1 || (args.length === 1 && args[0] !== '--skip-build')) {
  throw new Error('Usage: bun run package:macos [--skip-build]');
}
if (process.platform !== 'darwin' || process.arch !== 'arm64') {
  throw new Error('Package the supported Apple Silicon build on an ARM64 Mac.');
}
if (!/^[A-Fa-f0-9]{40}$/.test(identity)) {
  throw new Error('Set KYL_SIGNING_IDENTITY_SHA1 to the Developer ID SHA1.');
}
const info = readReleaseInfo();
const notary = notaryOptions(config);
execFileSync('zamokctl', ['--version'], { cwd: root, stdio: 'inherit' });
execFileSync('xcrun', ['notarytool', 'history', ...notary.verify, '--output-format', 'json'], {
  cwd: root,
  stdio: ['ignore', 'ignore', 'inherit'],
});
if (!args.includes('--skip-build')) {
  execFileSync('bun', ['run', 'tauri', 'build'], { cwd: root, stdio: 'inherit' });
}
execFileSync(
  'zamokctl',
  [
    'package',
    '--input',
    join(root, 'src-tauri/target/release/bundle/macos', `${info.name}.app`),
    '--output-dir',
    info.output,
    '--signing-identity-sha1',
    identity,
    ...notary.package,
    '--format',
    'dmg',
  ],
  { cwd: root, stdio: 'inherit' },
);
const manifest = verifyManifest(info);
const stagedApp = join(info.output, 'staging', `${info.name}.app`);
for (const artifact of [stagedApp, manifest.artifact]) {
  execFileSync('xcrun', ['stapler', 'validate', artifact], { cwd: root, stdio: 'inherit' });
}
execFileSync('codesign', ['--verify', '--deep', '--strict', stagedApp], {
  cwd: root,
  stdio: 'inherit',
});
execFileSync('spctl', ['--assess', '--type', 'execute', stagedApp], {
  cwd: root,
  stdio: 'inherit',
});
console.log(`Verified notarized DMG: ${manifest.artifact}`);
