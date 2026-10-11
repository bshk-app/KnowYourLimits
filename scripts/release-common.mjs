import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('..', import.meta.url));

export function readReleaseInfo(directory = root) {
  const pkg = JSON.parse(readFileSync(join(directory, 'package.json'), 'utf8'));
  const tauri = JSON.parse(readFileSync(join(directory, 'src-tauri/tauri.conf.json'), 'utf8'));
  const cargo = readFileSync(join(directory, 'src-tauri/Cargo.toml'), 'utf8')
    .split('[package]')[1]
    ?.split(/\n\[/)[0];
  const cargoVersion = cargo?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!/^\d+\.\d+\.\d+$/.test(pkg.version)) {
    throw new Error('Use a stable x.y.z version in package.json.');
  }
  if (pkg.version !== tauri.version || pkg.version !== cargoVersion) {
    throw new Error('Versions in package.json, tauri.conf.json and Cargo.toml must match.');
  }
  const notes = join(directory, 'docs/releases', `${pkg.version}.md`);
  if (!readFileSync(notes, 'utf8').startsWith(`# ${pkg.version} `)) {
    throw new Error(`Release notes must start with "# ${pkg.version} ".`);
  }
  return {
    version: pkg.version,
    name: tauri.productName,
    bundleId: tauri.identifier,
    minMacOS: tauri.bundle.macOS.minimumSystemVersion,
    output: join(directory, 'artifacts/macos', pkg.version),
    windowsOutput: join(directory, 'artifacts/windows', pkg.version),
    notes,
  };
}

export function assertReleaseTag(info, tag) {
  if (tag !== `v${info.version}`) {
    throw new Error(`Release tag must match the committed version: v${info.version}`);
  }
  return tag;
}

export function verifyWindowsManifest(info) {
  const manifestPath = join(info.windowsOutput, 'manifest.json');
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (
    manifest.version !== info.version ||
    manifest.platform !== 'windows' ||
    manifest.arch !== 'x64' ||
    typeof manifest.signed !== 'boolean' ||
    !/^[a-f0-9]{40}$/.test(manifest.sourceCommit || '') ||
    manifest.sourceDirty !== false
  ) {
    throw new Error('Windows manifest must describe this version built from a clean commit.');
  }
  const expected = [
    `${info.name}-${info.version}-windows-x64.exe`,
    `${info.name}-${info.version}-windows-x64-setup.exe`,
  ];
  if (
    !Array.isArray(manifest.files) ||
    manifest.files.length !== expected.length ||
    new Set(manifest.files.map((file) => file.name)).size !== expected.length ||
    manifest.files.some((file) => !expected.includes(file.name))
  ) {
    throw new Error('Windows manifest must contain exactly the installer and portable EXE.');
  }
  const files = manifest.files.map((file) => {
    const path = join(info.windowsOutput, file.name);
    const bytes = readFileSync(path);
    const digest = createHash('sha256').update(bytes).digest('hex');
    if (file.sizeBytes !== bytes.length || file.sha256 !== digest) {
      throw new Error(`Windows artifact size or SHA-256 mismatch: ${file.name}`);
    }
    return path;
  });
  return { manifest, manifestPath, files };
}

export function notaryOptions(config, env = process.env) {
  const api = [env.APPLE_API_KEY_PATH, env.APPLE_API_KEY, env.APPLE_API_ISSUER];
  if (api.some(Boolean)) {
    if (!api.every(Boolean)) {
      throw new Error('Set APPLE_API_KEY_PATH, APPLE_API_KEY and APPLE_API_ISSUER together.');
    }
    return {
      package: ['--notary-api-key', api[0], '--notary-key-id', api[1], '--notary-issuer', api[2]],
      verify: ['--key', api[0], '--key-id', api[1], '--issuer', api[2]],
    };
  }
  const profile = env.KYL_NOTARY_PROFILE || config.notaryKeychainProfile;
  if (typeof profile !== 'string' || !profile.trim()) {
    throw new Error('Set KYL_NOTARY_PROFILE to an existing notarytool Keychain profile.');
  }
  const keychain = env.KYL_NOTARY_KEYCHAIN;
  return {
    package: ['--notary-profile', profile, ...(keychain ? ['--notary-keychain', keychain] : [])],
    verify: ['--keychain-profile', profile, ...(keychain ? ['--keychain', keychain] : [])],
  };
}

export function verifyManifest(info) {
  const manifestPath = join(info.output, 'manifest.json');
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (
    manifest.shortVersion !== info.version ||
    manifest.bundleId !== info.bundleId ||
    manifest.appBundleName !== `${info.name}.app` ||
    manifest.minMacOS !== info.minMacOS ||
    manifest.format !== 'dmg' ||
    manifest.notarized !== true ||
    manifest.stapled !== true
  ) {
    throw new Error('Manifest must describe this version of the notarized and stapled macOS DMG.');
  }
  if (
    typeof manifest.artifact !== 'string' ||
    basename(manifest.artifact) !== manifest.artifact ||
    !manifest.artifact.endsWith('.dmg')
  ) {
    throw new Error('Manifest artifact must be a DMG filename without a directory.');
  }
  const digest = createHash('sha256')
    .update(readFileSync(join(info.output, manifest.artifact)))
    .digest('hex');
  if (digest !== manifest.sha256?.toLowerCase()) {
    throw new Error('DMG SHA-256 does not match manifest.json. Re-run package:macos.');
  }
  return { path: manifestPath, artifact: join(info.output, manifest.artifact) };
}
