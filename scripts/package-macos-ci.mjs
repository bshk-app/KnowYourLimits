import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { notaryOptions, readReleaseInfo, root, verifyMacOSManifest } from './release-common.mjs';

if (process.platform !== 'darwin' || process.arch !== 'arm64') {
  throw new Error('macOS release packaging requires an Apple Silicon Mac.');
}
if (process.argv.length !== 2) throw new Error('Usage: bun run package:macos:ci');
const identity = process.env.APPLE_SIGNING_IDENTITY;
if (!/^[A-Fa-f0-9]{40}$/.test(identity || '')) {
  throw new Error('Set APPLE_SIGNING_IDENTITY to the imported Developer ID SHA1.');
}
for (const name of ['APPLE_API_KEY_PATH', 'APPLE_API_KEY', 'APPLE_API_ISSUER']) {
  if (!process.env[name]) throw new Error(`Set ${name} for App Store Connect notarization.`);
}
const notary = notaryOptions({});
const info = readReleaseInfo();
const run = (command, args) => execFileSync(command, args, { cwd: root, stdio: 'inherit' });
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
const sourceCommit = git('rev-parse', 'HEAD');
const assertClean = () => {
  if (git('status', '--porcelain') || git('rev-parse', 'HEAD') !== sourceCommit) {
    throw new Error('Package the macOS release from an unchanged, clean source commit.');
  }
};
assertClean();

// Tauri signs the app with hardened runtime, notarizes it, and staples its ticket.
run('bun', ['run', 'tauri', 'build', '--bundles', 'app']);
const app = join(root, 'src-tauri/target/release/bundle/macos', `${info.name}.app`);
run('codesign', ['--verify', '--deep', '--strict', app]);
run('xcrun', ['stapler', 'validate', app]);
run('spctl', ['--assess', '--type', 'execute', app]);

mkdirSync(info.output, { recursive: true });
const staging = join(info.output, 'staging');
const artifact = `${info.name}-${info.version}-macos-arm64.dmg`;
const dmg = join(info.output, artifact);
rmSync(staging, { recursive: true, force: true });
mkdirSync(staging);
run('ditto', [app, join(staging, `${info.name}.app`)]);
symlinkSync('/Applications', join(staging, 'Applications'));
run('hdiutil', [
  'create',
  '-volname',
  info.name,
  '-srcfolder',
  staging,
  '-ov',
  '-format',
  'UDZO',
  dmg,
]);
run('codesign', [
  '--force',
  '--timestamp',
  '--sign',
  identity,
  ...(process.env.KYL_SIGNING_KEYCHAIN ? ['--keychain', process.env.KYL_SIGNING_KEYCHAIN] : []),
  dmg,
]);
const submission = JSON.parse(
  execFileSync(
    'xcrun',
    ['notarytool', 'submit', dmg, ...notary.verify, '--wait', '--output-format', 'json'],
    { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] },
  ),
);
if (submission.status !== 'Accepted') {
  throw new Error(`Apple rejected the DMG notarization: ${submission.id} (${submission.status}).`);
}
run('xcrun', ['stapler', 'staple', dmg]);
run('xcrun', ['stapler', 'validate', dmg]);
run('codesign', ['--verify', '--strict', dmg]);
run('spctl', ['--assess', '--type', 'open', '--context', 'context:primary-signature', dmg]);
assertClean();
const bytes = readFileSync(dmg);
writeFileSync(
  join(info.output, 'manifest.json'),
  `${JSON.stringify(
    {
      version: info.version,
      shortVersion: info.version,
      bundleId: info.bundleId,
      appBundleName: `${info.name}.app`,
      minMacOS: info.minMacOS,
      platform: 'macos',
      arch: 'arm64',
      format: 'dmg',
      signed: true,
      notarized: true,
      stapled: true,
      sourceCommit,
      sourceDirty: false,
      artifact,
      sizeBytes: bytes.length,
      sha256: createHash('sha256').update(bytes).digest('hex'),
    },
    null,
    2,
  )}\n`,
);
verifyMacOSManifest(info);
console.log(`Verified signed, notarized, and stapled macOS DMG: ${dmg}`);
