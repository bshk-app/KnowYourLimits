import { execFileSync } from 'node:child_process';
import { readReleaseInfo, root, verifyMacOSManifest } from './release-common.mjs';

if (process.argv.length !== 2) throw new Error('Usage: bun run package:macos:ci');
const info = readReleaseInfo();
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
const commit = git('rev-parse', 'HEAD');
if (git('status', '--porcelain')) {
  throw new Error('Package the macOS release from a clean source commit.');
}
execFileSync('bun', ['run', 'package:macos'], { cwd: root, stdio: 'inherit' });
const verified = verifyMacOSManifest(info);
if (git('rev-parse', 'HEAD') !== commit || verified.manifest.sourceCommit !== commit) {
  throw new Error('The source commit changed during macOS packaging.');
}
