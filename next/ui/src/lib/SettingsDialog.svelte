<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { app } from './state.svelte';
  import { api, isTauri } from './api';
  import { t } from './i18n';
  import type { AppSettings, InterfaceInfo } from './types';
  import Icon, { type IconName } from './Icon.svelte';

  type Tab = 'general' | 'downloads' | 'network' | 'browser' | 'advanced';

  const tabs: { id: Tab; icon: IconName; label: Parameters<typeof t>[0] }[] = [
    { id: 'general', icon: 'sun', label: 'settingsGeneral' },
    { id: 'downloads', icon: 'download', label: 'settingsDownloads' },
    { id: 'network', icon: 'wifi', label: 'settingsNetwork' },
    { id: 'browser', icon: 'globe', label: 'settingsBrowser' },
    { id: 'advanced', icon: 'panel', label: 'settingsAdvanced' },
  ];

  const defaults: AppSettings = {
    defaultDestination: '',
    defaultConnections: 8,
    maxActive: 3,
    extractByDefault: false,
    closeToTray: true,
    startMinimized: false,
    confirmRemove: true,
    startAtLogin: false,
    shutdownWhenDone: false,
    scanWithDefender: false,
    clipboardWatch: false,
    scheduleEnabled: false,
    scheduleStart: '02:00',
    scheduleStop: '07:00',
    scheduleStopAction: 'pause',
    proxyMode: 'system',
    proxyUrl: '',
    proxyUsername: '',
    proxyPassword: '',
  };

  let tab = $state<Tab>('general');
  let draft = $state<AppSettings>(app.currentSettings());
  let theme = $state<'dark' | 'light'>(app.theme);
  let lang = $state<'en' | 'ar'>(app.lang);
  let lowRam = $state(app.lowRam);
  let saving = $state(false);
  let saved = $state(false);
  let clearing = $state(false);
  let bridgeError = $state<string | null>(null);

  let finishedCount = $derived(
    app.downloads.filter((download) =>
      ['completed', 'cancelled', 'error'].includes(download.status),
    ).length,
  );

  onMount(() => {
    void app.refreshInterfaces();
    void refreshBridge();
  });

  async function refreshBridge(): Promise<void> {
    try {
      app.bridge = await api.bridgeStatus();
      bridgeError = null;
    } catch (error) {
      bridgeError = String(error);
    }
  }

  async function browseFolder(): Promise<void> {
    if (!isTauri) {
      const chosen = window.prompt(t('settingsFolderPrompt'), draft.defaultDestination);
      if (chosen !== null) draft.defaultDestination = chosen.trim();
      return;
    }
    try {
      const chosen = await open({
        directory: true,
        multiple: false,
        defaultPath: draft.defaultDestination || undefined,
      });
      if (typeof chosen === 'string') draft.defaultDestination = chosen;
    } catch (error) {
      app.toast('error', String(error));
    }
  }

  async function save(): Promise<void> {
    saving = true;
    saved = false;
    const ok = await app.saveSettings(draft);
    if (ok) {
      app.setTheme(theme);
      app.setLang(lang);
      app.setLowRam(lowRam);
      saved = true;
      setTimeout(() => (saved = false), 2200);
    }
    saving = false;
  }

  async function clearHistory(): Promise<void> {
    clearing = true;
    const count = await app.clearHistory();
    app.toast('info', count > 0 ? t('historyCleared') : t('historyEmpty'));
    clearing = false;
  }

  function resetDraft(): void {
    draft = { ...defaults };
    theme = 'dark';
    lang = 'en';
    lowRam = true;
  }

  async function copyBridge(): Promise<void> {
    if (!app.bridge) return;
    try {
      await navigator.clipboard.writeText(`127.0.0.1:${app.bridge.port}`);
      app.toast('info', t('copied'));
    } catch {
      app.toast('error', t('copyFailed'));
    }
  }

  function test(iface: InterfaceInfo): void {
    void app.testInterface(iface);
  }

  function close(): void {
    app.showSettings = false;
  }

  function statusOf(iface: InterfaceInfo): string {
    const test = app.interfaceTests[iface.id];
    if (!test) return iface.usable ? t('interfaceReady') : t('interfaceNoRoute');
    if (test.state === 'testing') return t('testingInterface');
    if (test.state === 'ok') return `${test.latency ?? 0} ms`;
    return test.error ?? t('interfaceFailed');
  }

  function toneOf(iface: InterfaceInfo): string {
    const test = app.interfaceTests[iface.id];
    if (test?.state === 'fail') return 'bad';
    if (test?.state === 'ok') return 'ok';
    return iface.usable ? 'ok' : 'bad';
  }

  function kindIcon(kind: InterfaceInfo['kind']): IconName {
    if (kind === 'wifi') return 'wifi';
    if (kind === 'ethernet') return 'ethernet';
    if (kind === 'usb') return 'usb';
    return 'globe';
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape') close();
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
      event.preventDefault();
      void save();
    }
  }}
/>

<div
  class="overlay"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) close();
  }}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
    <header class="dialog-head">
      <div class="title">
        <span class="mark"><Icon name="gear" size={15} stroke={2} /></span>
        <div>
          <h2 id="settings-title">{t('settings')}</h2>
          <span class="sub">{t('settingsSub')}</span>
        </div>
      </div>
      <button class="icon-btn" type="button" aria-label={t('close')} onclick={close}>
        <Icon name="close" size={14} />
      </button>
    </header>

    <div class="settings-body">
      <nav class="tabs" aria-label={t('settings')}>
        {#each tabs as item (item.id)}
          <button
            class="tab"
            type="button"
            aria-current={tab === item.id ? 'true' : undefined}
            onclick={() => (tab = item.id)}
          >
            <Icon name={item.icon} size={15} />
            <span>{t(item.label)}</span>
          </button>
        {/each}
        <span class="tab-spacer"></span>
        <div class="version num">v{t('version')}</div>
      </nav>

      <div class="content">
        {#if tab === 'general'}
          <section class="section">
            <div class="section-head">
              <h3>{t('settingsAppearance')}</h3>
              <span class="hint">{t('settingsAppearanceHint')}</span>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('language')}</b>
                <span class="hint">{t('settingsLanguageHint')}</span>
              </div>
              <select class="select compact" bind:value={lang}>
                <option value="en">English</option>
                <option value="ar">العربية</option>
              </select>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('theme')}</b>
                <span class="hint">{t('settingsThemeHint')}</span>
              </div>
              <select class="select compact" bind:value={theme}>
                <option value="dark">{t('themeDark')}</option>
                <option value="light">{t('themeLight')}</option>
              </select>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('lowRam')}</b>
                <span class="hint">{t('lowRamHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={lowRam}
                aria-label={t('lowRam')}
                onclick={() => (lowRam = !lowRam)}
              ></button>
            </div>
          </section>

          <section class="section">
            <div class="section-head">
              <h3>{t('settingsWindow')}</h3>
              <span class="hint">{t('settingsWindowHint')}</span>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsCloseToTray')}</b>
                <span class="hint">{t('settingsCloseToTrayHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.closeToTray}
                aria-label={t('settingsCloseToTray')}
                onclick={() => (draft.closeToTray = !draft.closeToTray)}
              ></button>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsStartMinimized')}</b>
                <span class="hint">{t('settingsStartMinimizedHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.startMinimized}
                aria-label={t('settingsStartMinimized')}
                onclick={() => (draft.startMinimized = !draft.startMinimized)}
              ></button>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsStartAtLogin')}</b>
                <span class="hint">{t('settingsStartAtLoginHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.startAtLogin}
                aria-label={t('settingsStartAtLogin')}
                onclick={() => (draft.startAtLogin = !draft.startAtLogin)}
              ></button>
            </div>
          </section>

          <section class="section">
            <div class="section-head">
              <h3>{t('settingsBehavior')}</h3>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsClipboardWatch')}</b>
                <span class="hint">{t('settingsClipboardWatchHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.clipboardWatch}
                aria-label={t('settingsClipboardWatch')}
                onclick={() => (draft.clipboardWatch = !draft.clipboardWatch)}
              ></button>
            </div>
          </section>
        {:else if tab === 'downloads'}
          <section class="section">
            <div class="section-head">
              <h3>{t('settingsDownloads')}</h3>
              <span class="hint">{t('settingsDownloadsHint')}</span>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsDefaultFolder')}</b>
                <span class="hint">{t('settingsDefaultFolderHint')}</span>
              </div>
              <div class="folder-row">
                <input
                  class="input mono"
                  bind:value={draft.defaultDestination}
                  placeholder={t('settingsOsDownloads')}
                  spellcheck="false"
                  dir="ltr"
                />
                <button class="btn btn-ghost" type="button" onclick={browseFolder}>
                  <Icon name="folder" size={13} />
                  {t('browse')}
                </button>
              </div>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsDefaultConnections')}</b>
                <span class="hint">{t('connectionsHint')}</span>
              </div>
              <div class="range-row">
                <input
                  type="range"
                  min="1"
                  max="32"
                  step="1"
                  bind:value={draft.defaultConnections}
                />
                <span class="range-value num">{draft.defaultConnections}</span>
              </div>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsMaxActive')}</b>
                <span class="hint">{t('parallelHint')}</span>
              </div>
              <div class="range-row">
                <input type="range" min="1" max="8" step="1" bind:value={draft.maxActive} />
                <span class="range-value num">{draft.maxActive}</span>
              </div>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsExtractDefault')}</b>
                <span class="hint">{t('extractHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.extractByDefault}
                aria-label={t('settingsExtractDefault')}
                onclick={() => (draft.extractByDefault = !draft.extractByDefault)}
              ></button>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsConfirmRemove')}</b>
                <span class="hint">{t('settingsConfirmRemoveHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.confirmRemove}
                aria-label={t('settingsConfirmRemove')}
                onclick={() => (draft.confirmRemove = !draft.confirmRemove)}
              ></button>
            </div>
          </section>

          <section class="section">
            <div class="section-head">
              <h3>{t('settingsAfterDownload')}</h3>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsScanDefender')}</b>
                <span class="hint">{t('settingsScanDefenderHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.scanWithDefender}
                aria-label={t('settingsScanDefender')}
                onclick={() => (draft.scanWithDefender = !draft.scanWithDefender)}
              ></button>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsShutdownDone')}</b>
                <span class="hint">{t('settingsShutdownDoneHint')}</span>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.shutdownWhenDone}
                aria-label={t('settingsShutdownDone')}
                onclick={() => (draft.shutdownWhenDone = !draft.shutdownWhenDone)}
              ></button>
            </div>
          </section>

          <section class="section">
            <div class="section-head">
              <h3>{t('settingsSchedule')}</h3>
              <span class="hint">{t('settingsScheduleHint')}</span>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsScheduleEnable')}</b>
              </div>
              <button
                class="switch"
                type="button"
                role="switch"
                aria-checked={draft.scheduleEnabled}
                aria-label={t('settingsScheduleEnable')}
                onclick={() => (draft.scheduleEnabled = !draft.scheduleEnabled)}
              ></button>
            </div>
            {#if draft.scheduleEnabled}
              <div class="setting">
                <div class="setting-text">
                  <b>{t('settingsScheduleStart')}</b>
                </div>
                <input
                  class="input time mono"
                  type="time"
                  bind:value={draft.scheduleStart}
                  aria-label={t('settingsScheduleStart')}
                />
              </div>
              <div class="setting">
                <div class="setting-text">
                  <b>{t('settingsScheduleStop')}</b>
                </div>
                <input
                  class="input time mono"
                  type="time"
                  bind:value={draft.scheduleStop}
                  aria-label={t('settingsScheduleStop')}
                />
              </div>
              <div class="setting">
                <div class="setting-text">
                  <b>{t('settingsScheduleStopAction')}</b>
                </div>
                <select
                  class="select action-select"
                  value={draft.scheduleStopAction}
                  onchange={(event) =>
                    (draft.scheduleStopAction = event.currentTarget
                      .value as AppSettings['scheduleStopAction'])}
                >
                  <option value="pause">{t('scheduleActionPause')}</option>
                  <option value="exit">{t('scheduleActionExit')}</option>
                  <option value="shutdown">{t('scheduleActionShutdown')}</option>
                </select>
              </div>
            {/if}
          </section>
        {:else if tab === 'network'}
          <section class="section">
            <div class="section-head">
              <h3>{t('settingsNetwork')}</h3>
              <div class="head-actions">
                <span class="hint">{t('settingsNetworkHint')}</span>
                <button class="btn btn-ghost small" type="button" onclick={() => app.refreshInterfaces()}>
                  <Icon name="refresh" size={12} />
                  {t('refresh')}
                </button>
              </div>
            </div>
            <div class="iface-list">
              {#each app.interfaces as iface (iface.id)}
                {#if iface.id !== 'auto'}
                  <div class="iface-row" class:dead={!iface.usable}>
                    <span class="iface-icon"><Icon name={kindIcon(iface.kind)} size={15} /></span>
                    <div class="iface-main">
                      <div class="iface-title">
                        <b>{iface.name}</b>
                        {#if iface.isDefault}<span class="chip" data-tone="info">{t('defaultAdapter')}</span>{/if}
                        <span class="chip" data-tone={toneOf(iface)}>{statusOf(iface)}</span>
                      </div>
                      <span class="iface-detail num">
                        {iface.address}{iface.network ? ` · ${iface.network}` : ''}
                      </span>
                    </div>
                    <button
                      class="btn btn-ghost small"
                      type="button"
                      disabled={!iface.usable || app.interfaceTests[iface.id]?.state === 'testing'}
                      onclick={() => test(iface)}
                    >
                      {app.interfaceTests[iface.id]?.state === 'testing'
                        ? t('testing')
                        : t('test')}
                    </button>
                  </div>
                {/if}
              {/each}
            </div>
            {#if app.interfaces.filter((iface) => iface.id !== 'auto').length === 0}
              <div class="empty">{t('settingsNoInterfaces')}</div>
            {/if}
            <div class="note">
              <Icon name="alert" size={13} />
              <span>{t('settingsNetworkNote')}</span>
            </div>
          </section>

          <section class="section">
            <div class="section-head">
              <h3>{t('settingsProxy')}</h3>
              <span class="hint">{t('settingsProxyHint')}</span>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsProxyMode')}</b>
              </div>
              <select
                class="select compact"
                value={draft.proxyMode}
                onchange={(event) =>
                  (draft.proxyMode = event.currentTarget.value as AppSettings['proxyMode'])}
              >
                <option value="system">{t('proxySystem')}</option>
                <option value="none">{t('proxyNone')}</option>
                <option value="manual">{t('proxyManual')}</option>
              </select>
            </div>
            {#if draft.proxyMode === 'manual'}
              <div class="setting">
                <div class="setting-text">
                  <b>{t('settingsProxyUrl')}</b>
                  <span class="hint">{t('settingsProxyUrlHint')}</span>
                </div>
                <input
                  class="input proxy-url mono"
                  bind:value={draft.proxyUrl}
                  placeholder="http://127.0.0.1:8080"
                  spellcheck="false"
                  dir="ltr"
                />
              </div>
              <div class="setting">
                <div class="setting-text">
                  <b>{t('settingsProxyUsername')}</b>
                  <span class="hint">{t('optional')}</span>
                </div>
                <input
                  class="input compact"
                  bind:value={draft.proxyUsername}
                  autocomplete="off"
                  spellcheck="false"
                />
              </div>
              <div class="setting">
                <div class="setting-text">
                  <b>{t('settingsProxyPassword')}</b>
                  <span class="hint">{t('optional')}</span>
                </div>
                <input
                  class="input compact"
                  type="password"
                  bind:value={draft.proxyPassword}
                  autocomplete="new-password"
                />
              </div>
            {/if}
          </section>
        {:else if tab === 'browser'}
          <section class="section">
            <div class="section-head">
              <h3>{t('settingsBrowser')}</h3>
              <span class="hint">{t('settingsBrowserHint')}</span>
            </div>
            <div class="bridge-card" data-live={app.bridge?.running ? 'yes' : 'no'}>
              <span class="bridge-dot"></span>
              <div class="bridge-main">
                <b>{app.bridge?.running ? t('bridgeRunning') : t('bridgeStopped')}</b>
                <span class="hint">
                  {#if app.bridge?.running}
                    127.0.0.1:{app.bridge.port} · API {app.bridge.api}
                  {:else}
                    {bridgeError ?? t('bridgeStoppedHint')}
                  {/if}
                </span>
              </div>
              {#if app.bridge?.running}
                <button class="btn btn-ghost small" type="button" onclick={copyBridge}>
                  <Icon name="copy" size={12} />
                  {t('copyAddress')}
                </button>
              {/if}
            </div>
            <div class="steps">
              <div class="step"><span class="step-num">1</span><span>{t('browserStepOne')}</span></div>
              <div class="step"><span class="step-num">2</span><span>{t('browserStepTwo')}</span></div>
              <div class="step"><span class="step-num">3</span><span>{t('browserStepThree')}</span></div>
            </div>
            <div class="row-actions">
              <button
                class="btn btn-primary"
                type="button"
                onclick={() =>
                  app.openUrl(
                    'https://github.com/bouadjadjatamer2-hub/Majra/releases/latest/download/Majra-Browser-Extension.zip',
                  )}
              >
                <Icon name="download" size={13} />
                {t('downloadExtension')}
              </button>
              <button class="btn btn-ghost" type="button" onclick={() => refreshBridge()}>
                <Icon name="refresh" size={13} />
                {t('refresh')}
              </button>
            </div>
            <div class="note">
              <Icon name="link" size={13} />
              <span>{t('deepLinkNote')}</span>
            </div>
          </section>
        {:else}
          <section class="section">
            <div class="section-head">
              <h3>{t('settingsStorage')}</h3>
              <span class="hint">{t('settingsStorageHint')}</span>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('dataFolder')}</b>
                <span class="path num">{app.dataFolder || '—'}</span>
              </div>
              <button class="btn btn-ghost" type="button" onclick={() => app.openFolder(app.dataFolder)}>
                <Icon name="folder" size={13} />
                {t('openFolder')}
              </button>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('settingsFile')}</b>
                <span class="path num">{app.settingsPath || '—'}</span>
              </div>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('clearHistory')}</b>
                <span class="hint">{t('clearHistoryHint')}</span>
              </div>
              <button
                class="btn btn-danger"
                type="button"
                disabled={clearing || finishedCount === 0}
                onclick={clearHistory}
              >
                <Icon name="trash" size={13} />
                {clearing ? t('clearing') : `${t('clearHistory')} (${finishedCount})`}
              </button>
            </div>
            <div class="setting">
              <div class="setting-text">
                <b>{t('resetSettings')}</b>
                <span class="hint">{t('resetSettingsHint')}</span>
              </div>
              <button class="btn btn-ghost" type="button" onclick={resetDraft}>
                <Icon name="refresh" size={13} />
                {t('reset')}
              </button>
            </div>
          </section>

          <section class="section">
            <div class="section-head">
              <h3>{t('about')}</h3>
            </div>
            <div class="about">
              <div class="about-mark"><Icon name="download" size={22} /></div>
              <div class="about-text">
                <b>Majra {t('version')}</b>
                <span class="hint">{t('aboutHint')}</span>
                <span class="hint">{t('aboutEngine')}</span>
              </div>
            </div>
          </section>
        {/if}
      </div>
    </div>

    <footer class="dialog-foot">
      {#if saved}
        <span class="saved"><Icon name="check" size={13} /> {t('settingsSaved')}</span>
      {/if}
      <span class="flex"></span>
      <button class="btn btn-ghost" type="button" onclick={close}>{t('close')}</button>
      <button class="btn btn-primary" type="button" disabled={saving} onclick={save}>
        <Icon name="check" size={13} />
        {saving ? t('saving') : t('save')}
      </button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 55;
    display: grid;
    place-items: center;
    padding: 22px;
    background: var(--overlay);
    animation: fade-in 120ms ease-out;
  }

  .dialog {
    display: flex;
    flex-direction: column;
    width: min(860px, 100%);
    height: min(640px, 88vh);
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
    gap: 10px;
  }

  .title > div {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .mark {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
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
    font-size: 10.5px;
    color: var(--faint);
  }

  .settings-body {
    display: grid;
    grid-template-columns: 186px minmax(0, 1fr);
    min-height: 0;
    flex: 1;
  }

  .tabs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 8px;
    border-inline-end: 1px solid var(--line);
    background: var(--bg-elev);
    overflow-y: auto;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 32px;
    padding-inline: 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-2);
    font-size: 12.5px;
    text-align: start;
    transition:
      background var(--motion),
      color var(--motion);
  }

  .tab:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .tab[aria-current='true'] {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 700;
  }

  .tab-spacer {
    flex: 1;
  }

  .version {
    padding: 8px 10px 2px;
    color: var(--faint);
    font-size: 10px;
  }

  .content {
    min-height: 0;
    overflow-y: auto;
    padding: 14px 16px 18px;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-block-end: 20px;
  }

  .section:last-child {
    margin-block-end: 0;
  }

  .section-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-block-end: 2px;
  }

  h3 {
    margin: 0;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  :global(html[lang='ar']) h3 {
    letter-spacing: 0;
    text-transform: none;
    font-size: 12px;
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    min-height: 46px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
  }

  .setting-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .setting-text b {
    font-size: 12.5px;
    font-weight: 700;
  }

  .compact {
    width: 150px;
    flex: none;
  }

  .time {
    width: 118px;
    flex: none;
  }

  .action-select {
    width: 190px;
    flex: none;
  }

  .proxy-url {
    width: min(360px, 100%);
    flex: none;
    font-size: 11px;
  }

  .folder-row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: min(420px, 100%);
    flex: none;
  }

  .folder-row .input {
    flex: 1;
    min-width: 0;
    font-size: 11px;
  }

  .mono {
    font-family: var(--font-mono);
    direction: ltr;
  }

  .range-row {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 230px;
    flex: none;
  }

  .range-row input[type='range'] {
    flex: 1;
    accent-color: var(--accent);
  }

  .range-value {
    width: 24px;
    text-align: center;
    font-size: 12px;
    font-weight: 700;
    color: var(--accent);
  }

  .iface-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .iface-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
  }

  .iface-row.dead {
    opacity: 0.68;
  }

  .iface-icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: var(--r-sm);
    background: var(--surface-2);
    color: var(--text-2);
  }

  .iface-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .iface-title {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .iface-title b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }

  .iface-detail {
    color: var(--faint);
    font-size: 10px;
    direction: ltr;
  }

  .bridge-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 11px 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
  }

  .bridge-card[data-live='yes'] {
    border-color: color-mix(in srgb, var(--jade) 45%, transparent);
    background: var(--jade-soft);
  }

  .bridge-dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    background: var(--danger);
    box-shadow: 0 0 0 3px var(--danger-soft);
  }

  .bridge-card[data-live='yes'] .bridge-dot {
    background: var(--jade);
    box-shadow: 0 0 0 3px var(--jade-soft);
  }

  .bridge-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .bridge-main b {
    font-size: 12.5px;
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 4px 2px;
  }

  .step {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.45;
  }

  .step-num {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .row-actions {
    display: flex;
    gap: 7px;
    flex-wrap: wrap;
  }

  .note {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    background: var(--info-soft);
    color: var(--info);
    font-size: 11px;
    line-height: 1.45;
  }

  .empty {
    padding: 18px;
    border: 1px dashed var(--line-strong);
    border-radius: var(--r-sm);
    color: var(--faint);
    text-align: center;
    font-size: 12px;
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--faint);
    font-size: 10.5px;
  }

  .about {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
  }

  .about-mark {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    flex: none;
    border-radius: var(--r-md);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .about-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .about-text b {
    font-size: 13px;
  }

  .dialog-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-block-start: 1px solid var(--line);
    background: var(--bg-elev);
    flex: none;
  }

  .saved {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--jade);
    font-size: 11.5px;
    font-weight: 700;
  }

  .flex {
    flex: 1;
  }

  .small {
    height: 24px;
    padding-inline: 8px;
    font-size: 11px;
  }

  @media (max-width: 720px) {
    .settings-body {
      grid-template-columns: 1fr;
    }

    .tabs {
      flex-direction: row;
      overflow-x: auto;
      border-inline-end: 0;
      border-block-end: 1px solid var(--line);
    }

    .tab {
      height: 28px;
      white-space: nowrap;
    }

    .tab-spacer,
    .version {
      display: none;
    }

    .setting {
      align-items: flex-start;
      flex-direction: column;
      gap: 8px;
    }

    .folder-row,
    .range-row,
    .compact {
      width: 100%;
    }
  }
</style>
