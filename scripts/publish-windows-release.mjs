import { execFileSync } from 'node:child_process';
import { appendFileSync, copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  assertReleaseTag,
  readReleaseInfo,
  root,
  verifyWindowsManifest,
} from './release-common.mjs';

const args = process.argv.slice(2);
if (args.length > 1 || (args.length === 1 && args[0] !== '--check-tag')) {
  throw new Error('Usage: node scripts/publish-windows-release.mjs [--check-tag]');
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
} else {
  const repository = process.env.KYL_GITHUB_REPOSITORY;
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository || '')) {
    throw new Error('Set KYL_GITHUB_REPOSITORY to the GitHub owner/repository.');
  }
  const gh = (...args) =>
    execFileSync('gh', args, {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  const verified = verifyWindowsManifest(info);
  const remote = JSON.parse(gh('api', `repos/${repository}/commits/${tag}`));
  if (remote.sha !== commit || verified.manifest.sourceCommit !== commit) {
    throw new Error('The remote tag, checked-out source and Windows build must match.');
  }
  let existing;
  try {
    existing = JSON.parse(gh('api', `repos/${repository}/releases/tags/${tag}`));
  } catch (error) {
    if (!String(error.stderr).includes('HTTP 404')) throw error;
  }
  if (existing && !existing.draft) {
    throw new Error('This version is already published. Publish changes under a new version.');
  }
  const directory = join(root, 'artifacts/release', tag);
  mkdirSync(directory, { recursive: true });
  const manifestName = `${info.name}-${info.version}-windows-x64-manifest.json`;
  const manifestCopy = join(directory, manifestName);
  copyFileSync(verified.manifestPath, manifestCopy);
  const checksums = join(directory, 'SHA256SUMS.txt');
  writeFileSync(
    checksums,
    verified.manifest.files.map((file) => `${file.sha256}  ${file.name}\n`).join(''),
  );
  const notes = join(directory, 'release-notes.md');
  writeFileSync(
    notes,
    `${readFileSync(info.notes, 'utf8').trim()}\n\n` +
      '## Скачать для Windows\n\n' +
      `- Установка: \`${info.name}-${info.version}-windows-x64-setup.exe\`.\n` +
      `- Без установки: \`${info.name}-${info.version}-windows-x64.exe\`.\n` +
      '- Windows 10/11 x64, WebView2. Установщик работает для текущего пользователя.\n' +
      '- Сборка пока без подписи Authenticode. SHA-256 доступны в SHA256SUMS.txt.\n' +
      '- В этот релиз входит Windows-сборка; подписанный DMG macOS публикуется отдельно.\n\n' +
      `Исходники: ${commit}. Собрано и проверено в GitHub Actions.\n`,
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
    manifestCopy,
    checksums,
    '--repo',
    repository,
    '--clobber',
  );
  const uploaded = JSON.parse(gh('api', `repos/${repository}/releases/tags/${tag}`));
  const expected = [
    ...verified.manifest.files.map((file) => file.name),
    manifestName,
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
