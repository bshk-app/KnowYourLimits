import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { assertReleaseTag, readReleaseInfo, root, verifyMacOSManifest } from './release-common.mjs';
import { restoreRelease } from './restore-release.mjs';
import { sha256, verifyUpdaterArtifacts } from './updater-artifacts.mjs';

export function zamokClient(config, key, fetcher = fetch) {
  if (!key) throw new Error('Missing ZAMOK_API_KEY.');
  return async (path, method = 'GET', body) => {
    const response = await fetcher(`${config.endpoint}${path}`, {
      method,
      headers: { 'Content-Type': 'application/json', 'x-api-key': key },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      signal: AbortSignal.timeout(60000),
    });
    if (!response.ok)
      throw new Error(`Zamok ${method} ${path.split('?')[0]} returned HTTP ${response.status}.`);
    return response.json();
  };
}

export async function findRelease(api, config, version) {
  const q = {
    where: { product: { slug: config.product }, version, channel: config.channel },
    select: {
      id: true,
      shortVersion: true,
      status: true,
      minPlan: true,
      artifacts: {
        select: {
          platform: true,
          variant: true,
          signature: true,
          enclosureUrl: true,
          enclosureLength: true,
        },
      },
    },
  };
  const result = await api(
    `/api/model/release/findMany?q=${encodeURIComponent(JSON.stringify(q))}`,
  );
  if (!Array.isArray(result.data) || result.data.length > 1)
    throw new Error('Ambiguous Zamok release.');
  return result.data[0];
}

export async function verifyHostedRelease(release, updates, version, fetcher = fetch) {
  if (
    release.shortVersion !== version ||
    release.minPlan !== null ||
    release.artifacts.length !== updates.length
  ) {
    throw new Error('Zamok release must be public and contain exactly both updater platforms.');
  }
  for (const update of updates) {
    const remote = release.artifacts.find(
      (item) => item.platform === update.platform && item.variant === 'online',
    );
    const signature = readFileSync(update.signature, 'utf8').trim();
    if (
      !remote ||
      remote.signature !== signature ||
      String(remote.enclosureLength) !== String(update.metadata.artifact.sizeBytes)
    ) {
      throw new Error(`Zamok updater metadata mismatch: ${update.platform}`);
    }
    if (new URL(remote.enclosureUrl).protocol !== 'https:')
      throw new Error('Updater downloads must use HTTPS.');
    const response = await fetcher(remote.enclosureUrl, { signal: AbortSignal.timeout(60000) });
    if (
      !response.ok ||
      sha256(Buffer.from(await response.arrayBuffer())) !== update.metadata.artifact.sha256
    ) {
      throw new Error(`Zamok hosted SHA-256 mismatch: ${update.platform}`);
    }
  }
}

export async function stageDistribution({ info, config, api, updates, fetcher = fetch }) {
  let release = await findRelease(api, config, info.version);
  if (release && release.status !== 'DRAFT')
    throw new Error('Stage requires a draft; published versions are immutable.');
  if (!release) {
    const artifacts = [];
    for (const update of updates) {
      const bytes = readFileSync(update.artifact);
      const upload = await api(`/api/products/${config.product}/releases/upload-url`, 'POST', {
        filename: basename(update.artifact),
        contentType: 'application/octet-stream',
        shortVersion: info.version,
      });
      const response = await fetcher(upload.presignedUrl, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: bytes,
        signal: AbortSignal.timeout(120000),
      });
      if (!response.ok) throw new Error(`Zamok storage upload failed: ${update.platform}`);
      artifacts.push({
        uploadId: upload.uploadId,
        platform: update.platform,
        variant: 'online',
        signature: readFileSync(update.signature, 'utf8').trim(),
      });
    }
    await api(`/api/products/${config.product}/releases/tauri/finalize`, 'POST', {
      version: info.version,
      shortVersion: info.version,
      channel: config.channel,
      publish: false,
      minPlan: null,
      releaseNotesMarkdown: info.notesMarkdown ?? readFileSync(info.notes, 'utf8'),
      artifacts,
    });
    release = await findRelease(api, config, info.version);
  }
  await verifyHostedRelease(release, updates, info.version, fetcher);
  console.log(`Verified Zamok draft ${config.product} ${info.version} (${release.id}).`);
  return release;
}

export async function promoteDistribution({
  info,
  tag,
  config,
  api,
  updates,
  gh,
  cask,
  fetcher = fetch,
}) {
  const release = await findRelease(api, config, info.version);
  if (!release || !['DRAFT', 'PUBLISHED'].includes(release.status))
    throw new Error('No promotable Zamok draft.');
  await verifyHostedRelease(release, updates, info.version, fetcher);
  const latest = await api(
    '/api/model/release/findMany?q=' +
      encodeURIComponent(
        JSON.stringify({
          where: {
            product: { slug: config.product },
            channel: config.channel,
            status: 'PUBLISHED',
          },
          select: { shortVersion: true },
        }),
      ),
  );
  const numbers = (version) => version.split('.').map(Number);
  if (
    latest.data.some((item) =>
      numbers(item.shortVersion).some(
        (part, i) =>
          part > numbers(info.version)[i] &&
          numbers(item.shortVersion)
            .slice(0, i)
            .every((earlier, j) => earlier === numbers(info.version)[j]),
      ),
    )
  ) {
    throw new Error('A newer stable release exists; refusing to roll Homebrew or the feed back.');
  }
  const github = JSON.parse(
    gh('release', 'view', tag, '--repo', config.repository, '--json', 'isDraft'),
  );
  // All GitHub assets were downloaded and verified by restore-release before this call.
  if (github.isDraft)
    gh('release', 'edit', tag, '--repo', config.repository, '--draft=false', '--latest');
  await cask();
  if (release.status === 'DRAFT') {
    const publishedAt = new Date().toISOString();
    // The dynamic Tauri feed reads Release directly. The Sparkle PATCH route also
    // removes CLI-managed casks with no server CaskConfig; use the same model API
    // as Zamok's update_release MCP tool for this Tauri-only product.
    await api('/api/model/release/update', 'PUT', {
      where: { id: release.id },
      data: { status: 'PUBLISHED', publishedAt, pubDate: publishedAt },
      select: { id: true, status: true },
    });
  }
  for (const update of updates) {
    const [target, arch] = update.platform.split('-');
    const endpoint = `${config.endpoint}/api/products/${config.product}/updater/${target}/${arch}`;
    const available = await fetcher(`${endpoint}/0.0.0?channel=stable&verify=${Date.now()}`, {
      signal: AbortSignal.timeout(30000),
    });
    if (!available.ok) throw new Error(`Public updater endpoint failed: ${update.platform}`);
    const payload = await available.json();
    const artifact = release.artifacts.find((item) => item.platform === update.platform);
    if (
      payload.version !== info.version ||
      payload.signature !== artifact.signature ||
      payload.url !== artifact.enclosureUrl
    ) {
      throw new Error(`Public updater payload mismatch: ${update.platform}`);
    }
    const current = await fetcher(
      `${endpoint}/${info.version}?channel=stable&verify=${Date.now()}`,
      { signal: AbortSignal.timeout(30000) },
    );
    if (current.status !== 204)
      throw new Error(`Current version must return HTTP 204: ${update.platform}`);
  }
  console.log(`Published ${tag}: GitHub, Homebrew and Zamok stable updates verified.`);
}

async function main() {
  const mode = process.argv[2];
  if (process.argv.length !== 3 || !['--stage', '--promote'].includes(mode))
    throw new Error('Usage: node scripts/zamok-distribution.mjs --stage|--promote');
  const info = readReleaseInfo();
  const tag = assertReleaseTag(info, process.env.KYL_RELEASE_TAG);
  const config = JSON.parse(readFileSync(join(root, 'packaging/zamok.json'), 'utf8'));
  const api = zamokClient(config, process.env.ZAMOK_API_KEY);
  const gh = (...args) =>
    execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  if (mode === '--promote') {
    if (!process.env.TAP_GITHUB_TOKEN) throw new Error('Missing TAP_GITHUB_TOKEN.');
    const git = (...args) =>
      execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
    restoreRelease({ info, tag, repository: config.repository, gh, git });
  }
  const updates = verifyUpdaterArtifacts(info);
  if (mode === '--stage') return stageDistribution({ info, config, api, updates });
  return promoteDistribution({
    info,
    tag,
    config,
    api,
    updates,
    gh,
    cask: async () => {
      if (!process.env.TAP_GITHUB_TOKEN) throw new Error('Missing TAP_GITHUB_TOKEN.');
      const macos = verifyMacOSManifest(info);
      const env = { ...process.env, GITHUB_TOKEN: process.env.TAP_GITHUB_TOKEN };
      for (const name of [
        'GH_TOKEN',
        'TAP_GITHUB_TOKEN',
        'ZAMOK_API_KEY',
        'TAURI_SIGNING_PRIVATE_KEY',
        'TAURI_SIGNING_PRIVATE_KEY_PASSWORD',
      ])
        delete env[name];
      execFileSync(
        'zamokctl',
        [
          'cask',
          '--manifest',
          macos.path,
          '--store',
          'url',
          '--url',
          `https://github.com/${config.repository}/releases/download/${tag}/${macos.manifest.artifact}`,
          '--tap',
          config.tap,
          '--metadata',
          join(root, 'packaging/cask.json'),
          '--audit-online',
        ],
        { env, stdio: 'inherit' },
      );
    },
  });
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await main();
