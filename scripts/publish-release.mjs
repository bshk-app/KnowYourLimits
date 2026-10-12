import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  assertReleaseTag,
  readReleaseInfo,
  root,
  verifyMacOSManifest,
  verifyWindowsManifest,
} from './release-common.mjs';
import { verifyUpdaterArtifacts } from './updater-artifacts.mjs';

export function publishRelease({
  info,
  tag,
  commit,
  repository,
  gh,
  publish = true,
  verifyUpdates = verifyUpdaterArtifacts,
  directory = join(root, 'artifacts/release', tag),
}) {
  assertReleaseTag(info, tag);
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository || '')) {
    throw new Error('Set KYL_GITHUB_REPOSITORY to the GitHub owner/repository.');
  }
  const verified = verifyWindowsManifest(info);
  const macos = verifyMacOSManifest(info);
  const updates = verifyUpdates(info);
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
  const files = [
    ...new Set([
      ...verified.files,
      macos.artifact,
      ...updates.flatMap((update) => [update.artifact, update.signature]),
      manifestCopy,
      macosManifestCopy,
    ]),
  ];
  const digest = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
  const checksums = join(directory, 'SHA256SUMS.txt');
  writeFileSync(checksums, files.map((path) => `${digest(path)}  ${basename(path)}\n`).join(''));
  files.push(checksums);
  const notes = join(directory, 'release-notes.md');
  writeFileSync(
    notes,
    `${(info.notesMarkdown ?? readFileSync(info.notes, 'utf8')).trim()}\n\n` +
      '## Downloads\n\n' +
      `- Windows installer: \`${info.name}-${info.version}-windows-x64-setup.exe\`.\n` +
      `- Windows portable executable: \`${info.name}-${info.version}-windows-x64.exe\`.\n` +
      '- Windows 10/11 x64 with WebView2; the installer installs for the current user.\n' +
      '- Windows binaries do not yet have an Authenticode signature.\n' +
      `- macOS ${info.minMacOS}+ on Apple Silicon: \`${macos.manifest.artifact}\`.\n` +
      '- The macOS app and DMG are Developer ID signed, notarized, and stapled.\n' +
      '- Both updater packages have version-bound Minisign signatures verified by the app.\n' +
      '- Verify downloads using SHA256SUMS.txt.\n\n' +
      `Both platform builds use source commit: ${commit}.\n`,
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
  gh('release', 'upload', tag, ...files, '--repo', repository, '--clobber');
  // The REST tag endpoint only finds published releases; CLI view also finds drafts.
  const uploaded = JSON.parse(
    gh('release', 'view', tag, '--repo', repository, '--json', 'isDraft,assets'),
  );
  const expected = files.map((path) => ({
    name: basename(path),
    size: readFileSync(path).length,
    sha256: digest(path),
  }));
  if (
    !uploaded.isDraft ||
    uploaded.assets.length !== expected.length ||
    expected.some(
      (file) =>
        !uploaded.assets.some(
          (asset) =>
            asset.name === file.name && asset.state === 'uploaded' && asset.size === file.size,
        ),
    )
  ) {
    throw new Error('Release assets are incomplete; the release remains a draft.');
  }
  const downloaded = join(directory, 'downloaded');
  mkdirSync(downloaded, { recursive: true });
  gh('release', 'download', tag, '--repo', repository, '--dir', downloaded, '--clobber');
  for (const file of expected) {
    if (digest(join(downloaded, file.name)) !== file.sha256) {
      throw new Error(`Uploaded SHA-256 mismatch: ${file.name}; the release remains a draft.`);
    }
  }
  if (publish) gh('release', 'edit', tag, '--repo', repository, '--draft=false', '--latest');
  console.log(
    `${publish ? 'Published' : 'Verified draft'} https://github.com/${repository}/releases/tag/${tag}`,
  );
}

function main() {
  const args = process.argv.slice(2);
  if (args.length > 1 || (args.length === 1 && !['--check-tag', '--draft'].includes(args[0]))) {
    throw new Error('Usage: node scripts/publish-release.mjs [--check-tag|--draft]');
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
  if (git('status', '--porcelain')) {
    throw new Error('Commit all source changes before publishing.');
  }
  publishRelease({
    info,
    tag,
    commit,
    repository: process.env.KYL_GITHUB_REPOSITORY,
    publish: false,
    gh: (...args) =>
      execFileSync('gh', args, {
        cwd: root,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
      }).trim(),
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
