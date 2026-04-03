<script lang="ts">
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import type { Game, SortMode, FilterPlatform, FilterStatus, GroupMode } from '../lib/types';

  let {
    games = [],
    onLaunchGame = (_game: Game) => {},
    onAddGame = () => {},
    onGamesChanged = () => {},
    onSwitchMode = () => {},
    t: _t = (key: string) => key,
  }: {
    games: Game[];
    onLaunchGame: (game: Game) => void;
    onAddGame: () => void;
    onGamesChanged: () => void;
    onSwitchMode: () => void;
    t: (key: string) => string;
  } = $props();

  const t = (key: string) => _t(key);

  function getCoverUrl(coverPath: string | undefined): string | null {
    if (!coverPath) return null;
    if (coverPath.match(/^[A-Z]:/i)) return convertFileSrc(coverPath);
    return coverPath;
  }

  let viewMode = $state<'grid' | 'list'>('grid');
  let sortMode = $state<SortMode>('name');
  let groupMode = $state<GroupMode>('none');
  let filterPlatform = $state<FilterPlatform>('all');
  let filterStatus = $state<FilterStatus>('all');
  let searchQuery = $state('');
  let selectedGame = $state<Game | null>(null);
  let showHidden = $state(false);
  let selectedIds = $state<Set<string>>(new Set());
  let showBatchBar = $derived(selectedIds.size > 1);

  function toggleSelect(game: Game, e: MouseEvent) {
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(selectedIds);
      if (next.has(game.id)) next.delete(game.id); else next.add(game.id);
      selectedIds = next;
    } else if (e.shiftKey && selectedGame) {
      const list = filteredGames;
      const a = list.findIndex(g => g.id === selectedGame!.id);
      const b = list.findIndex(g => g.id === game.id);
      const [start, end] = a < b ? [a, b] : [b, a];
      const next = new Set<string>();
      for (let i = start; i <= end; i++) next.add(list[i].id);
      selectedIds = next;
    } else {
      selectedIds = new Set();
      selectedGame = game;
    }
  }

  async function batchSetStatus(status: string) {
    for (const id of selectedIds) {
      await invoke('update_game', { gameId: id, updates: { completionStatus: status } });
    }
    selectedIds = new Set();
    onGamesChanged();
  }

  async function batchDelete() {
    for (const id of selectedIds) {
      await invoke('delete_game', { gameId: id });
    }
    selectedIds = new Set();
    selectedGame = null;
    onGamesChanged();
  }

  async function batchHide() {
    for (const id of selectedIds) {
      await invoke('update_game', { gameId: id, updates: { hidden: true } });
    }
    selectedIds = new Set();
    onGamesChanged();
  }

  const filteredGames = $derived((() => {
    let list = [...games];
    if (!showHidden) list = list.filter(g => !g.hidden);
    if (filterPlatform !== 'all') list = list.filter(g => (g.source ?? 'manual') === filterPlatform);
    if (filterStatus !== 'all') list = list.filter(g => (g.completionStatus ?? 'not_played') === filterStatus);
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      list = list.filter(g =>
        g.name.toLowerCase().includes(q)
        || (g.developers?.some(d => d.toLowerCase().includes(q)) ?? false)
        || (g.publishers?.some(p => p.toLowerCase().includes(q)) ?? false)
        || (g.tags?.some(t => t.toLowerCase().includes(q)) ?? false)
        || (g.genre?.toLowerCase().includes(q) ?? false)
      );
    }
    switch (sortMode) {
      case 'name': list.sort((a, b) => a.name.localeCompare(b.name)); break;
      case 'recent': list.sort((a, b) => (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0)); break;
      case 'most_played': list.sort((a, b) => (b.totalPlayTime ?? 0) - (a.totalPlayTime ?? 0)); break;
      case 'added': list.sort((a, b) => (b.addedAt ?? 0) - (a.addedAt ?? 0)); break;
      case 'platform': list.sort((a, b) => (a.source ?? 'zzz').localeCompare(b.source ?? 'zzz')); break;
      case 'rating': list.sort((a, b) => (b.criticScore ?? 0) - (a.criticScore ?? 0)); break;
      case 'release': list.sort((a, b) => (b.releaseYear ?? 0) - (a.releaseYear ?? 0)); break;
      case 'play_count': list.sort((a, b) => (b.playCount ?? 0) - (a.playCount ?? 0)); break;
    }
    return list;
  })());

  const groupedGames = $derived((() => {
    if (groupMode === 'none') return null;
    const groups = new Map<string, Game[]>();
    for (const g of filteredGames) {
      let key: string;
      switch (groupMode) {
        case 'platform': key = getPlatformLabel(g.source ?? 'manual'); break;
        case 'genre': key = g.genre?.split(',')[0]?.trim() ?? 'Unknown'; break;
        case 'developer': key = g.developers?.[0] ?? 'Unknown'; break;
        case 'series': key = g.series ?? 'No Series'; break;
        case 'status': key = t(`bs_status_${g.completionStatus ?? 'not_played'}`); break;
        case 'completion': key = g.completionStatus ?? 'not_played'; break;
        default: key = 'Other';
      }
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(g);
    }
    return [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  })());

  const stats = $derived((() => {
    const visible = games.filter(g => !g.hidden);
    return {
      total: visible.length,
      totalHours: Math.floor(visible.reduce((s, g) => s + (g.totalPlayTime ?? 0), 0) / 60),
    };
  })());

  const recentlyPlayed = $derived(
    [...games].filter(g => g.lastPlayedAt && !g.hidden).sort((a, b) => (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0)).slice(0, 5)
  );

  function formatPlayTime(m: number | undefined): string {
    if (!m || m <= 0) return '-';
    const h = Math.floor(m / 60);
    const mins = m % 60;
    return h > 0 ? `${h}h ${mins}m` : `${mins}m`;
  }

  function getPlatformLabel(src: string): string {
    const map: Record<string, string> = { steam: 'Steam', epic: 'Epic', ea: 'EA', ubisoft: 'Ubisoft', xbox: 'Xbox', gog: 'GOG', battlenet: 'Battle.net', manual: 'Manual' };
    return map[src] ?? src;
  }

  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  function debouncedSearch(value: string) {
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => { searchQuery = value; }, 150);
  }

  function handleLaunch(game: Game) {
    onLaunchGame(game);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'f' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      const input = document.querySelector('.search-input') as HTMLInputElement;
      input?.focus();
    }
    if (e.key === 'a' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      selectedIds = new Set(filteredGames.map(g => g.id));
    }
    if (e.key === 'Delete' && selectedIds.size > 0) {
      batchDelete();
    }
    if (e.key === 'Enter' && selectedGame) {
      handleLaunch(selectedGame);
    }
    if (e.key === 'Escape') {
      selectedIds = new Set();
      searchQuery = '';
    }
    if (e.key === 'F11') {
      onSwitchMode();
    }
  }

  import { onMount, onDestroy } from 'svelte';
  onMount(() => {
    window.addEventListener('keydown', handleKeyDown);
  });
  onDestroy(() => {
    window.removeEventListener('keydown', handleKeyDown);
  });
</script>

<div class="desktop-mode">
  <header class="desktop-header">
    <div class="header-left">
      <h1 class="app-title">KoGames</h1>
      <span class="game-count">{stats.total} games · {stats.totalHours}h played</span>
    </div>
    <div class="header-center">
      <input type="text" class="search-input" placeholder={t('bs_search_games')} oninput={(e: Event) => debouncedSearch((e.target as HTMLInputElement).value)} />
    </div>
    <div class="header-right">
      <button class="header-btn" onclick={onAddGame} title={t('bs_add_game')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
      </button>
      <button class="header-btn" class:active={viewMode === 'grid'} onclick={() => viewMode = 'grid'} title="Grid">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>
      </button>
      <button class="header-btn" class:active={viewMode === 'list'} onclick={() => viewMode = 'list'} title="List">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>
      </button>
      <button class="header-btn" onclick={onSwitchMode} title="Fullscreen Mode">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
      </button>
    </div>
  </header>

  <div class="desktop-toolbar">
    <div class="toolbar-filters">
      <select class="toolbar-select" bind:value={sortMode}>
        <option value="name">{t('bs_sort_name')}</option>
        <option value="recent">{t('bs_sort_recent')}</option>
        <option value="most_played">{t('bs_sort_most_played')}</option>
        <option value="added">{t('bs_sort_added')}</option>
        <option value="platform">{t('bs_sort_platform')}</option>
        <option value="rating">Rating</option>
        <option value="release">Release</option>
        <option value="play_count">Play Count</option>
      </select>
      <select class="toolbar-select" bind:value={groupMode}>
        <option value="none">No Group</option>
        <option value="platform">{t('bs_platform')}</option>
        <option value="genre">Genre</option>
        <option value="developer">{t('bs_developers')}</option>
        <option value="series">{t('bs_series')}</option>
        <option value="status">Status</option>
      </select>
      <select class="toolbar-select" bind:value={filterPlatform}>
        <option value="all">{t('bs_filter_all')}</option>
        <option value="steam">Steam</option>
        <option value="epic">Epic</option>
        <option value="ea">EA</option>
        <option value="ubisoft">Ubisoft</option>
        <option value="xbox">Xbox</option>
        <option value="gog">GOG</option>
        <option value="battlenet">Battle.net</option>
        <option value="manual">{t('bs_filter_manual')}</option>
      </select>
      <select class="toolbar-select" bind:value={filterStatus}>
        <option value="all">{t('bs_filter_all')}</option>
        <option value="not_played">{t('bs_status_not_played')}</option>
        <option value="playing">{t('bs_status_playing')}</option>
        <option value="completed">{t('bs_status_completed')}</option>
        <option value="on_hold">{t('bs_status_on_hold')}</option>
        <option value="dropped">{t('bs_status_dropped')}</option>
        <option value="backlog">{t('bs_status_backlog')}</option>
      </select>
    </div>
    <span class="toolbar-count">{filteredGames.length} / {stats.total}</span>
  </div>

  <div class="desktop-body">
    <nav class="sidebar-nav">
      <div class="nav-section">
        <button class="nav-item" class:active={filterPlatform === 'all' && filterStatus === 'all'} onclick={() => { filterPlatform = 'all'; filterStatus = 'all'; }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>
          All Games
        </button>
        <button class="nav-item" onclick={() => { filterStatus = 'playing'; filterPlatform = 'all'; }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>
          {t('bs_status_playing')}
        </button>
        <button class="nav-item" onclick={() => { filterStatus = 'not_played'; filterPlatform = 'all'; }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="16"/><line x1="8" y1="12" x2="16" y2="12"/></svg>
          {t('bs_status_backlog')}
        </button>
        <button class="nav-item" onclick={() => { filterStatus = 'completed'; filterPlatform = 'all'; }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>
          {t('bs_status_completed')}
        </button>
      </div>
      <div class="nav-divider"></div>
      <div class="nav-section">
        <div class="nav-label">Platforms</div>
        {#each ['steam', 'epic', 'ea', 'ubisoft', 'xbox', 'gog', 'battlenet'] as p}
          {#if games.some(g => g.source === p)}
            <button class="nav-item" class:active={filterPlatform === p} onclick={() => { filterPlatform = (filterPlatform === p ? 'all' : p) as FilterPlatform; filterStatus = 'all'; }}>
              {getPlatformLabel(p)}
              <span class="nav-count">{games.filter(g => g.source === p).length}</span>
            </button>
          {/if}
        {/each}
      </div>
    </nav>
    <div class="game-list-container">
      {#if recentlyPlayed.length > 0 && !searchQuery}
        <div class="recent-section">
          <h3 class="section-title">{t('bs_recently_played')}</h3>
          <div class="recent-row">
            {#each recentlyPlayed as game}
              <div class="recent-card" onclick={() => { selectedGame = game; }} ondblclick={() => handleLaunch(game)}>
                <div class="recent-cover">
                  {#if getCoverUrl(game.cover)}
                    <img src={getCoverUrl(game.cover)!} alt={game.name} />
                  {:else}
                    <span>{game.name.charAt(0)}</span>
                  {/if}
                </div>
                <span class="recent-name">{game.name}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}
      {#if viewMode === 'grid'}
        {#if groupedGames}
          {#each groupedGames as [groupName, groupGames]}
            <div class="group-header">{groupName} <span class="group-count">({groupGames.length})</span></div>
            <div class="game-grid">
              {#each groupGames as game}
                <div class="grid-card" class:selected={selectedGame?.id === game.id || selectedIds.has(game.id)} onclick={(e: MouseEvent) => toggleSelect(game, e)} ondblclick={() => handleLaunch(game)}>
                  <div class="grid-cover">
                    {#if getCoverUrl(game.cover)}
                      <img src={getCoverUrl(game.cover)!} alt={game.name} loading="lazy" />
                    {:else}
                      <div class="grid-placeholder">{game.name.charAt(0)}</div>
                    {/if}
                    {#if game.hidden}<span class="hidden-badge">H</span>{/if}
                  </div>
                  <div class="grid-name">{game.name}</div>
                </div>
              {/each}
            </div>
          {/each}
        {:else}
          <div class="game-grid">
            {#each filteredGames as game}
              <div class="grid-card" class:selected={selectedGame?.id === game.id || selectedIds.has(game.id)} onclick={(e: MouseEvent) => toggleSelect(game, e)} ondblclick={() => handleLaunch(game)}>
                <div class="grid-cover">
                  {#if getCoverUrl(game.cover)}
                    <img src={getCoverUrl(game.cover)!} alt={game.name} loading="lazy" />
                  {:else}
                    <div class="grid-placeholder">{game.name.charAt(0)}</div>
                  {/if}
                  {#if game.hidden}<span class="hidden-badge">H</span>{/if}
                </div>
                <div class="grid-name">{game.name}</div>
              </div>
            {/each}
          </div>
        {/if}
      {:else}
        <table class="game-table">
          <thead>
            <tr>
              <th></th>
              <th>{t('bs_sort_name')}</th>
              <th>{t('bs_platform')}</th>
              <th>{t('bs_play_time')}</th>
              <th>{t('bs_change_status')}</th>
              <th>{t('bs_critic_score')}</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredGames as game}
              <tr class:selected={selectedGame?.id === game.id || selectedIds.has(game.id)} onclick={(e: MouseEvent) => toggleSelect(game, e)} ondblclick={() => handleLaunch(game)}>
                <td class="table-cover-cell">
                  {#if getCoverUrl(game.cover)}
                    <img src={getCoverUrl(game.cover)!} alt="" class="table-cover" />
                  {:else}
                    <div class="table-cover-placeholder">{game.name.charAt(0)}</div>
                  {/if}
                </td>
                <td class="table-name">{game.name}</td>
                <td>{getPlatformLabel(game.source ?? 'manual')}</td>
                <td>{formatPlayTime(game.totalPlayTime)}</td>
                <td>{t(`bs_status_${game.completionStatus ?? 'not_played'}`)}</td>
                <td>{game.criticScore ? game.criticScore.toFixed(0) : '-'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    {#if showBatchBar}
      <div class="batch-bar">
        <span class="batch-count">{selectedIds.size} selected</span>
        <button class="batch-btn" onclick={() => batchSetStatus('playing')}>Set Playing</button>
        <button class="batch-btn" onclick={() => batchSetStatus('completed')}>Set Completed</button>
        <button class="batch-btn" onclick={() => batchSetStatus('backlog')}>Set Backlog</button>
        <button class="batch-btn" onclick={batchHide}>Hide</button>
        <button class="batch-btn danger" onclick={batchDelete}>Delete</button>
        <button class="batch-btn" onclick={() => { selectedIds = new Set(); }}>Cancel</button>
      </div>
    {/if}

    {#if selectedGame}
      <aside class="detail-sidebar">
        <div class="sidebar-cover">
          {#if getCoverUrl(selectedGame.cover)}
            <img src={getCoverUrl(selectedGame.cover)!} alt={selectedGame.name} />
          {:else}
            <div class="sidebar-placeholder">{selectedGame.name.charAt(0)}</div>
          {/if}
        </div>
        <h2 class="sidebar-title">{selectedGame.name}</h2>
        <button class="sidebar-play-btn" onclick={() => handleLaunch(selectedGame!)}>
          <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
          {selectedGame.lastPlayedAt ? t('bs_continue') : t('bs_play')}
        </button>
        <div class="sidebar-info">
          {#if selectedGame.developers?.length}
            <div class="info-row"><span class="info-label">{t('bs_developers')}</span><span>{selectedGame.developers.join(', ')}</span></div>
          {/if}
          {#if selectedGame.publishers?.length}
            <div class="info-row"><span class="info-label">{t('bs_publishers')}</span><span>{selectedGame.publishers.join(', ')}</span></div>
          {/if}
          <div class="info-row"><span class="info-label">{t('bs_platform')}</span><span>{getPlatformLabel(selectedGame.source ?? 'manual')}</span></div>
          <div class="info-row"><span class="info-label">{t('bs_play_time')}</span><span>{formatPlayTime(selectedGame.totalPlayTime)}</span></div>
          {#if selectedGame.criticScore}
            <div class="info-row"><span class="info-label">{t('bs_critic_score')}</span><span style="color: {selectedGame.criticScore >= 75 ? '#4ade80' : selectedGame.criticScore >= 50 ? '#facc15' : '#f87171'}">{selectedGame.criticScore.toFixed(0)}</span></div>
          {/if}
          {#if selectedGame.releaseDate}
            <div class="info-row"><span class="info-label">{t('bs_release_date')}</span><span>{selectedGame.releaseDate}</span></div>
          {/if}
          {#if selectedGame.genre}
            <div class="info-row"><span class="info-label">Genre</span><span>{selectedGame.genre}</span></div>
          {/if}
        </div>
        {#if selectedGame.description}
          <p class="sidebar-desc">{selectedGame.description}</p>
        {/if}
        {#if selectedGame.tags?.length}
          <div class="sidebar-tags">
            {#each selectedGame.tags as tag}
              <span class="sidebar-tag">{tag}</span>
            {/each}
          </div>
        {/if}
      </aside>
    {/if}
  </div>
</div>

<style>
  .desktop-mode { display: flex; flex-direction: column; height: 100vh; background: #1a1a2e; color: #e0e0e0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }
  .desktop-header { display: flex; align-items: center; justify-content: space-between; padding: 10px 20px; background: #16213e; border-bottom: 1px solid rgba(255,255,255,0.08); gap: 16px; -webkit-app-region: drag; }
  .header-left { display: flex; align-items: baseline; gap: 12px; }
  .app-title { font-size: 1.2rem; font-weight: 700; color: #818cf8; margin: 0; }
  .game-count { font-size: 0.75rem; color: rgba(255,255,255,0.4); }
  .header-center { flex: 1; max-width: 400px; }
  .search-input { width: 100%; background: rgba(255,255,255,0.08); border: 1px solid rgba(255,255,255,0.12); border-radius: 6px; padding: 6px 12px; color: white; font-size: 0.85rem; outline: none; -webkit-app-region: no-drag; }
  .search-input:focus { border-color: #818cf8; }
  .header-right { display: flex; gap: 4px; -webkit-app-region: no-drag; }
  .header-btn { background: rgba(255,255,255,0.06); border: 1px solid transparent; border-radius: 6px; padding: 6px; cursor: pointer; color: rgba(255,255,255,0.6); transition: all 0.15s; }
  .header-btn:hover { background: rgba(255,255,255,0.12); color: white; }
  .header-btn.active { background: rgba(129,140,248,0.2); border-color: #818cf8; color: #818cf8; }
  .header-btn svg { width: 18px; height: 18px; }

  .desktop-toolbar { display: flex; align-items: center; justify-content: space-between; padding: 8px 20px; background: rgba(255,255,255,0.02); border-bottom: 1px solid rgba(255,255,255,0.05); }
  .toolbar-filters { display: flex; gap: 8px; }
  .toolbar-select { background: rgba(255,255,255,0.08); border: 1px solid rgba(255,255,255,0.1); border-radius: 4px; padding: 4px 8px; color: white; font-size: 0.8rem; outline: none; cursor: pointer; }
  .toolbar-count { font-size: 0.75rem; color: rgba(255,255,255,0.4); }

  .desktop-body { display: flex; flex: 1; overflow: hidden; }
  .game-list-container { flex: 1; overflow-y: auto; padding: 16px; }

  .game-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 12px; contain: layout; }
  .grid-card { cursor: pointer; border-radius: 8px; overflow: hidden; transition: transform 0.15s, box-shadow 0.15s; border: 2px solid transparent; contain: layout style paint; content-visibility: auto; contain-intrinsic-size: 140px 250px; will-change: transform; }
  .grid-card:hover { transform: translateY(-2px); box-shadow: 0 4px 12px rgba(0,0,0,0.3); }
  .grid-card.selected { border-color: #818cf8; }
  .grid-cover { aspect-ratio: 2/3; background: rgba(255,255,255,0.05); position: relative; }
  .grid-cover img { width: 100%; height: 100%; object-fit: cover; }
  .grid-placeholder { width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; font-size: 2rem; color: rgba(255,255,255,0.2); }
  .hidden-badge { position: absolute; top: 4px; right: 4px; background: rgba(0,0,0,0.7); color: rgba(255,255,255,0.5); font-size: 0.6rem; padding: 1px 4px; border-radius: 3px; }
  .grid-name { padding: 6px 8px; font-size: 0.78rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; background: rgba(255,255,255,0.03); }

  .game-table { width: 100%; border-collapse: collapse; font-size: 0.82rem; }
  .game-table th { text-align: left; padding: 8px 12px; border-bottom: 1px solid rgba(255,255,255,0.1); color: rgba(255,255,255,0.5); font-weight: 500; font-size: 0.75rem; text-transform: uppercase; }
  .game-table td { padding: 6px 12px; border-bottom: 1px solid rgba(255,255,255,0.04); }
  .game-table tr { cursor: pointer; transition: background 0.1s; content-visibility: auto; contain-intrinsic-size: auto 44px; }
  .game-table tr:hover { background: rgba(255,255,255,0.04); }
  .game-table tr.selected { background: rgba(129,140,248,0.1); }
  .table-cover-cell { width: 32px; }
  .table-cover { width: 28px; height: 38px; object-fit: cover; border-radius: 3px; }
  .table-cover-placeholder { width: 28px; height: 38px; background: rgba(255,255,255,0.08); border-radius: 3px; display: flex; align-items: center; justify-content: center; font-size: 0.7rem; color: rgba(255,255,255,0.3); }
  .table-name { font-weight: 500; }

  .detail-sidebar { width: 300px; background: rgba(255,255,255,0.03); border-left: 1px solid rgba(255,255,255,0.06); overflow-y: auto; padding: 16px; flex-shrink: 0; }
  .sidebar-cover { width: 100%; aspect-ratio: 2/3; border-radius: 8px; overflow: hidden; margin-bottom: 12px; background: rgba(255,255,255,0.05); }
  .sidebar-cover img { width: 100%; height: 100%; object-fit: cover; }
  .sidebar-placeholder { width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; font-size: 3rem; color: rgba(255,255,255,0.15); }
  .sidebar-title { font-size: 1.1rem; font-weight: 600; margin: 0 0 10px; line-height: 1.3; }
  .sidebar-play-btn { width: 100%; display: flex; align-items: center; justify-content: center; gap: 6px; padding: 10px; background: #818cf8; border: none; border-radius: 8px; color: white; font-size: 0.9rem; font-weight: 600; cursor: pointer; margin-bottom: 14px; transition: background 0.15s; }
  .sidebar-play-btn:hover { background: #6366f1; }
  .sidebar-play-btn svg { width: 16px; height: 16px; }
  .sidebar-info { display: flex; flex-direction: column; gap: 6px; margin-bottom: 12px; }
  .info-row { display: flex; justify-content: space-between; font-size: 0.78rem; }
  .info-label { color: rgba(255,255,255,0.45); }
  .sidebar-desc { font-size: 0.78rem; color: rgba(255,255,255,0.55); line-height: 1.5; margin: 0 0 10px; max-height: 120px; overflow-y: auto; }
  .sidebar-tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .sidebar-tag { background: rgba(129,140,248,0.2); padding: 2px 8px; border-radius: 10px; font-size: 0.7rem; color: rgba(255,255,255,0.7); }

  .batch-bar { position: fixed; bottom: 0; left: 0; right: 0; display: flex; align-items: center; gap: 8px; padding: 10px 20px; background: #1e1b4b; border-top: 2px solid #818cf8; z-index: 100; }
  .batch-count { font-size: 0.85rem; font-weight: 600; color: #818cf8; margin-right: 8px; }
  .batch-btn { background: rgba(255,255,255,0.1); border: 1px solid rgba(255,255,255,0.15); border-radius: 4px; padding: 5px 12px; color: white; font-size: 0.78rem; cursor: pointer; transition: background 0.1s; }
  .batch-btn:hover { background: rgba(255,255,255,0.2); }
  .batch-btn.danger { color: #f87171; border-color: rgba(248,113,113,0.3); }
  .batch-btn.danger:hover { background: rgba(248,113,113,0.2); }

  .recent-section { margin-bottom: 20px; }
  .section-title { font-size: 0.8rem; text-transform: uppercase; color: rgba(255,255,255,0.4); margin: 0 0 8px; font-weight: 600; letter-spacing: 0.5px; }
  .recent-row { display: flex; gap: 10px; overflow-x: auto; padding-bottom: 4px; }
  .recent-card { cursor: pointer; flex-shrink: 0; width: 100px; text-align: center; transition: transform 0.15s; }
  .recent-card:hover { transform: translateY(-2px); }
  .recent-cover { width: 100px; height: 56px; border-radius: 6px; overflow: hidden; background: rgba(255,255,255,0.06); display: flex; align-items: center; justify-content: center; }
  .recent-cover img { width: 100%; height: 100%; object-fit: cover; }
  .recent-cover span { font-size: 1.2rem; color: rgba(255,255,255,0.2); }
  .recent-name { font-size: 0.7rem; display: block; margin-top: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: rgba(255,255,255,0.7); }

  .group-header { font-size: 0.85rem; font-weight: 600; padding: 12px 0 6px; border-bottom: 1px solid rgba(255,255,255,0.08); margin-bottom: 10px; color: #818cf8; }
  .group-count { font-weight: 400; color: rgba(255,255,255,0.35); font-size: 0.75rem; }

  .sidebar-nav { width: 180px; flex-shrink: 0; background: rgba(255,255,255,0.02); border-right: 1px solid rgba(255,255,255,0.06); padding: 12px 0; overflow-y: auto; }
  .nav-section { padding: 0 8px; }
  .nav-label { font-size: 0.65rem; text-transform: uppercase; color: rgba(255,255,255,0.3); padding: 8px 10px 4px; letter-spacing: 0.5px; font-weight: 600; }
  .nav-item { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 10px; border: none; background: transparent; color: rgba(255,255,255,0.6); font-size: 0.78rem; border-radius: 4px; cursor: pointer; transition: all 0.1s; text-align: left; }
  .nav-item:hover { background: rgba(255,255,255,0.06); color: white; }
  .nav-item.active { background: rgba(129,140,248,0.15); color: #818cf8; }
  .nav-item svg { width: 14px; height: 14px; flex-shrink: 0; }
  .nav-count { margin-left: auto; font-size: 0.7rem; color: rgba(255,255,255,0.3); }
  .nav-divider { height: 1px; background: rgba(255,255,255,0.06); margin: 8px 12px; }
</style>
