import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { readReleaseInfo, root, verifyWindowsManifest } from './release-common.mjs';

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
  throw new Error('Set KYL_GITHUB_REPOSITORY to the GitHub owner/repository.');
}
const tag = `v${info.version}`;
const publish = args.includes('--publish');
const env = { ...process.env, KYL_RELEASE_TAG: tag };
const command = [join(root, 'scripts/publish-release.mjs'), '--draft'];
console.log(
  `${publish ? 'Publish' : 'Draft'} ${repository} ${tag}: macOS DMG + Windows EXEs + manifests + SHA256SUMS.txt.`,
);
if (args.includes('--dry-run')) {
  console.log(
    'Plan only: require the clean version tag and matching Windows artifacts, package macOS with Zamok, verify both platforms, then upload and verify the draft.',
  );
  console.log(JSON.stringify(['node', ...command]));
} else {
  execFileSync(process.execPath, [command[0], '--check-tag'], { cwd: root, env, stdio: 'inherit' });
  if (execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim()) {
    throw new Error('Commit all source changes before releasing.');
  }
  const commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
  if (verifyWindowsManifest(info).manifest.sourceCommit !== commit) {
    throw new Error('Download Windows artifacts built from this exact tag before releasing.');
  }
  execFileSync('bun', ['run', 'package:macos'], { cwd: root, stdio: 'inherit' });
  execFileSync('bun', [join(root, 'scripts/sign-updates.mjs')], { cwd: root, stdio: 'inherit' });
  execFileSync(process.execPath, command, { cwd: root, env, stdio: 'inherit' });
  execFileSync(process.execPath, [join(root, 'scripts/zamok-distribution.mjs'), '--stage'], {
    cwd: root,
    env,
    stdio: 'inherit',
  });
  if (publish)
    execFileSync(process.execPath, [join(root, 'scripts/zamok-distribution.mjs'), '--promote'], {
      cwd: root,
      env,
      stdio: 'inherit',
    });
}
