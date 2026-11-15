<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { 
    ArrowLeft,
    StopCircle, 
    ChevronRight,
    AlertTriangle,
    XCircle,
    Loader2
  } from '@lucide/svelte';

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

  interface ScanRequest {
    source: string;
    session_dir: string;
    include_live: boolean;
    carve: boolean;
    volume_indices: number[];
    max_candidates: number;
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
  let scanning = $state(false);
  let stopping = $state(false);
  let scanError = $state('');
  let candidatesFound = $state(0);
  let startTime = $state<number>(Date.now());
  let elapsed = $state(0);
  let eta = $state(0);
  let throughput = $state(0);
  let unlistenProgress: (() => void) | null = null;
  let scanTask: Promise<void> | null = null;

  // Computed
  let progressPercent = $derived(progress.total > 0 ? Math.min(100, Math.round((progress.done / progress.total) * 100)) : 0);

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
    const stored = sessionStorage.getItem('relift_scan_request');
    if (stored) {
      const request = JSON.parse(stored) as ScanRequest;
      sessionStorage.removeItem('relift_scan_request');
      session = {
        session_dir: request.session_dir,
        source_path: request.source,
        source_size: 0,
        source_size_human: 'Unknown',
        status: 'scanning',
        filesystems: [],
        candidates: 0,
        deleted: 0,
        live: 0,
        with_content: 0,
        metadata_only: 0,
        warnings: []
      };
      sessionStorage.setItem('relift_session', JSON.stringify(session));
      await startScan(request);
    } else {
      goto('/');
    }
  }

  async function startScan(request: ScanRequest) {
    scanning = true;
    scanError = '';
    startTime = Date.now();
    
    try {
      unlistenProgress = await listen<ScanProgress>('scan-progress', (event) => {
        progress = event.payload;
        updateMetrics();
      });
    } catch (e) {
      console.error('Failed to listen for progress:', e);
    }

    scanTask = invoke<SessionSummary>('scan_cmd', { options: request })
      .then((summary) => {
        session = summary;
        candidatesFound = summary.candidates;
        sessionStorage.setItem('relift_session', JSON.stringify(summary));
      })
      .catch((error) => {
        scanError = String(error);
      })
      .finally(() => {
        scanning = false;
        stopping = false;
        unlistenProgress?.();
        unlistenProgress = null;
      });

    await scanTask;
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
    
  }

  async function refreshCandidateCount() {
    if (!session?.session_dir) return;
    try {
      const candidates = await invoke<unknown[]>('list_candidates_cmd', {
        sessionDir: session.session_dir
      });
      candidatesFound = candidates.length;
    } catch {
      // The session candidate file may not exist until the first result is written.
    }
  }

  async function stopScan() {
    if (!scanning || stopping || !confirm('Stop scan and keep current results?')) return;

    stopping = true;
    await invoke('cancel_scan_cmd');
    await scanTask;
    goto('/results');
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

  onMount(() => {
    const interval = setInterval(() => {
      if (scanning) {
        updateMetrics();
        void refreshCandidateCount();
      }
    }, 1000);

    void loadSession();
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
        <button class="btn-ghost p-2" onclick={() => scanning ? void stopScan() : goto('/')} aria-label="Back" disabled={stopping}>
        <ArrowLeft class="w-5 h-5" />
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
        <h2 class="text-base font-medium text-text mb-3">Candidates saved so far</h2>
        <p class="text-sm text-text-muted">
          Review candidates after the scan completes or after stopping it.
        </p>
      </section>
    {/if}

    {#if scanError}
      <section class="card p-4 border-error/50 bg-error-muted/30" role="alert">
        <h2 class="text-sm font-medium text-error mb-2">Scan failed</h2>
        <p class="text-sm text-text-muted break-words">{scanError}</p>
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
        onclick={stopScan}
        disabled={!scanning || stopping}
      >
        <StopCircle class="w-4 h-4" /> {stopping ? 'Stopping…' : 'Stop and keep results'}
      </button>
      {#if !scanning && session}
        <a href="/results" class="btn-primary">
          <ChevronRight class="w-4 h-4" />
          View saved results
        </a>
      {/if}
    </div>
  </main>
</div>
