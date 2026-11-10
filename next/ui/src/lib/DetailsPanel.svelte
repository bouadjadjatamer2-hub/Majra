<script lang="ts">
  import { app } from './state.svelte';
  import { t } from './i18n';
  import type { MessageKey } from './i18n';
  import {
    categoryOf,
    formatBytes,
    formatEta,
    formatSpeed,
    formatWhen,
    percent,
    statusTone,
  } from './format';
  import type { Category, DownloadStatus } from './types';
  import Icon, { type IconName } from './Icon.svelte';

  let download = $derived(app.selected);
  let history = $derived(download ? (app.speedHistory[download.id] ?? []) : []);
  let value = $derived(download ? percent(download) : 0);
  let tone = $derived(download ? statusTone(download.status) : 'info');
  let spark = $derived(build(history));
  let extraction = $derived(download?.extraction ?? null);
  let paths = $derived.by(() => {
    if (!download) return [];
    const map = new Map<
      string,
      { id: string; name: string; speed: number; network: string | null }
    >();
    for (const connection of download.connections) {
      const key = connection.interfaceId || 'auto';
      const known = app.interfaces.find((iface) => iface.id === connection.interfaceId);
      const current = map.get(key);
      if (current) {
        current.speed += connection.speedBytesPerSec;
      } else {
        map.set(key, {
          id: key,
          name: connection.interfaceName || 'Auto',
          speed: connection.speedBytesPerSec,
          network: known?.network ?? null,
        });
      }
    }
    return [...map.values()].sort((a, b) => b.speed - a.speed);
  });
  let sharedNetwork = $derived.by(() => {
    const networks = paths.map((path) => path.network).filter((value): value is string => Boolean(value));
    return networks.length > 1 && new Set(networks).size < networks.length;
  });
  let extractPercent = $derived(
    extraction
      ? extraction.entriesTotal
        ? Math.min(100, (extraction.entriesDone / extraction.entriesTotal) * 100)
        : extraction.bytesTotal
          ? Math.min(100, (extraction.bytesWritten / extraction.bytesTotal) * 100)
          : 0
      : 0,
  );

  const categoryIcon: Record<Category, IconName> = {
    video: 'video',
    audio: 'audio',
    document: 'document',
    archive: 'archive',
    program: 'program',
    other: 'file',
  };

  const statusLabel: Record<DownloadStatus, MessageKey> = {
    downloading: 'downloading',
    queued: 'queued',
    paused: 'paused',
    completed: 'completed',
    cancelled: 'cancelled',
    error: 'failed',
  };

  const extractLabel: Record<string, MessageKey> = {
    pending: 'extraction',
    running: 'extracting',
    paused: 'paused',
    cancelled: 'cancelled',
    done: 'extractDone',
    error: 'extractFailed',
    skipped: 'extractSkipped',
  };

  function build(values: number[], width = 280, height = 46): { line: string; area: string } {
    if (values.length < 2) return { line: '', area: '' };
    const max = Math.max(...values, 1);
    const step = width / (values.length - 1);
    const points = values.map((sample, index) => {
      const x = index * step;
      const y = height - Math.max(1.5, (sample / max) * height);
      return [x, y] as const;
    });
    const line = points
      .map(([x, y], index) => `${index === 0 ? 'M' : 'L'}${x.toFixed(1)} ${y.toFixed(1)}`)
      .join(' ');
    return { line, area: `${line} L ${width} ${height} L 0 ${height} Z` };
  }

  function pathWidth(speed: number): number {
    const total = paths.reduce((sum, path) => sum + path.speed, 0);
    if (total <= 0) return 0;
    return Math.max(2, (speed / total) * 100);
  }

  async function copyLink(): Promise<void> {
    if (!download) return;
    try {
      await navigator.clipboard.writeText(download.url);
      app.toast('info', t('copied'));
    } catch {
      app.toast('error', download.url);
    }
  }
</script>

<aside class="details" aria-label={t('details')}>
  {#if !download}
    <div class="placeholder">
      <span class="placeholder-icon"><Icon name="panel" size={22} /></span>
      <p>{t('emptySelection')}</p>
    </div>
  {:else}
    <header class="head">
      <span class="file-icon" data-cat={categoryOf(download.fileName)}>
        <Icon name={categoryIcon[categoryOf(download.fileName)]} size={16} />
      </span>
      <div class="title-wrap">
        <h2 title={download.fileName}>{download.fileName}</h2>
        <div class="destination" title={download.destination}>{download.destination}</div>
      </div>
      <button
        class="icon-btn"
        type="button"
        aria-label={t('hideDetails')}
        title={t('hideDetails')}
        onclick={() => (app.detailsOpen = false)}
      >
        <Icon name="close" size={14} />
      </button>
    </header>

    <div class="scroll">
      <section class="hero">
        <div class="hero-top">
          <span class="chip" data-tone={tone}>
            <span class="dot" data-tone={tone}></span>
            {t(statusLabel[download.status])}
          </span>
          <span class="hero-pct num">{Math.round(value)}%</span>
        </div>
        <span class="bar hero-bar" data-tone={tone}>
          <span style="width: {value.toFixed(1)}%"></span>
        </span>
        <div class="hero-meta">
          <span class="num">
            {download.totalBytes > 0
              ? `${formatBytes(download.downloadedBytes)} / ${formatBytes(download.totalBytes)}`
              : formatBytes(download.downloadedBytes)}
          </span>
          <span class="num accent">
            {download.status === 'downloading' ? formatSpeed(download.speedBytesPerSec) : formatEta(download.etaSeconds)}
          </span>
        </div>
      </section>

      {#if history.length > 1}
        <section class="sparkline" aria-hidden="true">
          <svg viewBox="0 0 280 46" preserveAspectRatio="none">
            <defs>
              <linearGradient id="spark-fill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stop-color="var(--accent)" stop-opacity="0.28" />
                <stop offset="1" stop-color="var(--accent)" stop-opacity="0" />
              </linearGradient>
            </defs>
            <path d={spark.area} fill="url(#spark-fill)" />
            <path d={spark.line} fill="none" stroke="var(--accent)" stroke-width="1.6" />
          </svg>
        </section>
      {/if}

      <section class="stats">
        <div class="stat">
          <span class="stat-label">{t('speed')}</span>
          <span class="stat-value num">
            {download.status === 'downloading' ? formatSpeed(download.speedBytesPerSec) : '—'}
          </span>
        </div>
        <div class="stat">
          <span class="stat-label">{t('eta')}</span>
          <span class="stat-value num">
            {download.status === 'downloading' ? formatEta(download.etaSeconds) : '—'}
          </span>
        </div>
        <div class="stat">
          <span class="stat-label">{t('size')}</span>
          <span class="stat-value num">
            {download.totalBytes > 0 ? formatBytes(download.totalBytes) : '—'}
          </span>
        </div>
        <div class="stat">
          <span class="stat-label">{t('started')}</span>
          <span class="stat-value">{formatWhen(download.startedAt, app.lang)}</span>
        </div>
      </section>

      {#if download.error}
        <div class="error-box">
          <Icon name="alert" size={14} />
          <span>{download.error}</span>
        </div>
      {/if}

      {#if paths.length > 0}
        <section class="block">
          <div class="section-title">
            <span>{t('paths')}</span>
            <span class="section-count num">{paths.length}</span>
          </div>
          <div class="conns paths">
            {#each paths as path (path.id)}
              <div class="conn">
                <span class="conn-name">
                  <span class="path-label">{path.name}</span>
                  {#if path.network}<span class="conn-net num">{path.network}</span>{/if}
                </span>
                <span class="conn-bar">
                  <span style="width: {pathWidth(path.speed).toFixed(0)}%"></span>
                </span>
                <span class="conn-speed num">{formatSpeed(path.speed)}</span>
              </div>
            {/each}
          </div>
          {#if sharedNetwork}
            <div class="path-warn">
              <Icon name="alert" size={13} />
              <span>{t('sameNetworkHint')}</span>
            </div>
          {/if}
        </section>
      {/if}

      {#if extraction}
        <section class="block">
          <div class="section-title">
            <span>{t('extraction')}</span>
            <span class="section-count">{t(extractLabel[extraction.status] ?? 'extraction')}</span>
          </div>
          {#if extraction.entriesTotal || extraction.bytesTotal}
            <span
              class="bar extract-bar"
              data-tone={extraction.status === 'error' ? 'bad' : 'ok'}
            >
              <span style="width: {extractPercent.toFixed(1)}%"></span>
            </span>
          {/if}
          {#if extraction.status !== 'skipped'}
            <div class="extract-meta">
              <span class="num">
                {extraction.entriesDone}{extraction.entriesTotal
                  ? ` / ${extraction.entriesTotal}`
                  : ''}
                {t('entries')}
              </span>
              <span class="num">
                {formatBytes(extraction.bytesWritten)}{extraction.bytesTotal
                  ? ` / ${formatBytes(extraction.bytesTotal)}`
                  : ''}
              </span>
            </div>
            {#if extraction.current}
              <div class="extract-current" title={extraction.current}>{extraction.current}</div>
            {/if}
            <div class="extract-folder" title={extraction.folder}>
              <span>{t('unpackingTo')}</span>
              <span class="num">{extraction.folder}</span>
            </div>
          {/if}
          {#if extraction.error}
            <div class="error-box">
              <Icon name="alert" size={14} />
              <span>{extraction.error}</span>
            </div>
          {/if}
        </section>
      {/if}

      <section class="block">
        <div class="section-title">
          <span>{t('source')}</span>
        </div>
        <div class="url" title={download.url}>{download.url}</div>
        <button class="btn btn-ghost copy" type="button" onclick={copyLink}>
          <Icon name="copy" size={13} />
          {t('copyLink')}
        </button>
      </section>
    </div>

    <footer class="actions">
      {#if download.status === 'downloading'}
        <button class="btn" type="button" onclick={() => app.pause(download.id)}>
          <Icon name="pause" size={14} />
          {t('pause')}
        </button>
      {:else if download.status === 'paused' || download.status === 'error'}
        <button class="btn btn-primary" type="button" onclick={() => app.resume(download.id)}>
          <Icon name="play" size={14} />
          {t('resume')}
        </button>
      {/if}
      <span class="flex"></span>
      <button class="btn btn-danger" type="button" onclick={() => app.remove(download.id)}>
        <Icon name="trash" size={14} />
        {t('remove')}
      </button>
    </footer>
  {/if}
</aside>

<style>
  .details {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-elev);
    border-inline-start: 1px solid var(--line);
  }

  .placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    color: var(--faint);
    text-align: center;
  }

  .placeholder-icon {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
  }

  .placeholder p {
    margin: 0;
    max-width: 190px;
    font-size: 12px;
    line-height: 1.5;
  }

  .head {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 12px 12px 10px;
    border-block-end: 1px solid var(--line);
    flex: none;
  }

  .file-icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: var(--r-md);
    background: var(--surface-3);
    color: var(--muted);
  }

  .file-icon[data-cat='video'] {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .file-icon[data-cat='audio'] {
    background: var(--jade-soft);
    color: var(--jade);
  }

  .file-icon[data-cat='document'] {
    background: var(--info-soft);
    color: var(--info);
  }

  .file-icon[data-cat='archive'] {
    background: var(--warn-soft);
    color: var(--warn);
  }

  .file-icon[data-cat='program'] {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .title-wrap {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 700;
    color: var(--text);
  }

  :global(html[dir='rtl']) h2 {
    unicode-bidi: plaintext;
    text-align: start;
  }

  .destination {
    margin-block-start: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
    color: var(--faint);
    font-family: var(--font-mono);
    direction: ltr;
    text-align: start;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px;
  }

  .hero {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .hero-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .hero-pct {
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text);
  }

  .hero-bar {
    height: 8px;
  }

  .hero-meta {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 10.5px;
    color: var(--muted);
  }

  .hero-meta .accent {
    color: var(--accent);
    font-weight: 600;
  }

  .sparkline {
    height: 46px;
    margin-block: 12px 4px;
  }

  .sparkline svg {
    width: 100%;
    height: 100%;
    display: block;
  }

  .stats {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    margin-block-start: 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--line);
    overflow: hidden;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    background: var(--surface);
    min-width: 0;
  }

  .stat-label {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--faint);
  }

  :global(html[lang='ar']) .stat-label {
    letter-spacing: 0;
    text-transform: none;
    font-size: 10px;
  }

  .stat-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }

  .error-box {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    margin-block-start: 10px;
    padding: 8px 9px;
    border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
    border-radius: var(--r-sm);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: 11px;
    line-height: 1.45;
  }

  .block {
    margin-block-start: 14px;
  }

  .section-title {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-block-end: 7px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--faint);
  }

  :global(html[lang='ar']) .section-title {
    letter-spacing: 0;
    text-transform: none;
    font-size: 10.5px;
  }

  .section-count {
    padding: 1px 5px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--muted);
    font-size: 9.5px;
  }

  .conns {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .conn {
    display: grid;
    grid-template-columns: 64px 1fr 58px;
    align-items: center;
    gap: 7px;
  }

  .conn-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10.5px;
    color: var(--muted);
  }

  .paths .conn {
    grid-template-columns: 92px 1fr 58px;
  }

  .paths .conn-name {
    white-space: normal;
    line-height: 1.25;
  }

  .path-label {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .conn-net {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: ltr;
    text-align: start;
    font-family: var(--font-mono);
    font-size: 8.5px;
    color: var(--faint);
  }

  .path-warn {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin-block-start: 7px;
    color: var(--warn);
    font-size: 10px;
    line-height: 1.45;
  }

  .conn-bar {
    height: 4px;
    border-radius: 999px;
    background: var(--surface-3);
    overflow: hidden;
  }

  .conn-bar > span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--jade);
    opacity: 0.85;
  }

  .conn-speed {
    text-align: end;
    font-size: 10px;
    color: var(--faint);
  }

  .url {
    padding: 7px 8px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: 10px;
    line-height: 1.45;
    word-break: break-all;
    direction: ltr;
    user-select: text;
  }

  .copy {
    margin-block-start: 6px;
    height: 26px;
    font-size: 11px;
  }

  .extract-bar {
    height: 5px;
    margin-block-end: 7px;
  }

  .extract-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 10px;
    color: var(--muted);
  }

  .extract-current,
  .extract-folder {
    margin-block-start: 5px;
    overflow: hidden;
    white-space: nowrap;
    font-size: 10px;
    color: var(--faint);
  }

  .extract-current {
    font-family: var(--font-mono);
    direction: ltr;
    text-align: start;
    text-overflow: ellipsis;
  }

  .extract-folder {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .extract-folder .num {
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--font-mono);
    direction: ltr;
  }

  .extract-folder > span:first-child {
    flex: none;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-block-start: 1px solid var(--line);
    flex: none;
  }

  .flex {
    flex: 1;
  }
</style>
