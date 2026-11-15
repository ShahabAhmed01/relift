<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { 
    Search, 
    Filter, 
    ChevronRight, 
    ChevronDown,
    Check, 
    CheckCheck,
    Minus,
    FileText,
    Image,
    Video,
    Music,
    Archive,
    Database,
    HardDrive,
    Folder,
    Eye,
    Download,
    MoreHorizontal,
    RotateCcw,
    X,
    Loader2
  } from 'lucide-svelte';

  // Types
  interface Candidate {
    id: number;
    name: string;
    path: string;
    size: number;
    size_human: string;
    format: string;
    confidence: string;
    confidence_score: number;
    confidence_reasons: string[];
    method: string;
    state: string;
    modified?: string;
    accessed?: string;
    created?: string;
    volume_offset: number;
    record_id?: string;
    complete: boolean;
    validation: string[];
  }

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
  let candidates = $state<Candidate[]>([]);
  let selectedIds = $state<Set<number>>(new Set());
  let searchQuery = $state('');
  let selectedType = $state<string>('all');
  let selectedConfidence = $state<string>('all');
  let sortColumn = $state<string>('name');
  let sortDirection = $state<'asc' | 'desc'>('asc');
  let viewMode = $state<'table' | 'tree'>('table');
  let showPreview = $state(false);
  let previewCandidate: Candidate | null = $state(null);
  let destination = $state('');
  let recovering = $state(false);
  let recoveryProgress = $state({ index: 0, total: 0, percent: 0 });
  let recoverySummary: RecoverySummary | null = $state(null);
  let loading = $state(true);

  // File type categories
  const typeCategories = [
    { value: 'all', label: 'All types', icon: null },
    { value: 'image', label: 'Images', icon: Image },
    { value: 'video', label: 'Videos', icon: Video },
    { value: 'audio', label: 'Audio', icon: Music },
    { value: 'document', label: 'Documents', icon: FileText },
    { value: 'archive', label: 'Archives', icon: Archive },
    { value: 'database', label: 'Databases', icon: Database },
    { value: 'other', label: 'Other', icon: null }
  ];

  const confidenceLevels = [
    { value: 'all', label: 'All confidence' },
    { value: 'verified', label: 'Verified' },
    { value: 'likely', label: 'Likely' },
    { value: 'partial', label: 'Partial' },
    { value: 'unverified', label: 'Unverified' }
  ];

  // Computed
  let totalSelected = $derived(selectedIds.size);
  let totalSelectedSize = $derived(
    Array.from(selectedIds).reduce((sum, id) => {
      const c = candidates.find(x => x.id === id);
      return sum + (c?.size || 0);
    }, 0)
  );

  // Filtering and sorting are derived from the raw candidate list plus the
  // current query/sort state, so no manual recomputation is needed.
  let filteredCandidates = $derived.by(() => {
    let result = candidates;

    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      result = result.filter(c =>
        c.name.toLowerCase().includes(q) ||
        c.path.toLowerCase().includes(q) ||
        c.format.toLowerCase().includes(q)
      );
    }

    if (selectedType !== 'all') {
      result = result.filter(c => getTypeCategory(c.format) === selectedType);
    }

    if (selectedConfidence !== 'all') {
      result = result.filter(c => c.confidence === selectedConfidence);
    }

    return [...result].sort((a, b) => {
      let aVal: any = a[sortColumn as keyof Candidate];
      let bVal: any = b[sortColumn as keyof Candidate];

      if (typeof aVal === 'string') {
        aVal = aVal.toLowerCase();
        bVal = bVal.toLowerCase();
      }

      if (aVal < bVal) return sortDirection === 'asc' ? -1 : 1;
      if (aVal > bVal) return sortDirection === 'asc' ? 1 : -1;
      return 0;
    });
  });

  let selectAllState = $derived.by<'none' | 'all' | 'partial'>(() => {
    const visible = filteredCandidates.map(c => c.id);
    if (visible.length === 0) return 'none';
    if (visible.every(id => selectedIds.has(id))) return 'all';
    if (visible.some(id => selectedIds.has(id))) return 'partial';
    return 'none';
  });

  let selectAllIndeterminate = $derived(selectAllState === 'partial');
  let selectAllChecked = $derived(selectAllState === 'all');

  // Format helpers
  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  function getTypeCategory(format: string): string {
    const img = ['jpg', 'png', 'gif', 'bmp', 'tiff', 'webp', 'heic', 'avif'];
    const vid = ['mp4', 'mov', 'mkv', 'avi', 'webm', 'flv', 'wmv', '3gp'];
    const aud = ['mp3', 'wav', 'flac', 'ogg', 'aac', 'm4a'];
    const doc = ['pdf', 'docx', 'xlsx', 'pptx', 'doc', 'xls', 'ppt', 'rtf', 'txt', 'odt', 'ods', 'odp'];
    const arc = ['zip', 'rar', '7z', 'gz', 'tar', 'bz2', 'xz', 'zst'];
    const db = ['sqlite', 'mdb', 'accdb'];
    
    if (img.includes(format)) return 'image';
    if (vid.includes(format)) return 'video';
    if (aud.includes(format)) return 'audio';
    if (doc.includes(format)) return 'document';
    if (arc.includes(format)) return 'archive';
    if (db.includes(format)) return 'database';
    return 'other';
  }

  function getConfidenceBadge(confidence: string) {
    const badges: Record<string, string> = {
      verified: 'badge-verified',
      likely: 'badge-likely',
      partial: 'badge-partial',
      unverified: 'badge-unverified'
    };
    return badges[confidence] || 'badge-unverified';
  }

  function getFileIcon(format: string) {
    const cat = getTypeCategory(format);
    const icons: Record<string, any> = {
      image: Image,
      video: Video,
      audio: Music,
      document: FileText,
      archive: Archive,
      database: Database
    };
    return icons[cat] || FileText;
  }

  function toggleSelectAll() {
    if (selectAllState === 'all') {
      // Deselect all visible
      filteredCandidates.forEach(c => selectedIds.delete(c.id));
    } else {
      // Select all visible
      filteredCandidates.forEach(c => selectedIds.add(c.id));
    }
  }

  function toggleSelect(id: number) {
    if (selectedIds.has(id)) {
      selectedIds.delete(id);
    } else {
      selectedIds.add(id);
    }
  }

  function selectHighConfidence() {
    candidates
      .filter(c => c.confidence === 'verified' || c.confidence === 'likely')
      .forEach(c => selectedIds.add(c.id));
  }

  function clearSelection() {
    selectedIds.clear();
  }

  function handleSort(column: string) {
    if (sortColumn === column) {
      sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
    } else {
      sortColumn = column;
      sortDirection = 'asc';
    }
  }

  async function loadCandidates() {
    loading = true;
    try {
      const sessionDir = sessionStorage.getItem('relift_session');
      if (sessionDir) {
        const session = JSON.parse(sessionDir);
        const result = await invoke<Candidate[]>('list_candidates_cmd', { session_dir: session.session_dir });
        candidates = result;
      }
    } catch (e) {
      console.error('Failed to load candidates:', e);
    } finally {
      loading = false;
    }
  }

  async function previewFile(candidate: Candidate) {
    previewCandidate = candidate;
    showPreview = true;
  }

  async function startRecovery() {
    if (totalSelected === 0) {
      alert('No files selected');
      return;
    }
    
    if (!destination) {
      alert('Please select a destination folder');
      return;
    }
    
    const sessionDir = JSON.parse(sessionStorage.getItem('relift_session') || '{}').session_dir;
    if (!sessionDir) {
      alert('Session not found');
      return;
    }
    
    recovering = true;
    recoverySummary = null;
    
    try {
      const unlisten = await listen('recover-progress', (event) => {
        recoveryProgress = event.payload;
      });
      
      const summary = await invoke<RecoverySummary>('recover_cmd', {
        options: {
          session_dir: sessionDir,
          dest: destination,
          min_confidence: selectedConfidence === 'all' ? undefined : selectedConfidence,
          ids: Array.from(selectedIds),
          verify: true
        }
      });
      
      unlisten();
      recoverySummary = summary;
      recovering = false;
    } catch (e) {
      console.error('Recovery failed:', e);
      recovering = false;
      alert(`Recovery failed: ${e}`);
    }
  }

  async function selectDestination() {
    try {
      const path = await invoke<string>('select_directory');
      if (path) {
        destination = path;
      }
    } catch (e) {
      console.error('Destination selection failed:', e);
    }
  }

  onMount(async () => {
    await loadCandidates();
  });
</script>

<div class="min-h-screen flex flex-col">
  <!-- Header -->
  <header class="border-b border-border px-4 py-3 flex items-center justify-between sticky top-0 z-40 bg-bg/95 backdrop-blur-sm">
    <div class="flex items-center gap-3">
      <button class="btn-ghost p-2" onclick={() => goto('/scan')} aria-label="Back">
        <RotateCcw class="w-5 h-5" />
      </button>
      <div>
        <h1 class="text-lg font-semibold text-text">Results</h1>
        <p class="text-xs text-text-muted">
          {candidates.length} candidates found · {filteredCandidates.length} visible
        </p>
      </div>
    </div>
    <div class="flex items-center gap-2">
      <span class="text-xs text-text-muted monospace">
        {totalSelected} selected · {formatBytes(totalSelectedSize)}
      </span>
    </div>
  </header>

  <!-- Main Content -->
  <main class="flex-1 overflow-hidden flex">
    <!-- Sidebar: Filters & Folder Tree -->
    <aside class="w-72 flex-shrink-0 border-r border-border flex flex-col">
      <!-- Filter Bar -->
      <div class="p-3 border-b border-border space-y-3">
        <div>
          <label class="label">Search</label>
          <div class="relative">
            <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-subtle" />
            <input
              type="text"
              bind:value={searchQuery}
              placeholder="Search name, path, format…"
              class="input pl-9"
            />
          </div>
        </div>
        
        <div>
          <label class="label">Type</label>
          <select bind:value={selectedType} class="input">
            {#each typeCategories as cat}
              <option value={cat.value}>{cat.label}</option>
            {/each}
          </select>
        </div>
        
        <div>
          <label class="label">Confidence</label>
          <select bind:value={selectedConfidence} class="input">
            {#each confidenceLevels as conf}
              <option value={cat.value}>{cat.label}</option>
            {/each}
          </select>
        </div>
        
        <div class="flex gap-2">
          <button class="btn-ghost text-xs flex-1" onclick={selectHighConfidence}>
            <CheckCheck class="w-3 h-3" /> Select Verified+
          </button>
          <button class="btn-ghost text-xs flex-1" onclick={clearSelection}>
            <X class="w-3 h-3" /> Clear
          </button>
        </div>
      </div>

      <!-- Folder Tree (placeholder) -->
      <div class="flex-1 overflow-auto p-3 border-b border-border">
        <h3 class="text-xs font-medium text-text-muted px-2 mb-2">Folders</h3>
        <div class="text-sm text-text-muted text-center py-8">
          Tree view coming soon
        </div>
      </div>
    </aside>

    <!-- Main Area: Results Table + Preview -->
    <div class="flex-1 flex flex-col overflow-hidden">
      <!-- Table Toolbar -->
      <div class="p-3 border-b border-border flex flex-wrap items-center gap-3">
        <div class="flex items-center gap-2">
          <label class="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={selectAllChecked}
              indeterminate={selectAllIndeterminate}
              onchange={toggleSelectAll}
              class="w-4 h-4 accent-accent"
            />
            <span class="text-sm text-text-muted">Select all visible</span>
          </label>
        </div>
        <div class="flex-1"></div>
        <div class="flex items-center gap-2">
          <label class="text-xs text-text-muted">View:</label>
          <div class="flex border border-border rounded-md overflow-hidden">
            <button
              class="btn-ghost px-2 py-1"
              class:bg-accent-muted={viewMode === 'table'}
              class:text-accent={viewMode === 'table'}
              onclick={() => viewMode = 'table'}
              aria-label="Table view"
            >
              <Table class="w-4 h-4" />
            </button>
            <button
              class="btn-ghost px-2 py-1"
              class:bg-accent-muted={viewMode === 'tree'}
              class:text-accent={viewMode === 'tree'}
              onclick={() => viewMode = 'tree'}
              aria-label="Tree view"
            >
              <Folder class="w-4 h-4" />
            </button>
          </div>
        </div>
      </div>

      <!-- Results Table -->
      <div class="flex-1 overflow-auto table-container scrollbar-thin">
        {#if loading}
          <div class="p-8 text-center text-text-muted">
            <Loader2 class="w-8 h-8 animate-spin mx-auto mb-2 text-accent" />
            <p>Loading candidates…</p>
          </div>
        {:else if filteredCandidates.length === 0}
          <div class="p-8 text-center text-text-muted">
            <Search class="w-12 h-12 mx-auto mb-2 text-text-subtle" />
            <p>No candidates match your filters</p>
          </div>
        {:else}
          <table class="table">
            <thead>
              <tr>
                <th class="w-10">
                  <input
                    type="checkbox"
                    checked={selectAllChecked}
                    indeterminate={selectAllIndeterminate}
                    onchange={toggleSelectAll}
                    class="w-4 h-4 accent-accent"
                    aria-label="Select all"
                  />
                </th>
                <th class="w-10"></th>
                <th onclick={() => handleSort('name')} class="cursor-pointer select-none flex items-center gap-1">
                  Name {sortColumn === 'name' ? (sortDirection === 'asc' ? '↑' : '↓') : ''}
                </th>
                <th onclick={() => handleSort('path')} class="cursor-pointer select-none flex items-center gap-1 hidden sm:table-cell">
                  Path {sortColumn === 'path' ? (sortDirection === 'asc' ? '↑' : '↓') : ''}
                </th>
                <th onclick={() => handleSort('size')} class="cursor-pointer select-none flex items-center gap-1 monospace">
                  Size {sortColumn === 'size' ? (sortDirection === 'asc' ? '↑' : '↓') : ''}
                </th>
                <th onclick={() => handleSort('format')} class="cursor-pointer select-none flex items-center gap-1 hidden md:table-cell">
                  Type {sortColumn === 'format' ? (sortDirection === 'asc' ? '↑' : '↓') : ''}
                </th>
                <th onclick={() => handleSort('confidence')} class="cursor-pointer select-none flex items-center gap-1">
                  Confidence {sortColumn === 'confidence' ? (sortDirection === 'asc' ? '↑' : '↓') : ''}
                </th>
                <th onclick={() => handleSort('modified')} class="cursor-pointer select-none flex items-center gap-1 hidden lg:table-cell">
                  Modified {sortColumn === 'modified' ? (sortDirection === 'asc' ? '↑' : '↓') : ''}
                </th>
                <th class="w-10"></th>
              </tr>
            </thead>
            <tbody>
              {#each filteredCandidates as candidate}
                <tr class={candidate.complete ? '' : 'opacity-60'} class:selected={selectedIds.has(candidate.id)}>
                  <td>
                    <input
                      type="checkbox"
                      checked={selectedIds.has(candidate.id)}
                      onchange={() => toggleSelect(candidate.id)}
                      class="w-4 h-4 accent-accent"
                    />
                  </td>
                  <td>
                    <svelte:component this={getFileIcon(candidate.format)} class="w-4 h-4 text-text-muted" />
                  </td>
                  <td class="font-mono truncate max-w-[200px]" title={candidate.name}>
                    {candidate.name}
                  </td>
                  <td class="hidden sm:table-cell text-text-muted truncate max-w-[200px]" title={candidate.path}>
                    {candidate.path || '/'}
                  </td>
                  <td class="monospace text-text">{candidate.size_human}</td>
                  <td class="hidden md:table-cell">
                    <span class="text-xs text-text-muted uppercase">{candidate.format}</span>
                  </td>
                  <td>
                    <span class={getConfidenceBadge(candidate.confidence)}>
                      {candidate.confidence.charAt(0).toUpperCase() + candidate.confidence.slice(1)}
                    </span>
                  </td>
                  <td class="hidden lg:table-cell text-text-muted monospace text-xs">
                    {candidate.modified ? new Date(candidate.modified).toLocaleString() : '—'}
                  </td>
                  <td>
                    <button
                      class="btn-ghost p-1"
                      onclick={() => previewFile(candidate)}
                      aria-label="Preview"
                    >
                      <Eye class="w-4 h-4" />
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>

      <!-- Recovery Bar -->
      <div class="border-t border-border p-3 bg-bg/80 backdrop-blur-sm flex items-center justify-between gap-4">
        <div class="flex items-center gap-3">
          <label class="flex items-center gap-2">
            <HardDrive class="w-4 h-4" />
            <input
              type="text"
              bind:value={destination}
              placeholder="Destination folder (different drive)"
              readonly
              class="input w-64 bg-surface"
            />
            <button class="btn-ghost p-2" onclick={selectDestination} aria-label="Browse">
              <MoreHorizontal class="w-4 h-4" />
            </button>
          </label>
        </div>
        <div class="flex-1"></div>
        <div class="flex items-center gap-3">
          <span class="text-sm text-text-muted">
            {totalSelected} files · {formatBytes(totalSelectedSize)}
          </span>
          <button
            class="btn-primary min-w-[160px]"
            onclick={startRecovery}
            disabled={recovering || totalSelected === 0 || !destination}
          >
            {#if recovering}
              <Loader2 class="animate-spin w-4 h-4" /> Recovering…
            {:else}
              <Download class="w-4 h-4" /> Recover {totalSelected} files
            {/if}
          </button>
        </div>
      </div>
    </div>
  </main>

  <!-- Preview Panel -->
  {#if showPreview && previewCandidate}
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm">
      <div class="bg-surface border border-border rounded-lg shadow-lg w-full max-w-3xl max-h-[80vh] flex flex-col overflow-hidden">
        <div class="p-3 border-b border-border flex items-center justify-between">
          <div class="flex items-center gap-3">
            <svelte:component this={getFileIcon(previewCandidate.format)} class="w-6 h-6 text-accent" />
            <div>
              <p class="font-medium text-text truncate max-w-[300px]">{previewCandidate.name}</p>
              <p class="text-xs text-text-muted">{previewCandidate.path}</p>
            </div>
          </div>
          <button class="btn-ghost p-2" onclick={() => { showPreview = false; previewCandidate = null; }}>
            <X class="w-5 h-5" />
          </button>
        </div>
        <div class="p-4 overflow-auto flex-1">
          <div class="grid gap-3">
            <div class="card p-3">
              <h4 class="text-sm font-medium text-text-muted mb-2">Metadata</h4>
              <dl class="grid grid-cols-2 gap-2 text-sm">
                <dt class="text-text-muted">Format</dt>
                <dd class="text-text font-mono">{previewCandidate.format}</dd>
                <dt class="text-text-muted">Size</dt>
                <dd class="text-text monospace">{previewCandidate.size_human}</dd>
                <dt class="text-text-muted">Confidence</dt>
                <dd class="text-text"><span class={getConfidenceBadge(previewCandidate.confidence)}>{previewCandidate.confidence}</span></dd>
                <dt class="text-text-muted">Method</dt>
                <dd class="text-text capitalize">{previewCandidate.method}</dd>
                <dt class="text-text-muted">Modified</dt>
                <dd class="text-text">{previewCandidate.modified || '—'}</dd>
                <dt class="text-text-muted">Created</dt>
                <dd class="text-text">{previewCandidate.created || '—'}</dd>
                <dt class="text-text-muted">State</dt>
                <dd class="text-text capitalize">{previewCandidate.state}</dd>
                <dt class="text-text-muted">Complete</dt>
                <dd class="text-text">{previewCandidate.complete ? 'Yes' : 'Partial'}</dd>
              </dl>
            </div>
            {#if previewCandidate.confidence_reasons.length}
              <div class="card p-3">
                <h4 class="text-sm font-medium text-text-muted mb-2">Confidence reasons</h4>
                <ul class="text-sm text-text space-y-1">
                  {#each previewCandidate.confidence_reasons as reason}
                    <li class="flex items-start gap-2">
                      <CheckCircle class="w-4 h-4 flex-shrink-0 text-success mt-0.5" />
                      <span>{reason}</span>
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}
            {#if previewCandidate.validation.length}
              <div class="card p-3">
                <h4 class="text-sm font-medium text-text-muted mb-2">Validation</h4>
                <ul class="text-sm text-text space-y-1">
                  {#each previewCandidate.validation as v}
                    <li class="flex items-start gap-2">
                      <CheckCircle class="w-4 h-4 flex-shrink-0 text-success mt-0.5" />
                      <span>{v}</span>
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}
          </div>
        </div>
      </div>
    </div>
  {/if}

  <!-- Recovery Complete Modal -->
  {#if recoverySummary && !recovering}
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm">
      <div class="bg-surface border border-border rounded-lg shadow-lg w-full max-w-md p-6">
        <div class="text-center mb-4">
          <div class="w-16 h-16 rounded-full bg-success-muted flex items-center justify-center mx-auto mb-3">
            <CheckCircle class="w-8 h-8 text-success" />
          </div>
          <h2 class="text-lg font-semibold text-text">Recovery Complete</h2>
        </div>
        <dl class="grid grid-cols-2 gap-2 text-sm mb-4">
          <dt class="text-text-muted">Recovered</dt>
          <dd class="text-text font-medium">{recoverySummary.succeeded}</dd>
          <dt class="text-text-muted">Partial</dt>
          <dd class="text-text font-medium text-warning">{recoverySummary.partial}</dd>
          <dt class="text-text-muted">Failed</dt>
          <dd class="text-text font-medium text-error">{recoverySummary.failed}</dd>
          <dt class="text-text-muted">Bytes</dt>
          <dd class="text-text font-medium monospace">{recoverySummary.bytes_written_human}</dd>
        </dl>
        <div class="flex gap-2">
          <button class="btn-secondary flex-1" onclick={() => { recoverySummary = null; }}>
            Close
          </button>
          <button class="btn-primary flex-1" onclick={() => { recoverySummary = null; goto('/'); }}>
            Recover More
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>