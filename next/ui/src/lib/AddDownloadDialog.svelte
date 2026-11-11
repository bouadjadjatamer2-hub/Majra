<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './state.svelte';
  import { api, errorText } from './api';
  import { t } from './i18n';
  import { formatBytes } from './format';
  import type { ProbeResult } from './types';
  import Icon from './Icon.svelte';

  let links = $state(
    app.prefillUrl ??
      (typeof location !== 'undefined'
        ? (new URLSearchParams(location.search).get('url') ?? '')
        : ''),
  );
  let destination = $state(app.defaultDestination);
  let connections = $state(app.defaultConnections);
  let auto = $state(true);
  let chosen = $state<string[]>([]);
  let extract = $state(app.extractByDefault);
  let probe = $state<ProbeResult | null>(null);
  let probing = $state(false);
  let starting = $state(false);
  let error = $state<string | null>(null);
  let textarea = $state<HTMLTextAreaElement | null>(null);
  let lastOfferId = $state<string | null>(null);

  let urls = $derived(
    links
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean),
  );

  let usableInterfaces = $derived(
    app.interfaces.filter((iface) => iface.id !== 'auto' && iface.usable),
  );

  let chosenNetworks = $derived(
    usableInterfaces
      .filter((iface) => !auto && chosen.includes(iface.id) && iface.network)
      .map((iface) => iface.network as string),
  );

  let sameNetwork = $derived(
    chosenNetworks.length > 1 && new Set(chosenNetworks).size < chosenNetworks.length,
  );

  onMount(() => {
    // The clipboard watcher hands one link over, then forgets it so reopening the dialog
    // does not silently repeat an old copy.
    app.prefillUrl = null;
    textarea?.focus();
  });

  // The extension already probed the link with the browser's own sign-in. Adopt that
  // result so Check and Start cost no extra request, and a signed link is not fetched twice.
  $effect(() => {
    const offer = app.offer;
    if (!offer) {
      lastOfferId = null;
      return;
    }
    if (offer.id === lastOfferId) return;
    lastOfferId = offer.id;
    links = offer.url;
    probe = offer.probe;
    error = null;
    void textarea?.focus();
  });

  function offerIdFor(url: string): string | null {
    return app.offer && app.offer.url === url ? app.offer.id : null;
  }

  function onLinksInput(): void {
    const first = urls[0];
    if (probe && probe.requestedUrl !== first) probe = null;
  }

  function validUrl(value: string): boolean {
    return /^https?:\/\/\S+$/i.test(value);
  }

  async function check(): Promise<void> {
    error = null;
    probe = null;
    const first = urls[0];
    if (!first) {
      error = t('linksRequired');
      return;
    }
    if (!validUrl(first)) {
      error = t('invalidLink');
      return;
    }
    probing = true;
    try {
      probe = await api.probeLink(first, offerIdFor(first));
    } catch (caught) {
      error = `${t('probeFailed')}: ${errorText(caught)}`;
    } finally {
      probing = false;
    }
  }

  function toggleInterface(id: string): void {
    const iface = app.interfaces.find((item) => item.id === id);
    if (!iface?.usable) return;
    auto = false;
    chosen = chosen.includes(id)
      ? chosen.filter((item) => item !== id)
      : [...chosen, id];
    // An empty manual selection is the same as automatic in the backend; show that
    // honestly instead of leaving Auto unchecked with no adapter selected.
    if (chosen.length === 0) auto = true;
  }

  function useAllInterfaces(): void {
    auto = usableInterfaces.length === 0;
    chosen = usableInterfaces.map((iface) => iface.id);
  }

  async function pasteLink(): Promise<void> {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) return;
      links = links.trim() ? `${links.trim()}\n${text.trim()}` : text.trim();
    } catch {
      error = t('pasteFailed');
    }
  }

  async function start(): Promise<void> {
    error = null;
    if (urls.length === 0) {
      error = t('linksRequired');
      return;
    }
    const invalid = urls.find((url) => !validUrl(url));
    if (invalid) {
      error = `${t('invalidLink')} ${invalid}`;
      return;
    }
    starting = true;
    try {
      for (const url of urls) {
        await app.addDownload(
          {
            url,
            destination: destination.trim(),
            connections,
            interfaceIds: auto ? [] : chosen,
            extract,
          },
          offerIdFor(url),
        );
      }
      app.closeAdd();
    } catch (caught) {
      error = `${t('startFailed')}: ${errorText(caught)}`;
    } finally {
      starting = false;
    }
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape') app.closeAdd();
  }}
/>

<div
  class="overlay"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) app.closeAdd();
  }}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="add-title">
    <header class="dialog-head">
      <div class="title">
        <span class="mark"><Icon name="plus" size={15} stroke={2.2} /></span>
        <h2 id="add-title">{t('newDownloadTitle')}</h2>
      </div>
      <button
        class="icon-btn"
        type="button"
        aria-label={t('cancel')}
        onclick={() => app.closeAdd()}
      >
        <Icon name="close" size={14} />
      </button>
    </header>

    <div class="dialog-body">
      {#if app.offer}
        <div class="offer">
          <span class="offer-mark"><Icon name="link" size={14} /></span>
          <span class="offer-text">
            <b>{t('browserOffer')}</b>
            {#if app.offer.from}<span class="offer-from">{app.offer.from}</span>{/if}
            <span class="hint">{t('browserOfferHint')}</span>
          </span>
        </div>
      {/if}

      <div class="field">
        <div class="field-head">
          <label class="field-label" for="add-links">{t('links')}</label>
          <button class="link-btn" type="button" onclick={pasteLink}>
            {t('paste')}
          </button>
        </div>
        <textarea
          id="add-links"
          class="textarea"
          rows="3"
          bind:this={textarea}
          bind:value={links}
          oninput={onLinksInput}
          placeholder={'https://example.com/file.zip'}
          spellcheck="false"
        ></textarea>
        <span class="hint">{t('linksHint')}</span>
      </div>

      <div class="row">
        <div class="field grow">
          <label class="field-label" for="add-dest">{t('destination')}</label>
          <div class="input-with-icon">
            <Icon name="folder" size={14} />
            <input
              id="add-dest"
              class="input bare"
              bind:value={destination}
              placeholder={'C:\\Users\\Lenovo\\Downloads'}
              spellcheck="false"
              dir="ltr"
            />
          </div>
          <span class="hint">{t('destinationHint')}</span>
        </div>

        <div class="field conn">
          <label class="field-label" for="add-conn">{t('connections')}</label>
          <div class="stepper">
            <input
              id="add-conn"
              type="range"
              min="1"
              max="32"
              step="1"
              bind:value={connections}
            />
            <span class="num value">{connections}</span>
          </div>
          <span class="hint">{t('connectionsHint')}</span>
        </div>
      </div>

      <div class="field">
        <div class="field-head">
          <span class="field-label">{t('interfaces')}</span>
          <button class="link-btn" type="button" onclick={useAllInterfaces}>
            {t('allInterfaces')}
          </button>
        </div>
        <label class="iface auto">
          <input type="checkbox" bind:checked={auto} />
          <span class="iface-name">{t('auto')}</span>
          <span class="iface-addr">{t('autoHint')}</span>
        </label>
        <div class="ifaces">
          {#each app.interfaces as iface (iface.id)}
            {#if iface.id !== 'auto'}
              <label
                class="iface"
                class:shared={sameNetwork && chosen.includes(iface.id)}
                class:unusable={!iface.usable}
                title={iface.usable ? iface.address : t('noRouteHint')}
              >
                <input
                  type="checkbox"
                  disabled={!iface.usable}
                  checked={!auto && chosen.includes(iface.id)}
                  onchange={() => toggleInterface(iface.id)}
                />
                <span class="iface-name">
                  {iface.name}
                  {#if iface.isDefault}<span class="tag">{t('defaultAdapter')}</span>{/if}
                  {#if !iface.usable}<span class="tag dead">{t('noRoute')}</span>{/if}
                </span>
                <span class="iface-addr num">{iface.network ?? iface.address}</span>
              </label>
            {/if}
          {/each}
        </div>
        {#if usableInterfaces.length === 0}
          <div class="warn">
            <Icon name="alert" size={13} />
            <span>{t('noUsableInterfaces')}</span>
          </div>
        {/if}
        {#if sameNetwork}
          <div class="warn">
            <Icon name="alert" size={13} />
            <span>{t('sameNetworkHint')}</span>
          </div>
        {/if}
        <span class="hint">{t('interfacesHint')}</span>
      </div>

      <label class="toggle">
        <input type="checkbox" bind:checked={extract} />
        <span class="toggle-text">
          <b>{t('extract')}</b>
          <span class="hint">{t('extractHint')}</span>
        </span>
      </label>

      {#if probe}
        <div class="probe">
          <div class="probe-row">
            <span>{t('fileName')}</span>
            <b title={probe.fileName}>{probe.fileName}</b>
          </div>
          <div class="probe-row">
            <span>{t('size')}</span>
            <b class="num">{probe.totalBytes ? formatBytes(probe.totalBytes) : '—'}</b>
          </div>
          <div class="probe-row">
            <span>{t('ranges')}</span>
            <b class:ok={probe.supportsRanges}>{probe.supportsRanges ? t('yes') : t('no')}</b>
          </div>
          <div class="probe-row">
            <span>{t('type')}</span>
            <b title={probe.contentType ?? ''}>{probe.contentType ?? '—'}</b>
          </div>
        </div>
      {/if}

      {#if error}
        <div class="error">
          <Icon name="alert" size={14} />
          <span>{error}</span>
        </div>
      {/if}
    </div>

    <footer class="dialog-foot">
      <button
        class="btn"
        type="button"
        onclick={check}
        disabled={probing || urls.length === 0}
      >
        <Icon name="search" size={13} />
        {probing ? t('checking') : t('check')}
      </button>
      <span class="flex"></span>
      <button class="btn btn-ghost" type="button" onclick={() => app.closeAdd()}>
        {t('cancel')}
      </button>
      <button
        class="btn btn-primary"
        type="button"
        onclick={start}
        disabled={starting || urls.length === 0}
      >
        <Icon name="download" size={14} />
        {t('start')}
      </button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--overlay);
    animation: fade-in 120ms ease-out;
  }

  .dialog {
    display: flex;
    flex-direction: column;
    width: min(580px, 100%);
    max-height: min(86vh, 720px);
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

  .dialog-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 16px;
    overflow-y: auto;
    min-height: 0;
  }

  .dialog .textarea {
    min-height: 64px;
  }

  .offer {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 9px 10px;
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: var(--r-sm);
    background: var(--accent-soft);
  }

  .offer-mark {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    flex: none;
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--accent);
  }

  .offer-text {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 7px;
    min-width: 0;
  }

  .offer-text b {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-2);
  }

  .offer-from {
    font-size: 10px;
    font-weight: 700;
    color: var(--accent);
  }

  .offer-text .hint {
    flex-basis: 100%;
  }

  .row {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }

  .grow {
    flex: 1;
    min-width: 0;
  }

  .conn {
    width: 210px;
    flex: none;
  }

  .input-with-icon {
    display: flex;
    align-items: center;
    gap: 7px;
    padding-inline: 9px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    color: var(--faint);
    transition:
      border-color var(--motion),
      box-shadow var(--motion);
  }

  .input-with-icon:focus-within {
    border-color: var(--focus);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--focus) 20%, transparent);
  }

  .input.bare {
    padding: 7px 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
  }

  .stepper {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
  }

  .stepper input[type='range'] {
    flex: 1;
    min-width: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .stepper .value {
    width: 30px;
    text-align: center;
    font-size: 12px;
    font-weight: 700;
    color: var(--accent);
  }

  .iface {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 30px;
    padding: 5px 9px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    font-size: 12px;
    cursor: pointer;
    transition:
      border-color var(--motion),
      background var(--motion);
  }

  .iface:hover {
    border-color: var(--line-strong);
  }

  .iface:has(input:checked) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .iface.auto {
    margin-block-end: 6px;
  }

  .iface input {
    accent-color: var(--accent);
    margin: 0;
  }

  .iface-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .iface-addr {
    color: var(--faint);
    font-size: 10px;
    direction: ltr;
  }

  .iface.shared,
  .iface.shared:has(input:checked) {
    border-color: var(--warn);
  }

  .iface.unusable {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .iface.unusable:hover {
    border-color: var(--line);
  }

  .ifaces {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .tag {
    margin-inline-start: 5px;
    padding: 0 5px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .tag.dead {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .warn {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin-block-start: 6px;
    color: var(--warn);
    font-size: 10.5px;
    line-height: 1.45;
  }

  .field-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .link-btn {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--accent);
    font-size: 10.5px;
    font-weight: 700;
    cursor: pointer;
  }

  .link-btn:hover {
    text-decoration: underline;
  }

  .toggle {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 9px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    cursor: pointer;
    transition:
      border-color var(--motion),
      background var(--motion);
  }

  .toggle:has(input:checked) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .toggle input {
    margin: 2px 0 0;
    accent-color: var(--accent);
  }

  .toggle-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .toggle-text b {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-2);
  }

  .probe {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--line);
    overflow: hidden;
  }

  .probe-row {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: 7px 9px;
    background: var(--surface-2);
  }

  .probe-row > span {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--faint);
  }

  :global(html[lang='ar']) .probe-row > span {
    letter-spacing: 0;
    text-transform: none;
    font-size: 10px;
  }

  .probe-row > b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-2);
  }

  .probe-row > b.ok {
    color: var(--jade);
  }

  .error {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 8px 9px;
    border: 1px solid color-mix(in srgb, var(--danger) 32%, transparent);
    border-radius: var(--r-sm);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: 11.5px;
    line-height: 1.45;
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

  @media (max-width: 640px) {
    .row {
      flex-direction: column;
    }

    .conn {
      width: 100%;
    }

    .ifaces,
    .probe {
      grid-template-columns: 1fr;
    }
  }
</style>
