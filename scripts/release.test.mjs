import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import {
  assertReleaseTag,
  notaryOptions,
  readReleaseInfo,
  root,
  verifyManifest,
  verifyWindowsManifest,
} from './release-common.mjs';

const scratch = mkdtempSync(join(tmpdir(), 'kyl-release-test-'));
after(() => rmSync(scratch, { recursive: true, force: true }));

function fixture() {
  const directory = mkdtempSync(join(scratch, 'source-'));
  mkdirSync(join(directory, 'src-tauri'));
  mkdirSync(join(directory, 'docs/releases'), { recursive: true });
  writeFileSync(join(directory, 'package.json'), JSON.stringify({ version: '1.2.3' }));
  writeFileSync(join(directory, 'src-tauri/Cargo.toml'), '[package]\nversion = "1.2.3"\n');
  writeFileSync(
    join(directory, 'src-tauri/tauri.conf.json'),
    JSON.stringify({
      version: '1.2.3',
      productName: 'Example',
      identifier: 'app.example',
      bundle: { macOS: { minimumSystemVersion: '13.0' } },
    }),
  );
  writeFileSync(join(directory, 'docs/releases/1.2.3.md'), '# 1.2.3 — Example\n');
  const info = readReleaseInfo(directory);
  mkdirSync(info.output, { recursive: true });
  const bytes = Buffer.from('test DMG bytes');
  const manifest = {
    artifact: 'Example-1.2.3.dmg',
    sha256: createHash('sha256').update(bytes).digest('hex'),
    shortVersion: info.version,
    bundleVersion: '123',
    bundleId: info.bundleId,
    appBundleName: `${info.name}.app`,
    minMacOS: info.minMacOS,
    format: 'dmg',
    notarized: true,
    stapled: true,
  };
  writeFileSync(join(info.output, manifest.artifact), bytes);
  const saveManifest = () =>
    writeFileSync(join(info.output, 'manifest.json'), JSON.stringify(manifest));
  saveManifest();
  return { directory, info, manifest, saveManifest };
}

test('release metadata rejects version drift across Rust and Tauri', () => {
  const { directory } = fixture();
  writeFileSync(join(directory, 'src-tauri/Cargo.toml'), '[package]\nversion = "1.2.2"\n');
  assert.throws(() => readReleaseInfo(directory), /must match/);
});

test('release metadata rejects notes for a different version', () => {
  const { directory } = fixture();
  writeFileSync(join(directory, 'docs/releases/1.2.3.md'), '# 1.2.2 — Example\n');
  assert.throws(() => readReleaseInfo(directory), /Release notes/);
});

test('notarization can use a profile in a separate Keychain', () => {
  const options = notaryOptions(
    { notaryKeychainProfile: 'default' },
    {
      KYL_NOTARY_PROFILE: 'release',
      KYL_NOTARY_KEYCHAIN: '/tmp/release.keychain-db',
    },
  );
  assert.deepEqual(options.package, [
    '--notary-profile',
    'release',
    '--notary-keychain',
    '/tmp/release.keychain-db',
  ]);
  assert.deepEqual(options.verify, [
    '--keychain-profile',
    'release',
    '--keychain',
    '/tmp/release.keychain-db',
  ]);
});

test('App Store Connect credentials use the same interface as Tish', () => {
  const options = notaryOptions(
    { notaryKeychainProfile: 'default' },
    {
      APPLE_API_KEY_PATH: '/tmp/test.p8',
      APPLE_API_KEY: 'test-key',
      APPLE_API_ISSUER: 'test-issuer',
    },
  );
  assert.deepEqual(options.package, [
    '--notary-api-key',
    '/tmp/test.p8',
    '--notary-key-id',
    'test-key',
    '--notary-issuer',
    'test-issuer',
  ]);
  assert.deepEqual(options.verify, [
    '--key',
    '/tmp/test.p8',
    '--key-id',
    'test-key',
    '--issuer',
    'test-issuer',
  ]);
});

test('partial API credentials fail instead of falling back to another account', () => {
  assert.throws(
    () =>
      notaryOptions(
        { notaryKeychainProfile: 'default' },
        {
          APPLE_API_KEY_PATH: '/tmp/test.p8',
        },
      ),
    /together/,
  );
});

test('manifest validates the exact packaged bytes', () => {
  const { info } = fixture();
  const verified = verifyManifest(info);
  writeFileSync(verified.artifact, 'a different build');
  assert.throws(() => verifyManifest(info), /SHA-256/);
});

test('manifest rejects an older version or a different product', () => {
  for (const [key, value] of [
    ['shortVersion', '1.2.2'],
    ['bundleId', 'app.other'],
  ]) {
    const { info, manifest, saveManifest } = fixture();
    manifest[key] = value;
    saveManifest();
    assert.throws(() => verifyManifest(info), /this version/);
  }
});

test('manifest rejects unnotarized and unstapled packages', () => {
  for (const key of ['notarized', 'stapled']) {
    const { info, manifest, saveManifest } = fixture();
    manifest[key] = false;
    saveManifest();
    assert.throws(() => verifyManifest(info), /notarized and stapled/);
  }
});

test('manifest rejects artifact paths outside the output directory', () => {
  const { info, manifest, saveManifest } = fixture();
  manifest.artifact = '../another.dmg';
  saveManifest();
  assert.throws(() => verifyManifest(info), /without a directory/);
});

test('dry-run prepares a draft without credentials, Git or network access', () => {
  const output = execFileSync(
    process.execPath,
    [join(root, 'scripts/release-github.mjs'), '--dry-run'],
    {
      encoding: 'utf8',
      env: { ...process.env, KYL_GITHUB_REPOSITORY: 'example/KnowYourLimits', GITHUB_TOKEN: '' },
    },
  );
  const command = JSON.parse(output.trim().split('\n').at(-1));
  assert.ok(command.includes('--no-publish'));
  assert.ok(!command.includes('--publish'));
  assert.equal(command[0], 'zamokctl');
  assert.equal(command.at(-1), '<clean HEAD>');
});

function windowsFixture() {
  const { info } = fixture();
  mkdirSync(info.windowsOutput, { recursive: true });
  const files = [
    `${info.name}-${info.version}-windows-x64.exe`,
    `${info.name}-${info.version}-windows-x64-setup.exe`,
  ].map((name) => {
    const bytes = Buffer.from(`synthetic test bytes for ${name}`);
    writeFileSync(join(info.windowsOutput, name), bytes);
    return {
      name,
      sizeBytes: bytes.length,
      sha256: createHash('sha256').update(bytes).digest('hex'),
    };
  });
  const manifest = {
    version: info.version,
    platform: 'windows',
    arch: 'x64',
    signed: false,
    sourceCommit: 'a'.repeat(40),
    sourceDirty: false,
    files,
  };
  const saveManifest = () =>
    writeFileSync(join(info.windowsOutput, 'manifest.json'), JSON.stringify(manifest));
  saveManifest();
  return { info, manifest, saveManifest };
}

test('release tag must match the exact stable source version', () => {
  const { info } = fixture();
  assert.equal(assertReleaseTag(info, 'v1.2.3'), 'v1.2.3');
  for (const tag of [undefined, 'main', 'v1.2.2', 'v1.2.3-beta', 'v1.2.3\nversion=other']) {
    assert.throws(() => assertReleaseTag(info, tag), /tag must match/);
  }
});

test('Windows manifest validates both executables and their exact bytes', () => {
  const { info, manifest } = windowsFixture();
  const verified = verifyWindowsManifest(info);
  assert.equal(verified.files.length, 2);
  assert.equal(verified.manifest.sourceCommit, manifest.sourceCommit);
  writeFileSync(verified.files[1], 'a replaced installer');
  assert.throws(() => verifyWindowsManifest(info), /SHA-256 mismatch/);
});

test('Windows release rejects dirty, untraceable and mismatched builds', () => {
  for (const [key, value] of [
    ['sourceDirty', true],
    ['sourceCommit', undefined],
    ['sourceCommit', 'not-a-commit'],
    ['version', '1.2.2'],
    ['platform', 'macos'],
    ['arch', 'arm64'],
    ['signed', 'false'],
  ]) {
    const { info, manifest, saveManifest } = windowsFixture();
    manifest[key] = value;
    saveManifest();
    assert.throws(() => verifyWindowsManifest(info), /clean commit/);
  }
});

test('Windows release rejects extra, missing, duplicate and escaping assets', () => {
  for (const mutate of [
    (files) => files.pop(),
    (files) => files.push(files[0]),
    (files) => {
      files[1] = files[0];
    },
    (files) => {
      files[0].name = '../other.exe';
    },
  ]) {
    const { info, manifest, saveManifest } = windowsFixture();
    mutate(manifest.files);
    saveManifest();
    assert.throws(() => verifyWindowsManifest(info), /exactly the installer/);
  }
});
