<script lang="ts">
  import { app } from './state.svelte';
  import { t } from './i18n';
  import Icon from './Icon.svelte';

  let canPause = $derived(
    app.selected?.status === 'downloading' || app.selected?.status === 'queued',
  );
  let canResume = $derived(
    app.selected?.status === 'paused' || app.selected?.status === 'error',
  );
  let canRemove = $derived(app.selected !== null);
  let canCancel = $derived(
    app.selected !== null &&
      ['downloading', 'queued', 'paused', 'error'].includes(app.selected.status),
  );
</script>

<div class="toolbar">
  <button class="btn btn-primary" type="button" onclick={() => (app.showAdd = true)}>
    <Icon name="plus" size={15} stroke={2.2} />
    <span class="label">{t('add')}</span>
    <kbd>Ctrl N</kbd>
  </button>

  <span class="sep" aria-hidden="true"></span>

  <button
    class="btn btn-ghost"
    type="button"
    disabled={!canPause}
    onclick={() => app.selected && app.pause(app.selected.id)}
    title={t('pause')}
  >
    <Icon name="pause" size={14} />
    <span class="label">{t('pause')}</span>
  </button>

  <button
    class="btn btn-ghost"
    type="button"
    disabled={!canResume}
    onclick={() => app.selected && app.resume(app.selected.id)}
    title={t('resume')}
  >
    <Icon name="play" size={14} />
    <span class="label">{t('resume')}</span>
  </button>

  <button
    class="btn btn-ghost"
    type="button"
    disabled={app.pausedCount === 0}
    onclick={() => app.resumeAll()}
    title={t('resumeAll')}
  >
    <Icon name="refresh" size={14} />
    <span class="label">{t('resumeAll')}</span>
  </button>

  <button
    class="btn btn-ghost"
    type="button"
    disabled={!canCancel}
    onclick={() => app.selected && app.cancel(app.selected.id)}
    title={t('cancelHint')}
  >
    <Icon name="close" size={14} />
    <span class="label">{t('cancel')}</span>
  </button>

  <button
    class="btn btn-ghost"
    type="button"
    disabled={!canRemove}
    onclick={() => app.selected && app.remove(app.selected.id)}
    title={t('remove')}
  >
    <Icon name="trash" size={14} />
    <span class="label">{t('remove')}</span>
  </button>

  <div class="search">
    <Icon name="search" size={14} />
    <input
      id="majra-search"
      class="search-input"
      type="search"
      placeholder={t('search')}
      aria-label={t('search')}
      bind:value={app.query}
      spellcheck="false"
    />
    {#if app.query}
      <button
        class="clear"
        type="button"
        aria-label="Clear search"
        onclick={() => (app.query = '')}
      >
        <Icon name="close" size={12} />
      </button>
    {/if}
  </div>

  <span class="spacer"></span>

  <button
    class="btn btn-ghost"
    type="button"
    aria-pressed={app.showSettings}
    onclick={() => (app.showSettings = true)}
    title={`${t('settings')} (Ctrl+,)`}
  >
    <Icon name="gear" size={15} />
    <span class="label">{t('settings')}</span>
  </button>

  <button
    class="btn btn-ghost details-toggle"
    type="button"
    aria-pressed={app.detailsOpen}
    onclick={() => (app.detailsOpen = !app.detailsOpen)}
    title={app.detailsOpen ? t('hideDetails') : t('showDetails')}
  >
    <Icon name="panel" size={15} />
    <span class="label">{t('details')}</span>
  </button>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 40px;
    padding-inline: 8px;
    background: var(--surface);
    border-block-end: 1px solid var(--line);
  }

  .label {
    white-space: nowrap;
  }

  kbd {
    margin-inline-start: 4px;
    padding: 1px 5px;
    border: 1px solid color-mix(in srgb, var(--accent-ink) 22%, transparent);
    border-radius: var(--r-xs);
    background: color-mix(in srgb, var(--accent-ink) 10%, transparent);
    font-family: var(--font-mono);
    font-size: 9.5px;
    font-weight: 600;
    opacity: 0.72;
  }

  .sep {
    width: 1px;
    height: 20px;
    margin-inline: 6px;
    background: var(--line);
  }

  .search {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    width: 210px;
    height: 28px;
    margin-inline-start: 8px;
    padding-inline: 8px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    color: var(--faint);
    transition:
      border-color var(--motion),
      box-shadow var(--motion);
  }

  .search:focus-within {
    border-color: var(--focus);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--focus) 18%, transparent);
    color: var(--muted);
  }

  .search-input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 12.5px;
    user-select: text;
  }

  .search-input::placeholder {
    color: var(--faint);
  }

  .search-input::-webkit-search-cancel-button {
    display: none;
  }

  .clear {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--surface-3);
    color: var(--muted);
  }

  .clear:hover {
    color: var(--text);
  }

  .spacer {
    flex: 1;
  }

  .details-toggle[aria-pressed='true'] {
    color: var(--accent);
    background: var(--accent-soft);
  }

  @media (max-width: 1080px) {
    .label,
    kbd {
      display: none;
    }

    .search {
      width: 180px;
    }
  }
</style>
