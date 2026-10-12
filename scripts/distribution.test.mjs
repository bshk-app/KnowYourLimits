import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { verifySignature } from './updater-artifacts.mjs';
import {
  promoteDistribution,
  stageDistribution,
  verifyHostedRelease,
} from './zamok-distribution.mjs';

const scratch = mkdtempSync(join(tmpdir(), 'kyl-distribution-test-'));
after(() => rmSync(scratch, { recursive: true, force: true }));
const config = {
  endpoint: 'https://zamok.example',
  product: 'example',
  channel: 'STABLE',
  repository: 'example/app',
};
function fixture() {
  const directory = mkdtempSync(join(scratch, 'case-'));
  const notes = join(directory, 'notes.md');
  writeFileSync(notes, '# 1.2.3 — Update\n');
  const updates = ['darwin-aarch64', 'windows-x86_64'].map((platform) => {
    const artifact = join(directory, `${platform}.bin`);
    const signature = `${artifact}.sig`;
    const bytes = Buffer.from(`Real test artifact ${platform}`);
    writeFileSync(artifact, bytes);
    writeFileSync(signature, `test signature ${platform}\n`);
    return {
      platform,
      artifact,
      signature,
      metadata: {
        artifact: {
          sizeBytes: bytes.length,
          sha256: createHash('sha256').update(bytes).digest('hex'),
        },
      },
    };
  });
  const release = {
    id: 'release-123',
    shortVersion: '1.2.3',
    status: 'DRAFT',
    minPlan: null,
    artifacts: updates.map((update) => ({
      platform: update.platform,
      variant: 'online',
      signature: readFileSync(update.signature, 'utf8').trim(),
      enclosureLength: String(update.metadata.artifact.sizeBytes),
      enclosureUrl: `https://storage.example/${update.platform}`,
    })),
  };
  const info = { version: '1.2.3', notes };
  const fetcher = async (url) => {
    const update = updates.find((item) => url.endsWith(item.platform));
    if (update) return new Response(readFileSync(update.artifact));
    if (url.includes('/1.2.3?')) return new Response(null, { status: 204 });
    const artifact = release.artifacts.find((item) =>
      url.includes(item.platform.replace('-', '/')),
    );
    if (!artifact) throw new Error(`Unexpected URL ${url}`);
    return Response.json({
      version: info.version,
      signature: artifact.signature,
      url: artifact.enclosureUrl,
    });
  };
  return { info, config, updates, release, fetcher };
}

test('host verification rejects altered bytes, signatures and gated releases', async () => {
  const f = fixture();
  await verifyHostedRelease(f.release, f.updates, f.info.version, f.fetcher);
  await assert.rejects(
    verifyHostedRelease(f.release, f.updates, f.info.version, async () => new Response('tampered')),
    /SHA-256/,
  );
  f.release.artifacts[0].signature = 'wrong signature';
  await assert.rejects(
    verifyHostedRelease(f.release, f.updates, f.info.version, f.fetcher),
    /metadata mismatch/,
  );
  f.release.minPlan = 'PRO';
  await assert.rejects(
    verifyHostedRelease(f.release, f.updates, f.info.version, f.fetcher),
    /public/,
  );
});

test('a verified existing draft resumes without uploading or allocating a duplicate version', async () => {
  const f = fixture();
  const api = async (_path, method = 'GET') => {
    assert.equal(method, 'GET');
    return { data: [f.release] };
  };
  await stageDistribution({ ...f, api });
});

test('staging never rewrites a published release', async () => {
  const f = fixture();
  f.release.status = 'PUBLISHED';
  await assert.rejects(
    stageDistribution({ ...f, api: async () => ({ data: [f.release] }) }),
    /immutable/,
  );
});

test('a new release uploads both exact packages and finalizes only a draft', async () => {
  const f = fixture();
  const events = [];
  let finalized = false;
  const api = async (path, method = 'GET', body) => {
    if (method === 'GET') return { data: finalized ? [f.release] : [] };
    assert.equal(method, 'POST');
    if (path.endsWith('/upload-url')) {
      events.push(`allocate:${body.filename}`);
      return { uploadId: body.filename, presignedUrl: `https://upload.example/${body.filename}` };
    }
    assert.ok(path.endsWith('/tauri/finalize'));
    assert.equal(body.publish, false);
    assert.equal(body.minPlan, null);
    assert.equal(body.artifacts.length, 2);
    assert.deepEqual(
      body.artifacts.map((item) => item.platform),
      f.updates.map((item) => item.platform),
    );
    finalized = true;
    events.push('finalize-draft');
    return { id: f.release.id, status: 'DRAFT' };
  };
  const fetcher = async (url, options) => {
    if (options?.method === 'PUT') {
      const update = f.updates.find((item) => url.endsWith(`${item.platform}.bin`));
      assert.deepEqual(options.body, readFileSync(update.artifact));
      events.push(`upload:${update.platform}`);
      return new Response(null, { status: 200 });
    }
    return f.fetcher(url);
  };
  await stageDistribution({ ...f, api, fetcher });
  assert.deepEqual(events, [
    'allocate:darwin-aarch64.bin',
    'upload:darwin-aarch64',
    'allocate:windows-x86_64.bin',
    'upload:windows-x86_64',
    'finalize-draft',
  ]);
});

function promotion(f, options = {}) {
  const events = [];
  const api = async (path, method = 'GET', body) => {
    if (method === 'PUT') {
      assert.equal(body.where.id, f.release.id);
      assert.equal(body.data.pubDate, body.data.publishedAt);
      assert.ok(Number.isFinite(Date.parse(body.data.pubDate)));
      events.push('publish-feed');
      return { data: { id: f.release.id, status: 'PUBLISHED' } };
    }
    const q = JSON.parse(decodeURIComponent(path.split('?q=')[1]));
    return {
      data: q.where.status ? (options.newer ? [{ shortVersion: '1.3.0' }] : []) : [f.release],
    };
  };
  const gh = (...args) => {
    if (args[1] === 'view') return JSON.stringify({ isDraft: !options.publicGitHub });
    events.push('publish-github');
    return '';
  };
  const cask = async () => {
    events.push('publish-cask');
    if (options.caskFails) throw new Error('Tap unavailable');
  };
  return { args: { ...f, tag: 'v1.2.3', api, gh, cask }, events };
}

test('promotion exposes the verified updater only after GitHub and Homebrew succeed', async () => {
  const p = promotion(fixture());
  await promoteDistribution(p.args);
  assert.deepEqual(p.events, ['publish-github', 'publish-cask', 'publish-feed']);
});

test('failed Homebrew publication leaves the updater draft and can resume with public GitHub bytes', async () => {
  const f = fixture();
  const p = promotion(f, { caskFails: true });
  await assert.rejects(promoteDistribution(p.args), /Tap unavailable/);
  assert.ok(!p.events.includes('publish-feed'));
  const retry = promotion(f, { publicGitHub: true });
  await promoteDistribution(retry.args);
  assert.deepEqual(retry.events, ['publish-cask', 'publish-feed']);
});

test('promotion refuses to roll a newer stable release back', async () => {
  const p = promotion(fixture(), { newer: true });
  await assert.rejects(promoteDistribution(p.args), /newer stable/);
  assert.deepEqual(p.events, []);
});

test('version-binding validation runs only after minisign cryptographic verification', () => {
  const path = join(scratch, 'bound.sig');
  const publicKey = Buffer.from('untrusted comment: public key\nRWTEST\n').toString('base64');
  writeFileSync(
    path,
    Buffer.from(
      'untrusted comment: test\nsignature\ntrusted comment: timestamp:1\tversion:1.2.2\nglobal-signature\n',
    ).toString('base64'),
  );
  let verified = false;
  assert.throws(
    () =>
      verifySignature('artifact', path, publicKey, '1.2.3', () => {
        verified = true;
      }),
    /not bound/,
  );
  assert.equal(verified, true);
  assert.throws(
    () =>
      verifySignature('artifact', path, publicKey, '1.2.3', () => {
        throw new Error('Invalid cryptographic signature');
      }),
    /cryptographic/,
  );
});
