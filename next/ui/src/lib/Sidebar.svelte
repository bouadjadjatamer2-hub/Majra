<script lang="ts">
  import { app, type Filter } from './state.svelte';
  import { t } from './i18n';
  import type { MessageKey } from './i18n';
  import Icon, { type IconName } from './Icon.svelte';

  const primary: { id: Filter; icon: IconName; label: MessageKey }[] = [
    { id: 'all', icon: 'list', label: 'all' },
    { id: 'downloading', icon: 'download', label: 'downloading' },
    { id: 'queued', icon: 'clock', label: 'queued' },
    { id: 'paused', icon: 'pause', label: 'paused' },
    { id: 'completed', icon: 'check', label: 'completed' },
    { id: 'cancelled', icon: 'close', label: 'cancelled' },
    { id: 'error', icon: 'alert', label: 'failed' },
  ];

  const categories: { id: Filter; icon: IconName; label: MessageKey }[] = [
    { id: 'video', icon: 'video', label: 'video' },
    { id: 'audio', icon: 'audio', label: 'audio' },
    { id: 'document', icon: 'document', label: 'documents' },
    { id: 'archive', icon: 'archive', label: 'archives' },
    { id: 'program', icon: 'program', label: 'programs' },
    { id: 'other', icon: 'file', label: 'other' },
  ];
</script>

<aside class="sidebar">
  <nav class="group" aria-label={t('all')}>
    {#each primary as item (item.id)}
      <button
        class="item"
        type="button"
        aria-current={app.filter === item.id ? 'true' : undefined}
        onclick={() => (app.filter = item.id)}
      >
        <Icon name={item.icon} size={15} />
        <span class="label">{t(item.label)}</span>
        <span class="count num">{app.count(item.id)}</span>
      </button>
    {/each}
  </nav>

  <div class="divider"></div>

  <div class="group">
    <div class="group-title">{t('categories')}</div>
    {#each categories as item (item.id)}
      <button
        class="item"
        type="button"
        aria-current={app.filter === item.id ? 'true' : undefined}
        onclick={() => (app.filter = item.id)}
      >
        <Icon name={item.icon} size={15} />
        <span class="label">{t(item.label)}</span>
        <span class="count num">{app.count(item.id)}</span>
      </button>
    {/each}
  </div>

  <div class="spacer"></div>

  <div class="parallel" title={t('parallelHint')}>
    <span class="parallel-title">{t('parallel')}</span>
    <div class="stepper-mini">
      <button
        type="button"
        aria-label={t('parallelFewer')}
        disabled={app.maxActive <= 0}
        onclick={() => app.setMaxActive(app.maxActive - 1)}>-</button>

      <span class="num">{app.maxActive === 0 ? '∞' : app.maxActive}</span>
      <button
        type="button"
        aria-label={t('parallelMore')}
        disabled={app.maxActive >= 64}
        onclick={() => app.setMaxActive(app.maxActive === 0 ? 1 : app.maxActive + 1)}>+</button>
    </div>
  </div>

  <div class="low-ram">
    <div class="low-ram-text">
      <div class="low-ram-title">{t('lowRam')}</div>
      <div class="low-ram-hint">{t('lowRamHint')}</div>
    </div>
    <button
      class="switch"
      type="button"
      role="switch"
      aria-checked={app.lowRam}
      aria-label={t('lowRam')}
      onclick={() => app.toggleLowRam()}
    ></button>
  </div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 6px 5px 8px;
    background: var(--bg-elev);
    border-inline-end: 1px solid var(--line);
    overflow-y: auto;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .group-title {
    padding: 6px 9px 4px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--faint);
  }

  :global(html[lang='ar']) .group-title {
    letter-spacing: 0;
    text-transform: none;
    font-size: 11px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 27px;
    padding-inline: 9px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-2);
    text-align: start;
    transition:
      background var(--motion),
      color var(--motion);
  }

  .item:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .item[aria-current='true'] {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 700;
  }

  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
  }

  .count {
    min-width: 18px;
    text-align: end;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--faint);
  }

  .item[aria-current='true'] .count {
    color: var(--accent);
  }

  .divider {
    height: 1px;
    margin: 7px 10px;
    background: var(--line);
  }

  .spacer {
    flex: 1;
    min-height: 12px;
  }

  .parallel {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: 6px 4px 0;
    padding: 7px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
  }

  .parallel-title {
    font-size: 11.5px;
    font-weight: 700;
    color: var(--text-2);
  }

  .stepper-mini {
    display: flex;
    align-items: center;
    gap: 2px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    overflow: hidden;
  }

  .stepper-mini button {
    width: 22px;
    height: 22px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
  }

  .stepper-mini button:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text);
  }

  .stepper-mini button:disabled {
    color: var(--faint);
    cursor: default;
    opacity: 0.5;
  }

  .stepper-mini .num {
    min-width: 16px;
    text-align: center;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-2);
  }

  .low-ram {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 6px 4px 0;
    padding: 9px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
  }

  .low-ram-text {
    flex: 1;
    min-width: 0;
  }

  .low-ram-title {
    font-size: 11.5px;
    font-weight: 700;
    color: var(--text-2);
  }

  .low-ram-hint {
    margin-block-start: 2px;
    font-size: 10px;
    line-height: 1.3;
    color: var(--faint);
  }
</style>
