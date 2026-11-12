<script lang="ts">
  import { app } from './state.svelte';
  import { t } from './i18n';
  import type { MessageKey } from './i18n';
  import {
    categoryOf,
    formatBytes,
    formatEta,
    formatSpeed,
    hostOf,
    percent,
    statusTone,
  } from './format';
  import type { Category, DownloadSnapshot, DownloadStatus } from './types';
  import Icon, { type IconName } from './Icon.svelte';

  let { rows }: { rows: DownloadSnapshot[] } = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(560);

  const OVERSCAN = 6;
  let rowH = $derived(app.lowRam ? 28 : 34);
  let start = $derived(Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN));
  let end = $derived(
    Math.min(rows.length, Math.ceil((scrollTop + viewportH) / rowH) + OVERSCAN),
  );
  let slice = $derived(rows.slice(start, end));

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

  $effect(() => {
    const node = viewport;
    if (!node) return;
    viewportH = node.clientHeight;
    const observer = new ResizeObserver(() => {
      viewportH = node.clientHeight;
    });
    observer.observe(node);
    return () => observer.disconnect();
  });

  function onScroll(event: Event): void {
    scrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }

  function select(id: string): void {
    app.selectedId = id;
  }

  function onRowKey(event: KeyboardEvent, id: string): void {
    if (event.key === 'Enter') {
      app.selectedId = id;
    } else if (event.key === ' ') {
      event.preventDefault();
      // The window-level shortcut also watches Space; stop it here so one press is one
      // pause/resume, not a toggle that immediately undoes itself.
      event.stopPropagation();
      app.togglePause(id);
    }
  }

  function interfacesOf(download: DownloadSnapshot): string[] {
    const seen = new Set<string>();
    for (const connection of download.connections) {
      if (connection.interfaceName) seen.add(connection.interfaceName);
    }
    return [...seen];
  }

  /// A downloading row that has nothing to show yet is not stuck: it is talking to the
  /// server. Naming that phase is what keeps a big file's slow start from looking frozen.
  function phaseOf(download: DownloadSnapshot): MessageKey | null {
    if (download.status !== 'downloading') return null;
    if (download.phase === 'starting') return 'connecting';
    if (download.phase === 'reconnecting') return 'reconnecting';
    if (download.phase === 'finalizing') return 'finalizing';
    return null;
  }
</script>

<section class="table" aria-label={t('all')}>
  <div class="head" role="row">
    <div class="col-file" role="columnheader">{t('file')}</div>
    <div class="col-size" role="columnheader">{t('sizeDownloaded')}</div>
    <div class="col-progress" role="columnheader">{t('progress')}</div>
    <div class="col-speed" role="columnheader">{t('speed')}</div>
    <div class="col-eta" role="columnheader">{t('eta')}</div>
    <div class="col-status" role="columnheader">{t('status')}</div>
    <div class="col-net" role="columnheader">{t('network')}</div>
  </div>

  <div class="viewport" bind:this={viewport} onscroll={onScroll}>
    {#if rows.length === 0}
      <div class="empty">
        <span class="empty-icon"><Icon name="download" size={26} /></span>
        <div class="empty-title">{t('noDownloads')}</div>
        <p class="empty-hint">{t('noDownloadsHint')}</p>
        <button class="btn btn-primary" type="button" onclick={() => (app.showAdd = true)}>
          <Icon name="plus" size={14} stroke={2.2} />
          {t('add')}
        </button>
      </div>
    {:else}
      <div class="canvas" style="height: {rows.length * rowH}px">
        {#each slice as download, index (download.id)}
          {@const tone = statusTone(download.status)}
          {@const value = percent(download)}
          {@const nets = interfacesOf(download)}
          {@const phase = phaseOf(download)}
          <div
            class="row"
            class:selected={app.selectedId === download.id}
            role="row"
            tabindex="0"
            style="transform: translateY({(start + index) * rowH}px)"
            onclick={() => select(download.id)}
            onkeydown={(event) => onRowKey(event, download.id)}
            ondblclick={() =>
              download.status === 'completed'
                ? app.openDownload(download)
                : app.togglePause(download.id)}
            aria-selected={app.selectedId === download.id}
          >
            <div class="cell cell-file" role="cell" title={`${download.fileName}\n${hostOf(download.url)}`}>
              <span class="file-icon" data-cat={categoryOf(download.fileName)}>
                <Icon name={categoryIcon[categoryOf(download.fileName)]} size={13} />
              </span>
              <span class="file-text">
                <span class="file-name">{download.fileName}</span>
              </span>
            </div>

            <div
              class="cell num size-cell"
              role="cell"
              title={`${formatBytes(download.downloadedBytes)} / ${
                download.totalBytes > 0 ? formatBytes(download.totalBytes) : '?'
              }`}
            >
              <span>{formatBytes(download.downloadedBytes)}</span>
              {#if download.totalBytes > 0}
                <span class="size-total">/ {formatBytes(download.totalBytes)}</span>
              {/if}
            </div>

            <div class="cell progress-cell" role="cell">
              <span class="bar" data-tone={tone}>
                <span style="width: {value.toFixed(1)}%"></span>
              </span>
              <span class="pct num">{Math.round(value)}%</span>
            </div>

            <div class="cell num speed" role="cell">
              {download.status === 'downloading' ? formatSpeed(download.speedBytesPerSec) : '—'}
            </div>

            <div class="cell num" role="cell">
              {download.status === 'downloading' ? formatEta(download.etaSeconds) : '—'}
            </div>

            <div class="cell" role="cell">
              <span class="chip" data-tone={tone}>
                <span class="dot" data-tone={tone}></span>
                {phase ? t(phase) : t(statusLabel[download.status])}
              </span>
            </div>

            <div class="cell cell-net" role="cell">
              {#if nets.length > 0}
                <span class="net">{nets[0]}{nets.length > 1 ? ` +${nets.length - 1}` : ''}</span>
              {:else}
                <span class="net muted">—</span>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .table {
    --cols: minmax(140px, 1fr) 116px 110px 74px 56px 90px 74px;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--surface);
  }

  .head {
    display: grid;
    grid-template-columns: var(--cols);
    gap: 10px;
    align-items: center;
    height: 26px;
    padding-inline: 10px 12px;
    border-block-end: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--faint);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    flex: none;
  }

  :global(html[lang='ar']) .head {
    letter-spacing: 0;
    text-transform: none;
    font-size: 10.5px;
  }

  .viewport {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .canvas {
    position: relative;
  }

  .row {
    position: absolute;
    inset-inline: 0;
    display: grid;
    grid-template-columns: var(--cols);
    gap: 10px;
    align-items: center;
    height: var(--row-h, 34px);
    padding-inline: 10px 12px;
    border-block-end: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
    border-inline-start: 2px solid transparent;
    color: var(--text-2);
    font-size: 12px;
    cursor: default;
    transition:
      background var(--motion),
      border-color var(--motion);
  }

  .row:hover {
    background: var(--surface-hover);
  }

  .row.selected {
    background: var(--surface-2);
    border-inline-start-color: var(--accent);
  }

  .cell {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .cell-file {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .file-icon {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    flex: none;
    border-radius: var(--r-xs);
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

  .file-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.2;
  }

  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-size: 12.5px;
    font-weight: 600;
  }

  .row.selected .file-name {
    color: var(--text);
  }

  /* Latin file names keep their own direction inside an RTL row, so the ellipsis
     falls on the readable end instead of reordering the path fragments. */
  :global(html[dir='rtl']) .file-name {
    unicode-bidi: plaintext;
    text-align: start;
  }

  .progress-cell {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .progress-cell .bar {
    flex: 1;
    min-width: 20px;
  }

  .pct {
    width: 30px;
    text-align: end;
    font-size: 10.5px;
    color: var(--muted);
  }

  .speed {
    color: var(--text-2);
  }

  /* Downloaded bytes stay bright; the file's full size is the quieter half. */
  .size-cell {
    display: flex;
    justify-content: flex-end;
    gap: 4px;
    font-size: 11px;
  }

  .size-total {
    color: var(--faint);
  }

  .cell .chip {
    max-width: 100%;
    overflow: hidden;
  }

  .net {
    display: inline-block;
    max-width: 100%;
    padding: 2px 6px;
    border: 1px solid var(--line);
    border-radius: var(--r-xs);
    color: var(--muted);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    vertical-align: middle;
  }

  .net.muted {
    border-color: transparent;
    color: var(--faint);
  }

  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px;
    text-align: center;
  }

  .empty-icon {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: var(--surface-2);
    color: var(--faint);
  }

  .empty-title {
    margin-block-start: 4px;
    font-size: 14px;
    font-weight: 700;
    color: var(--text);
  }

  .empty-hint {
    max-width: 330px;
    margin: 0 0 6px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--muted);
  }

  @media (max-width: 1239px) {
    .table {
      --cols: minmax(140px, 1fr) 70px 110px 74px 56px 90px;
    }

    .col-net,
    .cell-net {
      display: none;
    }
  }

  @media (max-width: 1000px) {
    .table {
      --cols: minmax(120px, 1fr) 64px 100px 70px 96px;
    }

    .col-eta,
    .row > .cell:nth-child(5) {
      display: none;
    }
  }
</style>
