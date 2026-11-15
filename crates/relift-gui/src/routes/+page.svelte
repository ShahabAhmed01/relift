<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { 
    HardDrive, 
    Usb, 
    Database, 
    AlertTriangle, 
    CheckCircle, 
    Info, 
    ChevronRight,
    Settings,
    Sun,
    Moon,
    HelpCircle,
    ExternalLink
  } from 'lucide-svelte';

  // Types
  interface DeviceInfo {
    path: string;
    kind: string;
    label?: string;
    filesystem?: string;
    size?: number;
    size_human: string;
    accessible: boolean;
    note?: string;
    health: string;
    is_system_drive: boolean;
  }

  interface SessionSummary {
    session_dir: string;
    source_path: string;
    source_size: number;
    source_size_human: string;
    status: string;
    filesystems: FilesystemSummary[];
    candidates: number;
    deleted: number;
    live: number;
    with_content: number;
    metadata_only: number;
    warnings: string[];
  }

  interface FilesystemSummary {
    volume_index: number;
    offset: number;
    length: number;
    label?: string;
    kind?: string;
    candidates: number;
    deleted: number;
    live: number;
    warnings: string[];
  }

  // State
  let devices = $state<DeviceInfo[]>([]);
  let selectedDevice: DeviceInfo | null = $state(null);
  let selectedImage: string | null = $state(null);
  let loading = $state(false);
  let showAdvanced = $state(false);
  let scanMode = $state<'quick' | 'deep' | 'formatted' | 'lost'>('quick');
  let includeLive = $state(false);
  let threadCount = $state<number>(navigator.hardwareConcurrency || 4);
  let showImagePicker = $state(false);

  // Computed
  let scanDisabled = $derived(!selectedDevice && !selectedImage);
  let primaryButtonText = $derived(selectedDevice?.health === 'degraded' ? 'Create image first (recommended)' : 'Scan for deleted files');
  let primaryButtonVariant = $derived(selectedDevice?.health === 'degraded' ? 'secondary' : 'primary');

  // Health badge helpers
  function getHealthIcon(health: string) {
    switch (health) {
      case 'healthy': return CheckCircle;
      case 'degraded': return AlertTriangle;
      case 'trim_warning': return Info;
      default: return AlertTriangle;
    }
  }

  function getHealthClass(health: string) {
    switch (health) {
      case 'healthy': return 'badge-success';
      case 'degraded': return 'badge-error';
      case 'trim_warning': return 'badge-warning';
      default: return 'badge-unverified';
    }
  }

  function getHealthLabel(health: string) {
    switch (health) {
      case 'healthy': return 'Healthy';
      case 'degraded': return 'Degraded';
      case 'trim_warning': return 'TRIM active';
      default: return 'Unknown';
    }
  }

  function getKindIcon(kind: string) {
    if (kind.includes('usb') || kind.includes('removable')) return Usb;
    if (kind.includes('nvme') || kind.includes('ssd')) return Database;
    return HardDrive;
  }

  async function loadDevices() {
    try {
      const result = await invoke<DeviceInfo[]>('list_devices_cmd');
      devices = result;
    } catch (e) {
      console.error('Failed to load devices:', e);
    }
  }

  async function handleImageSelect() {
    try {
      const path = await open({
        multiple: false,
        filters: [
          { name: 'Disk Images', extensions: ['img', 'dd', 'raw', 'e01', 'vhd', 'vhdx', 'vmdk', 'qcow2'] },
          { name: 'All Files', extensions: ['*'] }
        ]
      });
      if (path) {
        selectedImage = Array.isArray(path) ? path[0] : path;
        selectedDevice = null;
      }
    } catch (e) {
      console.error('Image selection failed:', e);
    }
  }

  async function startScan() {
    if (!selectedDevice && !selectedImage) return;
    
    loading = true;
    try {
      const source = selectedImage || selectedDevice!.path;
      const sessionDir = await getSessionDir(source);
      
      const summary = await invoke<SessionSummary>('scan_cmd', {
        options: {
          source,
          session_dir: sessionDir,
          include_live: includeLive,
          carve: scanMode !== 'quick',
          volume_indices: [],
          max_candidates: 1000000
        }
      });
      
      // Store session info and navigate to scan page
      sessionStorage.setItem('relift_session', JSON.stringify(summary));
      goto('/scan');
    } catch (e) {
      console.error('Scan failed:', e);
      alert(`Scan failed: ${e}`);
    } finally {
      loading = false;
    }
  }

  async function getSessionDir(source: string): Promise<string> {
    const base = await invoke<string>('get_sessions_dir') || '.';
    const name = source.replace(/[\\/:*?"<>|]/g, '_');
    return `${base}/relift_session_${name}_${Date.now()}`;
  }

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  onMount(async () => {
    await loadDevices();
  });
</script>

<div class="min-h-screen flex flex-col">
  <!-- Header -->
  <header class="border-b border-border px-4 py-3 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="w-8 h-8 rounded-lg bg-accent flex items-center justify-center">
        <Database class="w-5 h-5 text-bg" />
      </div>
      <div>
        <h1 class="text-lg font-semibold text-text">Relift</h1>
        <p class="text-xs text-text-muted">Read-only deleted-file recovery</p>
      </div>
    </div>
    <div class="flex items-center gap-2">
      <button class="btn-ghost p-2" title="Help" aria-label="Help">
        <HelpCircle class="w-5 h-5" />
      </button>
      <button class="btn-ghost p-2" onclick={toggleTheme} aria-label="Toggle theme">
        {#if theme === 'dark'}<Sun class="w-5 h-5" />{:else}<Moon class="w-5 h-5" />{/if}
      </button>
      <button class="btn-ghost p-2" onclick={() => showAdvanced = !showAdvanced} aria-label="Advanced options">
        <Settings class="w-5 h-5" />
      </button>
    </div>
  </header>

  <!-- Main Content -->
  <main class="flex-1 overflow-auto p-4">
    <!-- Safety Notice -->
    <div class="mb-4 p-3 rounded-lg bg-warning-muted border border-warning/30 flex items-start gap-3">
      <Info class="w-5 h-5 text-warning flex-shrink-0 mt-0.5" />
      <div class="text-sm text-text">
        <strong>Important:</strong> Stop using the drive immediately. Do not write anything to it. 
        Recover files to a <strong>different physical drive</strong>. 
        <a href="#" class="underline hover:text-accent" onclick={(e) => { e.preventDefault(); alert('Detailed safety guide coming soon'); }}>
          Why this matters
        </a>
      </div>
    </div>

    <!-- Device List -->
    <section class="mb-6">
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-base font-medium text-text">Select a source</h2>
        <button class="btn-secondary text-sm" onclick={handleImageSelect}>
          <Database class="w-4 h-4" />
          Open disk image…
        </button>
      </div>

      {#if showImagePicker}
        <div class="mb-4 p-4 rounded-lg bg-surface border border-border">
          <p class="text-sm text-text-muted mb-2">Selected image: <code class="monospace">{selectedImage}</code></p>
          <button class="btn-ghost text-sm" onclick={() => { selectedImage = null; showImagePicker = false; }}>
            Change image
          </button>
        </div>
      {/if}

      <div class="grid gap-3" style="grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));">
        {#each devices as device}
          <button
            class="card card-elevated p-4 text-left transition-all duration-120 flex flex-col"
            class:ring-2={selectedDevice?.path === device.path}
            class:ring-accent={selectedDevice?.path === device.path}
            class:bg-surface-hover={selectedDevice?.path === device.path}
            onclick={() => { selectedDevice = device; selectedImage = null; showImagePicker = false; }}
            disabled={!device.accessible}
            aria-pressed={selectedDevice?.path === device.path}
          >
            <div class="flex items-start gap-3 mb-3">
              <div class="w-10 h-10 rounded-lg bg-surface border border-border flex items-center justify-center flex-shrink-0">
                <svelte:component this={getKindIcon(device.kind)} class="w-5 h-5 text-text-muted" />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="font-medium text-text truncate">{device.path}</span>
                  <span class="badge badge-unverified">{device.kind.toUpperCase()}</span>
                  {#if device.is_system_drive}
                    <span class="badge badge-warning">System</span>
                  {/if}
                </div>
                <div class="flex items-center gap-2 text-xs text-text-muted mt-1 flex-wrap">
                  {#if device.label}
                    <span>Label: {device.label}</span>
                  {/if}
                  {#if device.filesystem}
                    <span>FS: {device.filesystem.toUpperCase()}</span>
                  {/if}
                  <span>{device.size_human}</span>
                </div>
              </div>
            </div>

            <div class="flex items-center justify-between pt-3 border-t border-border">
              <span class={getHealthClass(device.health)}>
                <svelte:component this={getHealthIcon(device.health)} class="w-3 h-3" />
                {getHealthLabel(device.health)}
              </span>
              {#if device.note}
                <span class="text-xs text-text-muted truncate max-w-[150px]" title={device.note}>{device.note}</span>
              {/if}
            </div>

            {#if !device.accessible}
              <div class="mt-3 p-2 rounded bg-error-muted text-error text-xs">
                Cannot access: requires administrator/root privileges
              </div>
            {/if}
          </button>
        {/each}

        {#if devices.length === 0}
          <div class="col-span-full card p-8 text-center text-text-muted">
            <HardDrive class="w-12 h-12 mx-auto mb-3 text-text-subtle" />
            <p>No drives detected</p>
            <p class="text-xs mt-1">Try running as administrator/root</p>
          </div>
        {/if}
      </div>
    </section>

    <!-- Advanced Options -->
    {#if showAdvanced}
      <section class="mb-6 card p-4 animate-fade-in">
        <h3 class="text-sm font-medium text-text mb-3 flex items-center gap-2">
          <Settings class="w-4 h-4" />
          Advanced scan options
        </h3>
        <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <div>
            <label class="label">Scan mode</label>
            <select bind:value={scanMode} class="input">
              <option value="quick">Quick (metadata only)</option>
              <option value="deep">Deep (metadata + carving)</option>
              <option value="formatted">Formatted partition</option>
              <option value="lost">Lost partition</option>
            </select>
          </div>
          <div>
            <label class="label">Include live files</label>
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="checkbox" bind:checked={includeLive} class="w-4 h-4 accent-accent" />
              <span class="text-sm text-text">List non-deleted files</span>
            </label>
          </div>
          <div>
            <label class="label">Worker threads</label>
            <input type="number" bind:value={threadCount} min="1" max="64" class="input" />
          </div>
        </div>
      </section>
    {/if}

    <!-- Primary Action -->
    <div class="sticky bottom-0 bg-bg/80 backdrop-blur-sm border-t border-border px-4 py-4 flex justify-end">
      <button
        class="btn w-full sm:w-auto min-w-[200px]"
        class:btn-primary={primaryButtonVariant === 'primary'}
        class:btn-secondary={primaryButtonVariant === 'secondary'}
        onclick={startScan}
        disabled={scanDisabled || loading}
      >
        {#if loading}
          <svg class="animate-spin -ml-1 mr-2 h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
          Scanning…
        {:else}
          {primaryButtonText}
        {/if}
      </button>
    </div>
  </main>
</div>