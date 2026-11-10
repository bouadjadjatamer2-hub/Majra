<script lang="ts">
  import { app } from './state.svelte';
  import { t } from './i18n';
  import { formatSpeed } from './format';
  import Icon from './Icon.svelte';

  let active = $derived(app.active.length);
</script>

<footer class="statusbar">
  <div class="side left">
    <span class="live" class:on={active > 0}></span>
    <span class="num strong">{formatSpeed(app.totalSpeed)}</span>
    <span class="muted">
      · {active} {t('active')} · {app.downloads.length} {t('total')}
    </span>
  </div>

  <div class="side center">
    {#if app.lowRam}
      <span class="chip" data-tone="ok">{t('lowRam')}</span>
    {/if}
  </div>

  <div class="side right">
    <button
      class="tool"
      type="button"
      title={t('theme')}
      aria-label={t('theme')}
      onclick={() => app.toggleTheme()}
    >
      <Icon name={app.theme === 'dark' ? 'sun' : 'moon'} size={13} />
    </button>
    <button
      class="tool lang"
      type="button"
      title={t('language')}
      aria-label={t('language')}
      onclick={() => app.toggleLang()}
    >
      <Icon name="globe" size={13} />
      <span>{app.lang === 'en' ? 'العربية' : 'English'}</span>
    </button>
    <span class="version num">{t('version')}</span>
  </div>
</footer>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 26px;
    padding-inline: 10px;
    border-block-start: 1px solid var(--line);
    background: var(--bg-elev);
    color: var(--muted);
    font-size: 10.5px;
  }

  .side {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    white-space: nowrap;
  }

  .left {
    flex: 1;
    overflow: hidden;
  }

  .center {
    flex: none;
  }

  .right {
    flex: none;
    gap: 4px;
  }

  .live {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--faint);
  }

  .live.on {
    background: var(--jade);
    box-shadow: 0 0 0 3px var(--jade-soft);
  }

  .strong {
    color: var(--text-2);
    font-weight: 600;
  }

  .muted {
    color: var(--faint);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tool {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding-inline: 6px;
    border: 0;
    border-radius: var(--r-xs);
    background: transparent;
    color: var(--muted);
    font-size: 10px;
  }

  .tool:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .version {
    padding-inline-start: 8px;
    border-inline-start: 1px solid var(--line);
    color: var(--faint);
    font-size: 9.5px;
  }

  @media (max-width: 900px) {
    .lang span {
      display: none;
    }

    .center {
      display: none;
    }
  }
</style>
