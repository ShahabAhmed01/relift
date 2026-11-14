<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import { 
    CheckCircle, 
    AlertTriangle, 
    XCircle, 
    Folder, 
    Download, 
    RotateCcw,
    HardDrive,
    FileText,
    Loader2
  } from 'lucide-svelte';

  // Types
  interface RecoverySummary {
    destination: string;
    considered: number;
    attempted: number;
    succeeded: number;
    partial: number;
    failed: number;
    skipped: number;
    bytes_written: number;
    bytes_written_human: string;
    warnings: string[];
  }

  // State
  let summary: RecoverySummary | null = $state(null);
  let recovering = $state(false);
  let progress = $state({ index: 0, total: 0, percent: 0 });
  let results: Array<{name: string; status: string; path?: string; note?: string}> = $state([]);

  // Computed
  let hasFailures = $derived(summary && (summary.failed > 0 || summary.partial > 0));
  let successRate = $derived(summary ? Math.round((summary.succeeded / summary.attempted) * 100) : 0);

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  async function startRecovery() {
    recovering = true;
    results = [];
    
    try {
      const sessionDir = JSON.parse(sessionStorage.getItem('relift_session') || '{}').session_dir;
      if (!sessionDir) {
        throw new Error('Session not found');
      }
      
      const dest = await invoke<string>('select_directory');
      if (!dest) {
        throw new Error('No destination selected');
      }
      
      const unlisten = await listen('recover-progress', (event) => {
        progress = event.payload;
      });
      
      const result = await invoke<RecoverySummary>('recover_cmd', {
        options: {
          session_dir: sessionDir,
          dest,
          verify: true
        }
      });
      
      unlisten();
      summary = result;
      recovering = false;
    } catch (e) {
      console.error('Recovery failed:', e);
      recovering = false;
      alert(`Recovery failed: ${e}`);
    }
  }

  function goHome() {
    sessionStorage.removeItem('relift_session');
    goto('/');
  }

  onMount(async () => {
    const stored = sessionStorage.getItem('relift_session');
    if (!stored) {
      goto('/');
    }
  });
</script>

<div class="min-h-screen flex flex-col">
  <!-- Header -->
  <header class="border-b border-border px-4 py-3 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <button class="btn-ghost p-2" on:click={goHome} aria-label="Home">
        <RotateCcw class="w-5 h-5" />
      </button>
      <div>
        <h1 class="text-lg font-semibold text-text">Recovery</h1>
        <p class="text-xs text-text-muted">Select destination and recover files</p>
      </div>
    </div>
  </header>

  <!-- Main Content -->
  <main class="flex-1 overflow-auto p-4 max-w-3xl mx-auto w-full">
    {#if recovering}
      <!-- Recovery in Progress -->
      <section class="card p-6 text-center">
        <div class="w-16 h-16 rounded-full bg-accent-muted flex items-center justify-center mx-auto mb-4">
          <Loader2 class="w-8 h-8 text-accent animate-spin" />
        </div>
        <h2 class="text-lg font-semibold text-text mb-2">Recovering files…</h2>
        <p class="text-text-muted mb-6">{progress.index} / {progress.total}</p>
        
        <div class="progress-bar max-w-md mx-auto mb-4">
          <div class="progress-fill" style="width: {progress.percent}%"></div>
        </div>
        
        <p class="text-sm text-text-muted">{progress.percent}% complete</p>
      </section>
    {:else if summary}
      <!-- Recovery Complete -->
      <section class="card p-6 space-y-6">
        <div class="text-center">
          <div class={`w-16 h-16 rounded-full flex items-center justify-center mx-auto mb-4 ${
            hasFailures ? 'bg-warning-muted' : 'bg-success-muted'
          }`}>
            {#if hasFailures}
              <AlertTriangle class="w-8 h-8 text-warning" />
            {:else}
              <CheckCircle class="w-8 h-8 text-success" />
            {/if}
          </div>
          <h2 class="text-xl font-semibold text-text mb-1">
            {hasFailures ? 'Recovery completed with issues' : 'Recovery completed successfully'}
          </h2>
          <p class="text-text-muted">{successRate}% success rate</p>
        </div>

        <dl class="grid grid-cols-2 gap-4">
          <div class="card p-4 text-center">
            <p class="text-3xl font-bold text-success">{summary.succeeded}</p>
            <p class="text-sm text-text-muted">Recovered</p>
          </div>
          <div class="card p-4 text-center">
            <p class="text-3xl font-bold text-warning">{summary.partial}</p>
            <p class="text-sm text-text-muted">Partial</p>
          </div>
          <div class="card p-4 text-center">
            <p class="text-3xl font-bold text-error">{summary.failed}</p>
            <p class="text-sm text-text-muted">Failed</p>
          </div>
          <div class="card p-4 text-center">
            <p class="text-3xl font-bold text-text">{formatBytes(summary.bytes_written)}</p>
            <p class="text-sm text-text-muted">Bytes written</p>
          </div>
        </dl>

        {#if summary.warnings.length}
          <div class="card p-4 border-border/50 bg-warning-muted/30">
            <h3 class="text-sm font-medium text-warning flex items-center gap-2 mb-2">
              <AlertTriangle class="w-4 h-4" />
              Warnings
            </h3>
            <ul class="text-sm text-text-muted space-y-1">
              {#each summary.warnings as warning}
                <li class="flex items-start gap-2">
                  <XCircle class="w-4 h-4 flex-shrink-0 mt-0.5" />
                  <span>{warning}</span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        <div class="flex gap-3 pt-4">
          <button class="btn-secondary flex-1" on:click={() => goto('/results')}>
            <RotateCcw class="w-4 h-4" />
            Back to results
          </button>
          <button class="btn-primary flex-1" on:click={goHome}>
            <HardDrive class="w-4 h-4" />
            New recovery
          </button>
        </div>
      </section>
    {:else}
      <!-- Destination Selection -->
      <section class="card p-6 space-y-6">
        <div class="text-center">
          <div class="w-16 h-16 rounded-full bg-accent-muted flex items-center justify-center mx-auto mb-4">
            <Download class="w-8 h-8 text-accent" />
          </div>
          <h2 class="text-xl font-semibold text-text mb-1">Choose recovery destination</h2>
          <p class="text-text-muted">Select a folder on a <strong>different physical drive</strong> than the source</p>
        </div>

        <div class="space-y-4">
          <div>
            <label class="label">Destination folder</label>
            <div class="flex gap-2">
              <input
                type="text"
                placeholder="Click Browse to select destination…"
                readonly
                class="input flex-1 bg-surface"
              />
              <button class="btn-secondary" on:click={startRecovery}>
                <Folder class="w-4 h-4" />
                Browse…
              </button>
            </div>
            <p class="text-xs text-text-muted mt-1">
              Must be on a different physical drive from the source. 
              Relift will verify this before writing.
            </p>
          </div>

          <div class="card p-4 border-border/50 bg-warning-muted/30">
            <h3 class="text-sm font-medium text-warning flex items-center gap-2 mb-2">
              <AlertTriangle class="w-4 h-4" />
              Safety reminder
            </h3>
            <ul class="text-sm text-text-muted space-y-1">
              <li class="flex items-start gap-2">
                <CheckCircle class="w-4 h-4 flex-shrink-0 text-success mt-0.5" />
                <span>Never recover to the same drive you're scanning</span>
              </li>
              <li class="flex items-start gap-2">
                <CheckCircle class="w-4 h-4 flex-shrink-0 text-success mt-0.5" />
                <span>Verify destination has enough free space</span>
              </li>
              <li class="flex items-start gap-2">
                <CheckCircle class="w-4 h-4 flex-shrink-0 text-success mt-0.5" />
                <span>SHA-256 verification runs automatically after recovery</span>
              </li>
            </ul>
          </div>
        </div>

        <div class="pt-4">
          <button class="btn-primary w-full min-h-[48px] text-base" on:click={startRecovery}>
            <Download class="w-5 h-5" />
            Start recovery
          </button>
        </div>
      </section>
    {/if}
  </main>
</div>