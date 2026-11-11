<script lang="ts" module>
  const paths = {
    plus: ['M12 5v14M5 12h14'],
    pause: ['M9 5v14M15 5v14'],
    play: ['M8 5l11 7-11 7z'],
    trash: ['M4 7h16M9 7V5h6v2M7 7l1 13h8l1-13'],
    search: ['M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM20 20l-4.2-4.2'],
    close: ['M6 6l12 12M18 6L6 18'],
    minimize: ['M5 12h14'],
    maximize: ['M6 6h12v12H6z'],
    chevron: ['M6 9l6 6 6-6'],
    folder: ['M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'],
    link: ['M10 14a5 5 0 0 0 7 0l2-2a5 5 0 0 0-7-7l-1 1M14 10a5 5 0 0 0-7 0l-2 2a5 5 0 0 0 7 7l1-1'],
    download: ['M12 4v11M7 10l5 5 5-5M5 20h14'],
    check: ['M5 12l5 5L20 7'],
    alert: ['M12 4l9 16H3zM12 10v4M12 17v.5'],
    wifi: ['M3 9a15 15 0 0 1 18 0M6 13a10 10 0 0 1 12 0M9 17a5 5 0 0 1 6 0M12 20v.5'],
    ethernet: ['M4 9h16v8H4zM8 17v3M16 17v3M8 13h.5M12 13h.5M16 13h.5'],
    usb: ['M12 21V5M9 8l3-3 3 3M7 12h10v4a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2z'],
    sun: ['M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2v2M12 20v2M4 12H2M22 12h-2M5 5l1.5 1.5M17.5 17.5L19 19M19 5l-1.5 1.5M6.5 17.5L5 19'],
    moon: ['M20 14A8 8 0 0 1 10 4a8 8 0 1 0 10 10z'],
    globe: ['M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c3 3.5 3 14.5 0 18M12 3c-3 3.5-3 14.5 0 18'],
    panel: ['M4 5h16v14H4zM14 5v14'],
    copy: ['M9 9h10v11H9zM5 15V4h10'],
    file: ['M7 3h7l4 4v14H7zM14 3v4h4'],
    video: ['M3 6h12v12H3zM15 10l6-3v10l-6-3'],
    audio: ['M9 18V6l10-2v12M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM19 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z'],
    document: ['M7 3h7l4 4v14H7zM14 3v4h4M10 12h5M10 16h5'],
    archive: ['M4 5h16v4H4zM6 9v10h12V9M10 13h4'],
    program: ['M4 5h16v12H4zM8 21h8M12 17v4'],
    list: ['M4 6h16M4 12h16M4 18h16'],
    clock: ['M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2'],
    refresh: ['M20 12a8 8 0 1 1-2-5M20 4v4h-4'],
    gear: [
      'M12 8.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7z',
      'M19.4 13.5a7.8 7.8 0 0 0 0-3l2-1.2-2-3.4-2.2 1.1a7.6 7.6 0 0 0-2.6-1.5L14.2 3h-4.4l-.4 2.5A7.6 7.6 0 0 0 6.8 7L4.6 5.9l-2 3.4 2 1.2a7.8 7.8 0 0 0 0 3l-2 1.2 2 3.4 2.2-1.1a7.6 7.6 0 0 0 2.6 1.5l.4 2.5h4.4l.4-2.5a7.6 7.6 0 0 0 2.6-1.5l2.2 1.1 2-3.4z',
    ],
  } as const;

  export type IconName = keyof typeof paths;
</script>

<script lang="ts">
  let {
    name,
    size = 16,
    stroke = 1.8,
  }: { name: IconName; size?: number; stroke?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#each paths[name] as d}
    <path {d} />
  {/each}
</svg>
