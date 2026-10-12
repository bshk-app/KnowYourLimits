import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  assertReleaseTag,
  readReleaseInfo,
  root,
  verifyMacOSManifest,
  verifyWindowsManifest,
} from './release-common.mjs';
import { sha256, verifyUpdaterArtifacts } from './updater-artifacts.mjs';

export function restoreRelease({
  info,
  tag,
  repository,
  gh,
  git,
  verifyUpdates = verifyUpdaterArtifacts,
  directory = join(root, 'artifacts/release'),
}) {
  assertReleaseTag(info, tag);
  const commit = git('rev-parse', 'HEAD');
  if (
    git('status', '--porcelain') ||
    git('rev-parse', `refs/tags/${tag}^{commit}`) !== commit ||
    JSON.parse(gh('api', `repos/${repository}/commits/${tag}`)).sha !== commit
  ) {
    throw new Error('Promotion requires the clean, exact, remotely published version tag.');
  }
  const base = directory;
  mkdirSync(base, { recursive: true });
  const downloaded = mkdtempSync(join(base, 'restore-'));
  gh('release', 'download', tag, '--repo', repository, '--dir', downloaded);
  const name = `${info.name}-${info.version}`;
  const expected = [
    `${name}-windows-x64.exe`,
    `${name}-windows-x64-setup.exe`,
    `${name}-windows-x64-setup.exe.sig`,
    `${name}-windows-x64-manifest.json`,
    `${name}-macos-arm64.dmg`,
    `${name}-macos-arm64.app.tar.gz`,
    `${name}-macos-arm64.app.tar.gz.sig`,
    `${name}-macos-arm64-manifest.json`,
    'SHA256SUMS.txt',
  ];
  const names = readdirSync(downloaded);
  if (names.length !== expected.length || expected.some((file) => !names.includes(file)))
    throw new Error('Release must contain exactly the nine verified distribution assets.');
  const sums = readFileSync(join(downloaded, 'SHA256SUMS.txt'), 'utf8').trim().split('\n');
  const seen = new Set();
  for (const line of sums) {
    const match = line.match(/^([a-f0-9]{64}) {2}(.+)$/);
    if (
      !match ||
      !expected.includes(match[2]) ||
      match[2] === 'SHA256SUMS.txt' ||
      seen.has(match[2])
    )
      throw new Error('Invalid release checksums.');
    seen.add(match[2]);
    if (sha256(readFileSync(join(downloaded, match[2]))) !== match[1])
      throw new Error(`Downloaded checksum mismatch: ${match[2]}`);
  }
  if (seen.size !== expected.length - 1) throw new Error('Incomplete release checksums.');
  for (const [platform, directory] of [
    ['windows-x64', info.windowsOutput],
    ['macos-arm64', info.output],
  ]) {
    mkdirSync(directory, { recursive: true });
    for (const file of expected.filter((file) => file.startsWith(`${name}-${platform}`))) {
      copyFileSync(
        join(downloaded, file),
        join(directory, file.endsWith('-manifest.json') ? 'manifest.json' : file),
      );
    }
  }
  if (
    verifyMacOSManifest(info).manifest.sourceCommit !== commit ||
    verifyWindowsManifest(info).manifest.sourceCommit !== commit
  )
    throw new Error('Release provenance differs from the version tag.');
  verifyUpdates(info);
  console.log(`Restored and verified the original ${tag} bytes; no rebuilding or re-signing.`);
}

if (process.argv[1]?.endsWith('/restore-release.mjs')) {
  const info = readReleaseInfo();
  const run =
    (command) =>
    (...args) =>
      execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  restoreRelease({
    info,
    tag: process.env.KYL_RELEASE_TAG,
    repository: process.env.KYL_GITHUB_REPOSITORY,
    gh: run('gh'),
    git: run('git'),
  });
}
