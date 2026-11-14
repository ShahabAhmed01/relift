<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { invoke, listen } from '@tauri-apps/api/core';
  import { 
    Pause, 
    StopCircle, 
    RotateCcw, 
    HardDrive, 
    Database, 
    Search, 
    Filter, 
    ChevronRight,
    AlertTriangle,
    CheckCircle,
    XCircle,
    Loader2
  } from 'lucide-svelte';
  
  import { Table } from 'lucide-svelte';

  // Types
  interface ScanProgress {
    phase: string;
    done: number;
    total: number;
    percent: number;
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
  let session: SessionSummary | null = $state(null);
  let progress = $state<ScanProgress>({ phase: 'probe', done: 0, total: 0, percent: 0 });
  let scanning = $state(true);
  let paused = $state(false);
  let candidatesFound = $state(0);
  let startTime = $state<number>(Date.now());
  let elapsed = $state(0);
  let eta = $state(0);
  let throughput = $state(0);
  let unlistenProgress: (() => void) | null = null;
  let phaseStartTime = $state<number>(Date.now());

  // Computed
  let progressPercent = $derived(progress.total > 0 ? Math.min(100, Math.round((progress.done / progress.total) * 100)) : 0);
  let canPause = $derived(scanning && !paused);
  let canResume = $derived(scanning && paused);

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  function formatDuration(ms: number): string {
    const s = Math.floor(ms / 1000);
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    if (h > 0) return `${h}h ${m}m ${sec}s`;
    if (m > 0) return `${m}m ${sec}s`;
    return `${sec}s`;
  }

  function formatThroughput(bytesPerSec: number): string {
    return formatBytes(bytesPerSec) + '/s';
  }

  async function loadSession() {
    const stored = sessionStorage.getItem('relift_session');
    if (stored) {
      session = JSON.parse(stored);
      startScan();
    } else {
      goto('/');
    }
  }

  async function startScan() {
    if (!session) return;
    
    scanning = true;
    paused = false;
    startTime = Date.now();
    phaseStartTime = Date.now();
    
    try {
      unlistenProgress = await listen<ScanProgress>('scan-progress', (event) => {
        progress = event.payload;
        updateMetrics();
      });
    } catch (e) {
      console.error('Failed to listen for progress:', e);
    }
  }

  function updateMetrics() {
    const now = Date.now();
    elapsed = now - startTime;
    
    if (progress.done > 0 && progress.total > 0 && elapsed > 1000) {
      throughput = progress.done / (elapsed / 1000);
      const remaining = progress.total - progress.done;
      if (throughput > 0) {
        eta = remaining / throughput * 1000;
      }
    }
    
    candidatesFound = session?.candidates || 0;
  }

  async function togglePause() {
    if (paused) {
      // Resume - not implemented in backend yet
      paused = false;
    } else {
      paused = true;
      await invoke('cancel_scan_cmd');
    }
  }

  async function stopScan() {
    if (confirm('Stop scan and keep current results?')) {
      scanning = false;
      await invoke('cancel_scan_cmd');
      // Navigate to results with partial data
      goto('/results');
    }
  }

  function getPhaseLabel(phase: string): string {
    const labels: Record<string, string> = {
      probe: 'Probing',
      metadata: 'Metadata scan',
      carve: 'Carving',
      done: 'Complete'
    };
    return labels[phase] || phase;
  }

  onMount(async () => {
    await loadSession();
    
    // Update timer
    const interval = setInterval(() => {
      if (scanning && !paused) {
        updateMetrics();
      }
    }, 1000);
    
    return () => clearInterval(interval);
  });

  onDestroy(() => {
    if (unlistenProgress) {
      unlistenProgress();
    }
  });
</script>

<div class="min-h-screen flex flex-col">
  <!-- Header -->
  <header class="border-b border-border px-4 py-3 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <button class="btn-ghost p-2" on:click={() => goto('/')} aria-label="Back">
        <RotateCcw class="w-5 h-5" />
      </button>
      <div>
        <h1 class="text-lg font-semibold text-text">Scanning</h1>
        <p class="text-xs text-text-muted">{session?.source_path || 'Unknown source'}</p>
      </div>
    </div>
    <div class="flex items-center gap-2">
      <span class="text-xs text-text-muted monospace">{formatDuration(elapsed)}</span>
    </div>
  </header>

  <!-- Main Content -->
  <main class="flex-1 overflow-auto p-4 space-y-4">
    <!-- Progress Section -->
    <section class="card p-4">
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-base font-medium text-text flex items-center gap-2">
          <Loader2 class="w-4 h-4 animate-spin text-accent" />
          {getPhaseLabel(progress.phase)} <span class="text-text-muted">({progressPercent}%)</span>
        </h2>
        <div class="flex items-center gap-2 text-sm text-text-muted">
          <span class="monospace">{formatBytes(progress.done)} / {formatBytes(progress.total)}</span>
        </div>
      </div>
      
      <div class="progress-bar mb-3">
        <div class="progress-fill" style="width: {progressPercent}%"></div>
      </div>

      <div class="grid grid-cols-2 sm:grid-cols-4 gap-4 text-sm">
        <div class="flex flex-col">
          <span class="text-text-muted">Throughput</span>
          <span class="font-mono text-text">{formatThroughput(throughput)}</span>
        </div>
        <div class="flex flex-col">
          <span class="text-text-muted">Elapsed</span>
          <span class="font-mono text-text">{formatDuration(elapsed)}</span>
        </div>
        <div class="flex flex-col">
          <span class="text-text-muted">ETA</span>
          <span class="font-mono text-text">{eta > 0 ? formatDuration(eta) : 'calculating…'}</span>
        </div>
        <div class="flex flex-col">
          <span class="text-text-muted">Found</span>
          <span class="font-mono text-text">{candidatesFound.toLocaleString()}</span>
        </div>
      </div>

      <!-- Phase breakdown if available -->
      {#if session?.filesystems}
        <div class="mt-4 pt-4 border-t border-border">
          <div class="flex flex-wrap gap-2">
            {#each session.filesystems as fs}
              <div class="badge bg-surface border border-border px-3 py-1.5">
                <span class="font-medium">{fs.kind?.toUpperCase() || 'Volume ' + fs.volume_index}</span>
                <span class="text-text-muted ml-2">{fs.candidates} files</span>
                {#if fs.deleted > 0}
                  <span class="badge-deleted ml-1">{fs.deleted} deleted</span>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </section>

    <!-- Live Results Preview (if any) -->
    {#if candidatesFound > 0}
      <section class="card p-4">
        <div class="flex items-center justify-between mb-3">
          <h2 class="text-base font-medium text-text">Results found so far</h2>
          <button class="btn-secondary text-sm" on:click={() => goto('/results')}>
            <Search class="w-4 h-4" />
            View all results
          </button>
        </div>
        <p class="text-sm text-text-muted">
          Results are streaming in. You can preview and recover files before the scan completes.
        </p>
      </section>
    {/if}

    <!-- Warnings -->
    {#if session?.warnings.length}
      <section class="card p-4 border-border/50 bg-warning-muted/30">
        <h3 class="text-sm font-medium text-warning flex items-center gap-2 mb-2">
          <AlertTriangle class="w-4 h-4" />
          Warnings
        </h3>
        <ul class="text-sm text-text-muted space-y-1">
          {#each session.warnings as warning}
            <li class="flex items-start gap-2">
              <XCircle class="w-4 h-4 flex-shrink-0 mt-0.5" />
              <span>{warning}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <!-- Controls -->
    <div class="sticky bottom-0 bg-bg/80 backdrop-blur-sm border-t border-border px-4 py-4 flex items-center justify-end gap-3">
      <button
        class="btn-secondary"
        on:click={togglePause}
        disabled={!scanning}
      >
        {#if paused}
          <RotateCcw class="w-4 h-4" /> Resume
        {:else}
          <Pause class="w-4 h-4" /> Pause
        {/if}
      </button>
      <button
        class="btn-secondary"
        on:click={stopScan}
        disabled={!scanning}
      >
        <StopCircle class="w-4 h-4" /> Stop & keep results
      </button>
      {#if !scanning && progressPercent === 100}
        <a href="/results" class="btn-primary">
          <ChevronRight class="w-4 h-4" />
          View results
        </a>
      {/if}
    </div>
  </main>
</div>