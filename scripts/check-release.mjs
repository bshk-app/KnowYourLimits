import { readReleaseInfo } from './release-common.mjs';

const info = readReleaseInfo();
console.log(`Release metadata matches: ${info.name} ${info.version}, macOS ${info.minMacOS}+.`);
