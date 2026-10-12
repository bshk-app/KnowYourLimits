import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { after, test } from 'node:test';
import { publishRelease } from './publish-release.mjs';
import {
  assertReleaseTag,
  changelogNotes,
  notaryOptions,
  readReleaseInfo,
  root,
  verifyMacOSManifest,
  verifyManifest,
  verifyWindowsManifest,
} from './release-common.mjs';
import { restoreRelease } from './restore-release.mjs';

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

test('automated releases extract only the exact version changelog section', () => {
  const changelog =
    '# Changelog\n\n## [1.2.30] — Later\n\nOther changes.\n\n## [1.2.3](https://example.test/compare) (2026-10-12)\n\n### Features\n\n- Signed updates.\n\n## [1.2.2] — Earlier\n\nOld changes.\n';
  assert.equal(
    changelogNotes(changelog, '1.2.3'),
    '# 1.2.3 — Release notes\n\n### Features\n\n- Signed updates.\n',
  );
  assert.throws(() => changelogNotes(changelog, '1.2.4'), /Missing changelog/);
  assert.throws(
    () => changelogNotes('## [1.2.3]\n\n## [1.2.2]\nOld changes.', '1.2.3'),
    /Empty changelog/,
  );
});

test('release metadata uses changelog when there are no curated version notes', () => {
  const f = fixture();
  rmSync(f.info.notes);
  writeFileSync(join(f.directory, 'CHANGELOG.md'), '# Changelog\n\n## [1.2.3]\n\n- New release.\n');
  assert.equal(
    readReleaseInfo(f.directory).notesMarkdown,
    '# 1.2.3 — Release notes\n\n- New release.\n',
  );
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
  assert.ok(command.includes('--draft'));
  assert.ok(!command.includes('--publish'));
  assert.equal(command[0], 'node');
  assert.equal(command[1], join(root, 'scripts/publish-release.mjs'));
});

function windowsFixture(source = fixture()) {
  const { info } = source;
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

function macosFixture(source = fixture()) {
  const { info, manifest, saveManifest } = source;
  Object.assign(manifest, {
    version: info.version,
    platform: 'macos',
    arch: 'arm64',
    signed: true,
    sourceCommit: 'a'.repeat(40),
    sourceDirty: false,
    artifact: `${info.name}-${info.version}-macos-arm64.dmg`,
  });
  const bytes = Buffer.from('test DMG bytes');
  manifest.sizeBytes = bytes.length;
  writeFileSync(join(info.output, manifest.artifact), bytes);
  saveManifest();
  return source;
}

test('macOS release validates the signed DMG and its source commit', () => {
  const { info, manifest } = macosFixture();
  const verified = verifyMacOSManifest(info);
  assert.equal(verified.manifest.sourceCommit, manifest.sourceCommit);
  writeFileSync(verified.artifact, 'a replaced DMG');
  assert.throws(() => verifyMacOSManifest(info), /SHA-256/);
});

test('macOS release rejects unsigned, dirty, untraceable and mismatched builds', () => {
  for (const [key, value] of [
    ['version', '1.2.2'],
    ['platform', 'windows'],
    ['arch', 'x64'],
    ['signed', false],
    ['sourceDirty', true],
    ['sourceCommit', undefined],
    ['sourceCommit', 'not-a-commit'],
    ['sizeBytes', 0],
  ]) {
    const { info, manifest, saveManifest } = macosFixture();
    manifest[key] = value;
    saveManifest();
    assert.throws(() => verifyMacOSManifest(info), /signed ARM64 build|artifact size/);
  }
});

test('macOS release rejects an unexpected DMG filename even with a valid checksum', () => {
  const { info, manifest, saveManifest } = macosFixture();
  manifest.artifact = 'different.dmg';
  writeFileSync(join(info.output, manifest.artifact), 'test DMG bytes');
  saveManifest();
  assert.throws(() => verifyMacOSManifest(info), /expected versioned DMG filename/);
});

function publicationFixture(options = {}) {
  const source = fixture();
  const windows = windowsFixture(source);
  const macos = macosFixture(source);
  const calls = [];
  let release = options.existing;
  let files = [];
  const directory = mkdtempSync(join(scratch, 'publication-'));
  const input = {
    info: windows.info,
    tag: 'v1.2.3',
    commit: 'a'.repeat(40),
    repository: 'example/KnowYourLimits',
    directory,
    verifyUpdates: () => [],
    gh(...args) {
      calls.push(args);
      if (args[0] === 'api') return JSON.stringify({ sha: options.remoteCommit || input.commit });
      switch (args[1]) {
        case 'view':
          if (!release)
            throw Object.assign(new Error('Not found'), { stderr: 'release not found' });
          return JSON.stringify(release);
        case 'create':
          release = { isDraft: true, assets: [] };
          break;
        case 'upload':
          if (options.uploadFails) throw new Error('Upload failed');
          files = args.slice(3, args.indexOf('--repo'));
          release.assets = files.map((file) => ({
            name: basename(file),
            state: 'uploaded',
            size: statSync(file).size + (options.wrongSize ? 1 : 0),
          }));
          if (options.incomplete) release.assets.pop();
          break;
        case 'download': {
          const dest = args[args.indexOf('--dir') + 1];
          for (const file of files) copyFileSync(file, join(dest, basename(file)));
          if (options.corrupt) writeFileSync(join(dest, basename(files[0])), 'corrupted');
          break;
        }
        case 'edit':
          if (args.includes('--draft=false')) release.isDraft = false;
          break;
        default:
          throw new Error(`Unexpected command: ${args.join(' ')}`);
      }
      return '';
    },
  };
  return { input, calls, windows, macos, getRelease: () => release };
}

test('publication uploads both platforms and checksums before making the release public', () => {
  const { input, calls, getRelease } = publicationFixture();
  publishRelease(input);
  assert.equal(getRelease().isDraft, false);
  assert.equal(getRelease().assets.length, 6);
  const checksums = readFileSync(join(input.directory, 'SHA256SUMS.txt'), 'utf8');
  assert.equal(checksums.trim().split('\n').length, 5);
  assert.match(checksums, /Example-1\.2\.3-macos-arm64\.dmg/);
  const notes = readFileSync(join(input.directory, 'release-notes.md'), 'utf8');
  assert.match(notes, /Windows installer/);
  assert.match(notes, /macOS 13\.0\+ on Apple Silicon/);
  assert.ok(calls.find((args) => args[1] === 'create').includes('--draft'));
  assert.ok(calls.at(-1).includes('--draft=false'));
  assert.equal(calls.at(-2)[1], 'download');
});

test('publication leaves failed or incomplete uploads as a draft', () => {
  for (const options of [{ incomplete: true }, { uploadFails: true }, { wrongSize: true }]) {
    const { input, calls, getRelease } = publicationFixture(options);
    assert.throws(() => publishRelease(input), /incomplete|Upload failed/);
    assert.equal(getRelease().isDraft, true);
    assert.ok(!calls.some((args) => args.includes('--draft=false')));
  }
});

test('publication rejects corrupted downloads while keeping the release a draft', () => {
  const { input, calls, getRelease } = publicationFixture({ corrupt: true });
  assert.throws(() => publishRelease(input), /Uploaded SHA-256 mismatch/);
  assert.equal(getRelease().isDraft, true);
  assert.ok(!calls.some((args) => args.includes('--draft=false')));
});

test('manual verification can keep a fully verified release as a draft', () => {
  const { input, calls, getRelease } = publicationFixture();
  publishRelease({ ...input, publish: false });
  assert.equal(getRelease().isDraft, true);
  assert.ok(calls.some((args) => args[1] === 'download'));
  assert.ok(!calls.some((args) => args.includes('--draft=false')));
});

test('publication can resume a draft but refuses to overwrite a public release', () => {
  const draft = publicationFixture({ existing: { isDraft: true, assets: [] } });
  publishRelease(draft.input);
  assert.equal(draft.getRelease().isDraft, false);
  assert.ok(!draft.calls.some((args) => args[1] === 'create'));
  const published = publicationFixture({ existing: { isDraft: false, assets: [] } });
  assert.throws(() => publishRelease(published.input), /already published/);
  assert.ok(!published.calls.some((args) => ['edit', 'upload'].includes(args[1])));
});

test('publication refuses to mix artifacts from another commit or a moved remote tag', () => {
  const remote = publicationFixture({ remoteCommit: 'b'.repeat(40) });
  assert.throws(() => publishRelease(remote.input), /both platform builds must match/);
  for (const platform of ['windows', 'macos']) {
    const source = publicationFixture();
    source[platform].manifest.sourceCommit = 'b'.repeat(40);
    if (platform === 'windows') source.windows.saveManifest();
    else {
      writeFileSync(
        join(source.input.info.output, 'manifest.json'),
        JSON.stringify(source.macos.manifest),
      );
    }
    assert.throws(() => publishRelease(source.input), /both platform builds must match/);
    assert.ok(!source.calls.some((args) => args[0] === 'release'));
  }
});

function promotionFixture({
  corrupt = false,
  unexpected = false,
  sourceCommit = 'a'.repeat(40),
} = {}) {
  const f = publicationFixture();
  const prefix = `${f.input.info.name}-${f.input.info.version}`;
  const extra = [
    join(f.input.info.output, `${prefix}-macos-arm64.app.tar.gz`),
    join(f.input.info.output, `${prefix}-macos-arm64.app.tar.gz.sig`),
    join(f.input.info.windowsOutput, `${prefix}-windows-x64-setup.exe.sig`),
  ];
  for (const path of extra) writeFileSync(path, `promotion test ${basename(path)}`);
  f.input.verifyUpdates = () => [
    { artifact: extra[0], signature: extra[1] },
    {
      artifact: join(f.input.info.windowsOutput, `${prefix}-windows-x64-setup.exe`),
      signature: extra[2],
    },
  ];
  publishRelease({ ...f.input, publish: false });
  const calls = [];
  const gh = (...args) => {
    calls.push(args);
    if (args[0] === 'api') return JSON.stringify({ sha: sourceCommit });
    assert.equal(args[1], 'download');
    const output = args[args.indexOf('--dir') + 1];
    const original = join(f.input.directory, 'downloaded');
    for (const name of readdirSync(original))
      copyFileSync(join(original, name), join(output, name));
    if (corrupt) writeFileSync(join(output, basename(extra[0])), 'changed archive');
    if (unexpected) writeFileSync(join(output, 'unknown.exe'), 'unexpected download');
    return '';
  };
  const git = (...args) => (args[0] === 'status' ? '' : f.input.commit);
  return {
    input: {
      info: f.input.info,
      tag: f.input.tag,
      repository: f.input.repository,
      gh,
      git,
      verifyUpdates: f.input.verifyUpdates,
      directory: mkdtempSync(join(scratch, 'restore-')),
    },
    calls,
  };
}

test('promotion restores nine original assets and never uploads or rebuilds', () => {
  const f = promotionFixture();
  restoreRelease(f.input);
  assert.ok(f.calls.every((args) => args[0] === 'api' || args[1] === 'download'));
});

test('promotion rejects altered bytes, extra assets and moved remote tags', () => {
  for (const options of [
    { corrupt: true },
    { unexpected: true },
    { sourceCommit: 'b'.repeat(40) },
  ]) {
    const f = promotionFixture(options);
    assert.throws(() => restoreRelease(f.input), /checksum mismatch|nine verified|exact/);
  }
});
