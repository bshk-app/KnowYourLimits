import { execFileSync } from 'node:child_process';
import { readReleaseInfo, root } from './release-common.mjs';

const args = process.argv.slice(2);
if (
  args.length > 1 ||
  (args.length === 1 && !['--draft', '--promote', '--publish'].includes(args[0]))
)
  throw new Error('Usage: bun run release [--draft|--promote|--publish]');
const info = readReleaseInfo();
const tag = `v${info.version}`;
const mode = args[0] ?? '--draft';
execFileSync(
  'gh',
  [
    'workflow',
    'run',
    'release.yml',
    '--repo',
    'bshk-app/KnowYourLimits',
    '--ref',
    'main',
    '-f',
    `tag=${tag}`,
    '-F',
    `promote=${mode === '--promote'}`,
    '-F',
    `publish=${mode === '--publish'}`,
  ],
  { cwd: root, stdio: 'inherit' },
);
console.log(
  `Requested ${mode === '--promote' ? 'promotion of existing signed bytes' : mode === '--publish' ? 'build, verification and full publication' : 'verified GitHub/Zamok drafts'} for ${tag}.`,
);
