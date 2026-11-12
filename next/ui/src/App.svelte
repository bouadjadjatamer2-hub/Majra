<script lang="ts">
  import { onMount } from 'svelte';
  import TitleBar from './lib/TitleBar.svelte';
  import Toolbar from './lib/Toolbar.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import DownloadTable from './lib/DownloadTable.svelte';
  import DetailsPanel from './lib/DetailsPanel.svelte';
  import StatusBar from './lib/StatusBar.svelte';
  import AddDownloadDialog from './lib/AddDownloadDialog.svelte';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import { app } from './lib/state.svelte';
  import { api, isTauri } from './lib/api';
  import { t } from './lib/i18n';
  import type { DownloadEvent } from './lib/types';
  import Icon from './lib/Icon.svelte';

  let rows = $derived(app.visible);

  let lastClipboard = '';
  let clipboardBusy = false;

  /// Only runs while the setting is on, no dialog is open, and Majra has focus. A copied
  /// link is offered once; copying the same link again does not reopen the dialog.
  async function pollClipboard(): Promise<void> {
    if (
      !isTauri ||
      !app.clipboardWatch ||
      app.showAdd ||
      app.showSettings ||
      clipboardBusy ||
      (typeof document !== 'undefined' && !document.hasFocus())
    ) {
      return;
    }
    clipboardBusy = true;
    try {
      const text = (await api.readClipboard()).trim();
      if (!text || text === lastClipboard) return;
      lastClipboard = text;
      if (text.length > 4096 || !/^https?:\/\/\S+$/i.test(text)) return;
      app.prefillUrl = text;
      app.showAdd = true;
    } catch {
      // An empty or locked clipboard is not an error worth a toast.
    } finally {
      clipboardBusy = false;
    }
  }

  onMount(() => {
    void app.init();
    const params = new URLSearchParams(location.search);
    if (params.get('add') === '1' || params.has('url')) app.showAdd = true;
    const disposers: (() => void)[] = [];
    if (isTauri) {
      // Progress arrives up to four times a second for every active download. Applying
      // each event the moment it lands repaints the list far more often than the screen
      // can show; keep the newest snapshot per row and paint once a frame instead.
      const pending = new Map<string, DownloadEvent>();
      let frame = 0;
      const flush = (): void => {
        frame = 0;
        const events = [...pending.values()];
        pending.clear();
        for (const event of events) app.applyEvent(event);
      };
      api
        .onDownloadEvent((event) => {
          pending.set(event.snapshot.id, event);
          if (!frame) frame = requestAnimationFrame(flush);
        })
        .then((off) => disposers.push(off))
        .catch(() => {});
      disposers.push(() => {
        if (frame) cancelAnimationFrame(frame);
      });
      api
        .onOffer((offer) => app.receiveOffer(offer))
        .then((off) => disposers.push(off))
        .catch(() => {});
      api
        .onScan((result) => {
          const text = `${result.fileName}: ${
            result.status === 'threat'
              ? t('scanThreat')
              : result.status === 'clean'
                ? t('scanClean')
                : result.status === 'unavailable'
                  ? t('scanUnavailable')
                  : t('scanFailed')
          }`;
          app.toast(result.status === 'threat' ? 'error' : 'info', text);
        })
        .then((off) => disposers.push(off))
        .catch(() => {});
      api
        .onShutdown((seconds) => {
          app.toast(
            'info',
            t('shutdownSoon').replace('{s}', String(seconds)),
            {
              label: t('cancelShutdown'),
              run: () => {
                void api
                  .cancelShutdown()
                  .then(() => app.toast('info', t('shutdownCancelled')))
                  .catch(() => app.toast('error', t('shutdownCancelFailed')));
              },
            },
            seconds * 1000 + 2000,
          );
        })
        .then((off) => disposers.push(off))
        .catch(() => {});
      api
        .onSchedule((event) => {
          app.toast(
            'info',
            event.state === 'start' ? t('scheduleStarted') : t('scheduleStopped'),
          );
        })
        .then((off) => disposers.push(off))
        .catch(() => {});
    }
    const timer = window.setInterval(() => void pollClipboard(), 1200);
    return () => {
      disposers.forEach((dispose) => dispose());
      window.clearInterval(timer);
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

    // A dialog owns the keyboard while it is open, so a shortcut cannot stack a second
    // dialog on top of it.
    if (app.showSettings) {
      if (event.key === 'Escape') app.showSettings = false;
      return;
    }
    if (app.showAdd) {
      if (event.key === 'Escape') app.closeAdd();
      return;
    }

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
    if ((event.ctrlKey || event.metaKey) && event.key === ',') {
      event.preventDefault();
      app.showSettings = true;
      return;
    }
    if (typing) return;

    if (event.key === 'Delete' && app.selectedId) {
      event.preventDefault();
      // Ctrl+Delete stops the transfer and clears its partial file; plain Delete only
      // removes the row from the list.
      if (event.ctrlKey || event.metaKey) void app.cancel(app.selectedId);
      else void app.remove(app.selectedId);
    } else if (event.key === 'Enter' && app.selected?.status === 'completed') {
      event.preventDefault();
      void app.openDownload(app.selected);
    } else if (event.key === ' ' && app.selectedId) {
      event.preventDefault();
      app.togglePause(app.selectedId);
    } else if (event.key === 'Escape') {
      if (app.showSettings) app.showSettings = false;
      else app.closeAdd();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:low-ram={app.lowRam} style="--row-h: {app.lowRam ? 28 : 34}px">
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

{#if app.showSettings}
  <SettingsDialog />
{/if}

<div class="toasts" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast" data-kind={toast.kind}>
      <Icon name={toast.kind === 'error' ? 'alert' : 'check'} size={14} />
      <span class="toast-text">{toast.text}</span>
      {#if toast.action}
        <button
          class="toast-action"
          type="button"
          onclick={() => {
            toast.action?.run();
            app.dismissToast(toast.id);
          }}
        >
          {toast.action.label}
        </button>
      {/if}
      <button
        class="icon-btn"
        type="button"
        aria-label={t('dismiss')}
        onclick={() => app.dismissToast(toast.id)}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
  {/each}
</div>

<style>
  .app {
    display: grid;
    grid-template-rows: 34px 40px minmax(0, 1fr) 24px;
    height: 100vh;
    background: var(--bg);
  }

  .workspace {
    display: grid;
    grid-template-columns: 196px minmax(0, 1fr) 296px;
    min-height: 0;
  }

  .workspace.no-details {
    grid-template-columns: 196px minmax(0, 1fr);
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

  .toast-action {
    flex: none;
    padding: 2px 7px;
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    background: var(--surface-3);
    color: var(--text);
    font-size: 10.5px;
    font-weight: 700;
  }

  .toast-action:hover {
    border-color: var(--accent);
    color: var(--accent);
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
