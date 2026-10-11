import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { readReleaseInfo, root, verifyManifest } from './release-common.mjs';

const args = process.argv.slice(2);
if (
  new Set(args).size !== args.length ||
  args.some((arg) => !['--publish', '--dry-run'].includes(arg))
) {
  throw new Error('Usage: bun run release:github [--publish] [--dry-run]');
}
const info = readReleaseInfo();
const repository = process.env.KYL_GITHUB_REPOSITORY;
if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository || '')) {
  throw new Error('Set KYL_GITHUB_REPOSITORY to the confirmed GitHub owner/repository.');
}
const publish = args.includes('--publish');
const mirrorArgs = [
  'github-release',
  '--manifest',
  join(info.output, 'manifest.json'),
  '--github-repository',
  repository,
  '--github-tag',
  `v${info.version}`,
  '--git-directory',
  root,
  '--release-notes-path',
  info.notes,
  '--channel',
  'STABLE',
  publish ? '--publish' : '--no-publish',
];
console.log(
  `${publish ? 'Publish' : 'Draft'} ${repository} v${info.version}: DMG + manifest.json.`,
);
if (args.includes('--dry-run')) {
  console.log(
    'Plan only: build with Tauri, sign/notarize with Zamok, verify, then mirror to GitHub.',
  );
  console.log(
    JSON.stringify(['zamokctl', ...mirrorArgs, '--github-target-commit', '<clean HEAD>']),
  );
} else {
  const git = (...gitArgs) =>
    execFileSync('git', ['-C', root, ...gitArgs], { encoding: 'utf8' }).trim();
  if (git('rev-parse', '--show-toplevel') !== root.replace(/\/$/, '')) {
    throw new Error('Initialize a Git repository at the project root before releasing.');
  }
  const assertClean = () => {
    if (git('status', '--porcelain')) {
      throw new Error('Commit all source changes before releasing.');
    }
  };
  assertClean();
  const commit = git('rev-parse', 'HEAD');
  if (!existsSync(join(root, 'LICENSE')) || !readFileSync(join(root, 'LICENSE'), 'utf8').trim()) {
    throw new Error('Choose and commit an open-source LICENSE before releasing.');
  }
  const token =
    process.env.GITHUB_TOKEN ||
    execFileSync('gh', ['auth', 'token'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  if (!token) throw new Error('Sign in with gh auth login or set GITHUB_TOKEN.');
  const env = { ...process.env, GITHUB_TOKEN: token };
  const remote = JSON.parse(
    execFileSync('gh', ['api', `repos/${repository}/commits/${commit}`], {
      encoding: 'utf8',
      env,
      stdio: ['ignore', 'pipe', 'inherit'],
    }),
  );
  if (remote.sha !== commit)
    throw new Error('Push the exact source commit to GitHub before releasing.');
  execFileSync('bun', ['run', 'package:macos'], { cwd: root, stdio: 'inherit' });
  verifyManifest(info);
  assertClean();
  if (git('rev-parse', 'HEAD') !== commit)
    throw new Error('HEAD changed while packaging. Re-run release:github.');
  execFileSync('zamokctl', [...mirrorArgs, '--github-target-commit', commit], {
    cwd: root,
    env,
    stdio: 'inherit',
  });
}
