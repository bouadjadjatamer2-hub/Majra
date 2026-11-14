<script lang="ts">
  import { app } from './state.svelte';
  import { t } from './i18n';
  import Icon from './Icon.svelte';

  let busy = $derived(
    app.updateState === 'downloading' || app.updateState === 'installing',
  );

  function close(): void {
    if (!busy) app.dismissUpdate();
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape') close();
  }}
/>

<div
  class="overlay"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) close();
  }}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="update-title">
    <header class="dialog-head">
      <div class="title">
        <span class="mark"><Icon name="download" size={15} stroke={2} /></span>
        <div>
          <h2 id="update-title">{t('updateAvailable')}</h2>
          <span class="sub">v{app.updateVersion}</span>
        </div>
      </div>
      <button
        class="icon-btn"
        type="button"
        aria-label={t('close')}
        disabled={busy}
        onclick={close}
      >
        <Icon name="close" size={14} />
      </button>
    </header>

    <div class="dialog-body">
      <p class="line">{t('updateCurrent').replace('{v}', app.version)}</p>

      {#if app.updateNotes}
        <div class="notes">
          <b>{t('updateNotes')}</b>
          <pre>{app.updateNotes.slice(0, 4000)}</pre>
        </div>
      {/if}

      {#if busy}
        <div
          class="progress"
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={app.updateProgress}
          aria-label={t('updateDownloading')}
        >
          <span style="width: {app.updateProgress}%"></span>
        </div>
        <span class="state">
          {#if app.updateState === 'installing'}
            {t('updateInstalling')}
          {:else}
            {t('updateDownloading')} · {t('updateProgress').replace('{p}', String(app.updateProgress))}
          {/if}
        </span>
      {:else if app.updateState === 'error'}
        <div class="error" role="alert">
          <Icon name="alert" size={13} />
          <span>{t('updateFailed')} {app.updateError}</span>
        </div>
      {:else}
        <span class="state hint">{t('updateAvailableHint')}</span>
      {/if}
    </div>

    <footer class="dialog-foot">
      <span class="flex"></span>
      <button class="btn btn-ghost" type="button" disabled={busy} onclick={close}>
        {t('updateLater')}
      </button>
      <button
        class="btn btn-primary"
        type="button"
        disabled={busy}
        onclick={() => void app.installUpdate()}
      >
        <Icon name="download" size={13} />
        {app.updateState === 'installing' ? t('updateInstallingShort') : t('updateNow')}
      </button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 58;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--overlay);
    animation: fade-in 120ms ease-out;
  }

  .dialog {
    display: flex;
    flex-direction: column;
    width: min(520px, 100%);
    max-height: min(82vh, 620px);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-lg);
    background: var(--surface);
    box-shadow: var(--shadow-2);
    animation: rise 160ms cubic-bezier(0.2, 0, 0.2, 1);
    overflow: hidden;
  }

  .dialog-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 12px 14px;
    border-block-end: 1px solid var(--line);
    background: var(--bg-elev);
    flex: none;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .mark {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: var(--r-sm);
    background: var(--accent-soft);
    color: var(--accent);
  }

  h2 {
    margin: 0;
    font-size: 13.5px;
    font-weight: 700;
  }

  .sub {
    color: var(--text-3);
    font-size: 11px;
    font-family: var(--mono, monospace);
  }

  .dialog-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    overflow-y: auto;
    min-height: 0;
  }

  .line {
    margin: 0;
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.5;
  }

  .notes {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 9px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--surface-2);
  }

  .notes b {
    font-size: 11.5px;
  }

  .notes pre {
    margin: 0;
    max-height: 170px;
    overflow-y: auto;
    color: var(--text-3);
    font-family: inherit;
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .state {
    color: var(--text-3);
    font-size: 11px;
  }

  .progress {
    height: 6px;
    border-radius: 99px;
    background: var(--surface-3);
    overflow: hidden;
  }

  .progress span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width 140ms linear;
  }

  .error {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    color: var(--danger);
    font-size: 11px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .dialog-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 11px 14px;
    border-block-start: 1px solid var(--line);
    background: var(--bg-elev);
    flex: none;
  }

  .flex {
    flex: 1;
  }
</style>
