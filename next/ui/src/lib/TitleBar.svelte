<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { app } from './state.svelte';
  import { isTauri } from './api';
  import { t } from './i18n';
  import Icon from './Icon.svelte';

  const windowHandle = isTauri ? getCurrentWindow() : null;

  async function minimize(): Promise<void> {
    await windowHandle?.minimize();
  }

  async function toggleMaximize(): Promise<void> {
    await windowHandle?.toggleMaximize();
  }

  async function close(): Promise<void> {
    await windowHandle?.close();
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="mark">
      <svg viewBox="0 0 32 32" width="22" height="22" aria-hidden="true">
        <defs>
          <linearGradient id="majra-mark" x1="0" y1="0" x2="32" y2="32">
            <stop stop-color="#ffc46b" />
            <stop offset="1" stop-color="#dd8618" />
          </linearGradient>
        </defs>
        <rect x="1" y="1" width="30" height="30" rx="9" fill="url(#majra-mark)" />
        <path
          d="M7 11.5c3.6 0 4.6 4.5 9 4.5s5.4-4.5 9-4.5"
          fill="none"
          stroke="#1d1404"
          stroke-width="2.2"
          stroke-linecap="round"
        />
        <path
          d="M7 19c3.6 0 4.6 4.5 9 4.5s5.4-4.5 9-4.5"
          fill="none"
          stroke="#1d1404"
          stroke-opacity=".55"
          stroke-width="2.2"
          stroke-linecap="round"
        />
      </svg>
    </span>
    <span class="name">Majra</span>
    <span class="descriptor">{app.lang === 'ar' ? 'مُدير التحميل' : 'download canal'}</span>
  </div>

  <div class="meta" data-tauri-drag-region>
    {#if !isTauri}
      <span class="chip" data-tone="info">{t('demo')}</span>
    {/if}
  </div>

  <div class="controls">
    <button class="win" type="button" onclick={minimize} aria-label="Minimize" title="Minimize">
      <Icon name="minimize" size={14} />
    </button>
    <button class="win" type="button" onclick={toggleMaximize} aria-label="Maximize" title="Maximize">
      <Icon name="maximize" size={13} />
    </button>
    <button class="win close" type="button" onclick={close} aria-label="Close" title="Close">
      <Icon name="close" size={14} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 34px;
    padding-inline: 10px 0;
    background: var(--bg-elev);
    border-block-end: 1px solid var(--line);
    user-select: none;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .mark {
    display: grid;
    place-items: center;
    filter: drop-shadow(0 1px 1px rgba(0, 0, 0, 0.25));
  }

  .name {
    font-size: 13.5px;
    font-weight: 800;
    letter-spacing: 0.01em;
  }

  .descriptor {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--faint);
    padding-inline-start: 8px;
    border-inline-start: 1px solid var(--line);
  }

  :global(html[lang='ar']) .descriptor {
    letter-spacing: 0;
    text-transform: none;
    font-size: 11px;
  }

  .meta {
    flex: 1;
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .controls {
    display: flex;
    align-self: stretch;
    flex: none;
  }

  .win {
    display: grid;
    place-items: center;
    width: 44px;
    height: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-2);
    transition:
      background var(--motion),
      color var(--motion);
  }

  .win:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .win.close:hover {
    background: #c8383a;
    color: #fff;
  }
</style>
