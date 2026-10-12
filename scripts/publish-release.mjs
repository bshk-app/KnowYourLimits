import { execFileSync } from 'node:child_process';
import { appendFileSync, copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  assertReleaseTag,
  readReleaseInfo,
  root,
  verifyMacOSManifest,
  verifyWindowsManifest,
} from './release-common.mjs';

export function publishRelease({
  info,
  tag,
  commit,
  repository,
  gh,
  directory = join(root, 'artifacts/release', tag),
}) {
  assertReleaseTag(info, tag);
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository || '')) {
    throw new Error('Set KYL_GITHUB_REPOSITORY to the GitHub owner/repository.');
  }
  const verified = verifyWindowsManifest(info);
  const macos = verifyMacOSManifest(info);
  const remote = JSON.parse(gh('api', `repos/${repository}/commits/${tag}`));
  if (
    remote.sha !== commit ||
    verified.manifest.sourceCommit !== commit ||
    macos.manifest.sourceCommit !== commit
  ) {
    throw new Error('The remote tag, checked-out source and both platform builds must match.');
  }
  let existing;
  try {
    existing = JSON.parse(
      gh('release', 'view', tag, '--repo', repository, '--json', 'isDraft,assets'),
    );
  } catch (error) {
    if (!/release not found|HTTP 404/i.test(String(error.stderr))) throw error;
  }
  if (existing && !existing.isDraft) {
    throw new Error('This version is already published. Publish changes under a new version.');
  }
  mkdirSync(directory, { recursive: true });
  const manifestName = `${info.name}-${info.version}-windows-x64-manifest.json`;
  const manifestCopy = join(directory, manifestName);
  copyFileSync(verified.manifestPath, manifestCopy);
  const macosManifestName = `${info.name}-${info.version}-macos-arm64-manifest.json`;
  const macosManifestCopy = join(directory, macosManifestName);
  copyFileSync(macos.path, macosManifestCopy);
  const checksums = join(directory, 'SHA256SUMS.txt');
  writeFileSync(
    checksums,
    verified.manifest.files.map((file) => `${file.sha256}  ${file.name}\n`).join('') +
      `${macos.manifest.sha256}  ${macos.manifest.artifact}\n`,
  );
  const notes = join(directory, 'release-notes.md');
  writeFileSync(
    notes,
    `${readFileSync(info.notes, 'utf8').trim()}\n\n` +
      '## Downloads\n\n' +
      `- Windows installer: \`${info.name}-${info.version}-windows-x64-setup.exe\`.\n` +
      `- Windows portable executable: \`${info.name}-${info.version}-windows-x64.exe\`.\n` +
      '- Windows 10/11 x64 with WebView2; the installer installs for the current user.\n' +
      '- Windows binaries do not yet have an Authenticode signature.\n' +
      `- macOS ${info.minMacOS}+ on Apple Silicon: \`${macos.manifest.artifact}\`.\n` +
      '- The macOS app and DMG are Developer ID signed, notarized, and stapled.\n' +
      '- Verify downloads using SHA256SUMS.txt.\n\n' +
      `Source commit: ${commit}. Built and verified in GitHub Actions.\n`,
  );
  if (!existing) {
    gh(
      'release',
      'create',
      tag,
      '--repo',
      repository,
      '--verify-tag',
      '--draft',
      '--title',
      `${info.name} ${info.version}`,
      '--notes-file',
      notes,
    );
  } else {
    gh(
      'release',
      'edit',
      tag,
      '--repo',
      repository,
      '--title',
      `${info.name} ${info.version}`,
      '--notes-file',
      notes,
    );
  }
  gh(
    'release',
    'upload',
    tag,
    ...verified.files,
    macos.artifact,
    manifestCopy,
    macosManifestCopy,
    checksums,
    '--repo',
    repository,
    '--clobber',
  );
  // The REST tag endpoint only finds published releases; CLI view also finds drafts.
  const uploaded = JSON.parse(
    gh('release', 'view', tag, '--repo', repository, '--json', 'isDraft,assets'),
  );
  const expected = [
    ...verified.manifest.files.map((file) => file.name),
    macos.manifest.artifact,
    manifestName,
    macosManifestName,
    'SHA256SUMS.txt',
  ];
  if (
    uploaded.assets.length !== expected.length ||
    expected.some(
      (name) => !uploaded.assets.some((asset) => asset.name === name && asset.state === 'uploaded'),
    )
  ) {
    throw new Error('Release assets are incomplete; the release remains a draft.');
  }
  gh('release', 'edit', tag, '--repo', repository, '--draft=false', '--latest');
  console.log(`Published https://github.com/${repository}/releases/tag/${tag}`);
}

function main() {
  const args = process.argv.slice(2);
  if (args.length > 1 || (args.length === 1 && args[0] !== '--check-tag')) {
    throw new Error('Usage: node scripts/publish-release.mjs [--check-tag]');
  }
  const info = readReleaseInfo();
  const tag = assertReleaseTag(info, process.env.KYL_RELEASE_TAG);
  const git = (...args) =>
    execFileSync('git', args, {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  const commit = git('rev-parse', 'HEAD');
  if (git('rev-parse', `refs/tags/${tag}^{commit}`) !== commit) {
    throw new Error('Check out the exact release tag before publishing.');
  }
  if (args.includes('--check-tag')) {
    if (process.env.GITHUB_OUTPUT) {
      appendFileSync(process.env.GITHUB_OUTPUT, `version=${info.version}\n`);
    }
    console.log(`Validated ${tag} at ${commit}.`);
    return;
  }
  publishRelease({
    info,
    tag,
    commit,
    repository: process.env.KYL_GITHUB_REPOSITORY,
    gh: (...args) =>
      execFileSync('gh', args, {
        cwd: root,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
      }).trim(),
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
