import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  readReleaseInfo,
  root,
  verifyMacOSManifest,
  verifyWindowsManifest,
} from './release-common.mjs';
import { describeFile, verifyUpdaterArtifacts } from './updater-artifacts.mjs';

if (process.platform !== 'darwin')
  throw new Error('Sign updater archives on the macOS release runner.');
if (!process.env.TAURI_SIGNING_PRIVATE_KEY && !process.env.TAURI_SIGNING_PRIVATE_KEY_PATH)
  throw new Error('Missing updater signing key.');
const info = readReleaseInfo();
const macos = verifyMacOSManifest(info);
const windows = verifyWindowsManifest(info);
if (macos.manifest.sourceCommit !== windows.manifest.sourceCommit)
  throw new Error('Updater builds must use the same source commit.');
const app = join(info.output, 'staging', `${info.name}.app`);
execFileSync('codesign', ['--verify', '--deep', '--strict', app], { stdio: 'inherit' });
execFileSync('xcrun', ['stapler', 'validate', app], { stdio: 'inherit' });
// Archive the final Zamok-signed/stapled app, never the earlier unsigned Tauri bundle.
const archive = `${info.name}-${info.version}-macos-arm64.app.tar.gz`;
execFileSync(
  'tar',
  ['-czf', join(info.output, archive), '-C', join(info.output, 'staging'), `${info.name}.app`],
  { stdio: 'inherit' },
);
for (const { directory, manifestPath, platform, name } of [
  { directory: info.output, manifestPath: macos.path, platform: 'darwin-aarch64', name: archive },
  {
    directory: info.windowsOutput,
    manifestPath: windows.manifestPath,
    platform: 'windows-x86_64',
    name: `${info.name}-${info.version}-windows-x64-setup.exe`,
  },
]) {
  // Capture CLI output: the signer prints key diagnostics which do not belong in CI logs.
  execFileSync(
    'bun',
    ['run', 'tauri', 'signer', 'sign', '--app-version', info.version, join(directory, name)],
    { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] },
  );
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  manifest.updater = {
    platform,
    artifact: describeFile(directory, name),
    signature: describeFile(directory, `${name}.sig`),
  };
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
}
verifyUpdaterArtifacts(info);
console.log(`Verified version-bound updater signatures for ${info.version} on both platforms.`);
