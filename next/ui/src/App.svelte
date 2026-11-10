<script lang="ts">
  import { onMount } from 'svelte';
  import TitleBar from './lib/TitleBar.svelte';
  import Toolbar from './lib/Toolbar.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import DownloadTable from './lib/DownloadTable.svelte';
  import DetailsPanel from './lib/DetailsPanel.svelte';
  import StatusBar from './lib/StatusBar.svelte';
  import AddDownloadDialog from './lib/AddDownloadDialog.svelte';
  import { app } from './lib/state.svelte';
  import { api, isTauri } from './lib/api';
  import { t } from './lib/i18n';
  import Icon from './lib/Icon.svelte';

  let rows = $derived(app.visible);

  onMount(() => {
    void app.init();
    const params = new URLSearchParams(location.search);
    if (params.get('add') === '1' || params.has('url')) app.showAdd = true;
    let dispose: (() => void) | null = null;
    if (isTauri) {
      api
        .onDownloadEvent((event) => app.applyEvent(event))
        .then((off) => {
          dispose = off;
        })
        .catch(() => {});
    }
    return () => {
      dispose?.();
    };
  });

  $effect(() => {
    document.documentElement.dataset.theme = app.theme;
    document.documentElement.lang = app.lang;
    document.documentElement.dir = app.lang === 'ar' ? 'rtl' : 'ltr';
  });

  function onKey(event: KeyboardEvent): void {
    const target = event.target as HTMLElement | null;
    const typing =
      target !== null &&
      (target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable);

    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'n') {
      event.preventDefault();
      app.showAdd = true;
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
      event.preventDefault();
      document.getElementById('majra-search')?.focus();
      return;
    }
    if (typing) return;

    if (event.key === 'Delete' && app.selectedId) {
      event.preventDefault();
      void app.remove(app.selectedId);
    } else if (event.key === ' ' && app.selectedId) {
      event.preventDefault();
      app.togglePause(app.selectedId);
    } else if (event.key === 'Escape') {
      app.showAdd = false;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:low-ram={app.lowRam} style="--row-h: {app.lowRam ? 32 : 38}px">
  <TitleBar />
  <Toolbar />
  <div class="workspace" class:no-details={!app.detailsOpen}>
    <Sidebar />
    <DownloadTable {rows} />
    {#if app.detailsOpen}
      <DetailsPanel />
    {/if}
  </div>
  <StatusBar />
</div>

{#if app.showAdd}
  <AddDownloadDialog />
{/if}

<div class="toasts" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast" data-kind={toast.kind}>
      <Icon name={toast.kind === 'error' ? 'alert' : 'check'} size={14} />
      <span class="toast-text">{toast.text}</span>
      <button
        class="icon-btn"
        type="button"
        aria-label={t('dismiss')}
        onclick={() => (app.toasts = app.toasts.filter((item) => item.id !== toast.id))}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
  {/each}
</div>

<style>
  .app {
    display: grid;
    grid-template-rows: 36px 46px minmax(0, 1fr) 26px;
    height: 100vh;
    background: var(--bg);
  }

  .workspace {
    display: grid;
    grid-template-columns: 208px minmax(0, 1fr) 304px;
    min-height: 0;
  }

  .workspace.no-details {
    grid-template-columns: 208px minmax(0, 1fr);
  }

  .toasts {
    position: fixed;
    inset-block-end: 36px;
    inset-inline-end: 14px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(360px, calc(100vw - 28px));
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 9px 10px;
    border: 1px solid var(--line-strong);
    border-inline-start: 3px solid var(--accent);
    border-radius: var(--r-md);
    background: var(--surface-2);
    box-shadow: var(--shadow-2);
    color: var(--text-2);
    font-size: 11.5px;
    line-height: 1.45;
    pointer-events: auto;
    animation: rise 160ms cubic-bezier(0.2, 0, 0.2, 1);
  }

  .toast[data-kind='error'] {
    border-inline-start-color: var(--danger);
    color: var(--danger);
  }

  .toast-text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .toast .icon-btn {
    width: 20px;
    height: 20px;
    margin-block-start: -2px;
    margin-inline-end: -3px;
  }

  @media (max-width: 1080px) {
    .workspace,
    .workspace.no-details {
      grid-template-columns: 196px minmax(0, 1fr);
    }

    :global(.workspace > .details) {
      display: none;
    }
  }
</style>
