import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { root, verifyMacOSManifest, verifyWindowsManifest } from './release-common.mjs';

export const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
export function describeFile(directory, name) {
  const bytes = readFileSync(join(directory, name));
  return { name, sizeBytes: bytes.length, sha256: sha256(bytes) };
}

export function verifySignature(artifact, signature, pubkey, version, run = execFileSync) {
  const publicLines = Buffer.from(pubkey, 'base64').toString('utf8').trim().split('\n');
  const decoded = Buffer.from(readFileSync(signature, 'utf8').trim(), 'base64').toString('utf8');
  const lines = decoded.trim().split('\n');
  if (publicLines.length !== 2 || lines.length !== 4)
    throw new Error('Invalid updater signature/key encoding.');
  const scratch = mkdtempSync(join(tmpdir(), 'kyl-signature-'));
  try {
    const path = join(scratch, 'signature.minisig');
    writeFileSync(path, decoded);
    // minisign verifies both the artifact and its authenticated trusted comment.
    run('minisign', ['-Vm', artifact, '-P', publicLines[1], '-x', path], {
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    if (
      !lines[2].startsWith('trusted comment: ') ||
      !lines[2].split('\t').includes(`version:${version}`)
    ) {
      throw new Error('Updater signature is not bound to this app version.');
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

export function verifyUpdaterArtifacts(info, { pubkey, verify = verifySignature } = {}) {
  const key =
    pubkey ??
    JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8')).plugins.updater
      .pubkey;
  const macos = verifyMacOSManifest(info);
  const windows = verifyWindowsManifest(info);
  const definitions = [
    {
      directory: info.output,
      manifest: macos.manifest,
      platform: 'darwin-aarch64',
      name: `${info.name}-${info.version}-macos-arm64.app.tar.gz`,
    },
    {
      directory: info.windowsOutput,
      manifest: windows.manifest,
      platform: 'windows-x86_64',
      name: `${info.name}-${info.version}-windows-x64-setup.exe`,
    },
  ];
  return definitions.map(({ directory, manifest, platform, name }) => {
    const update = manifest.updater;
    if (
      !update ||
      update.platform !== platform ||
      update.artifact?.name !== name ||
      update.signature?.name !== `${name}.sig`
    ) {
      throw new Error(`Missing or invalid signed updater artifact for ${platform}.`);
    }
    for (const file of [update.artifact, update.signature]) {
      if (basename(file.name) !== file.name)
        throw new Error('Updater asset paths must be filenames.');
      const actual = describeFile(directory, file.name);
      if (actual.sizeBytes !== file.sizeBytes || actual.sha256 !== file.sha256)
        throw new Error(`Updater checksum mismatch: ${file.name}`);
    }
    const artifact = join(directory, name);
    const signature = join(directory, `${name}.sig`);
    verify(artifact, signature, key, info.version);
    return { platform, artifact, signature, metadata: update };
  });
}
