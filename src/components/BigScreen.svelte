<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Hls from 'hls.js';
  import type { Game, CompletionStatus, SortMode, FilterPlatform, FilterStatus } from '../lib/types';
  import { setLocale, getLocale, t as _t } from '../lib/i18n/index';

  function getCoverUrl(coverPath: string | undefined): string | null {
    if (!coverPath) return null;
    if (coverPath.match(/^[A-Z]:/i)) {
      return convertFileSrc(coverPath);
    }
    return coverPath;
  }

  let {
    games = [],
    onExit = () => {},
    onLaunchGame = (_game: Game) => {},
    onAddGame = () => {},
    onTogglePin = (_game: Game) => {},
    onGamesChanged = () => {},
    onSwitchMode = () => {},
    t: _tProp = (key: string) => key,
  }: {
    games: Game[];
    onExit: () => void;
    onLaunchGame: (game: Game) => void;
    onAddGame: () => void;
    onTogglePin: (game: Game) => void;
    onGamesChanged: () => void;
    onSwitchMode: () => void;
    t: (key: string) => string;
  } = $props();

  // 视图状态
  let currentView = $state<'home' | 'library' | 'settings' | 'search' | 'detail'>('home');
  let focusedIndex = $state(0);
  let libraryFocusIndex = $state(0);
  let libraryGridRef: HTMLDivElement | undefined = $state(undefined);
  let currentTime = $state('');
  let isLoaded = $state(false);
  let controlCenterOpen = $state(false);
  let launchingGame: Game | null = $state(null);
  let searchQuery = $state('');
  let isImporting = $state(false);
  let soundEnabled = $state(true);
  let screenSaverEnabled = $state(true);
  let showSponsor = $state(false);
  let homePreviewMode = $state<'hero' | 'video'>('hero');
  let currentLang = $state(getLocale());
  let settingsTab = $state<'stats' | 'preferences' | 'import' | 'data' | 'about' | 'system'>('stats');

  // 右键菜单
  let ctxMenuOpen = $state(false);
  let ctxMenuX = $state(0);
  let ctxMenuY = $state(0);
  let ctxMenuGame: Game | null = $state(null);
  let ctxMenuFocusIdx = $state(0);

  function openContextMenu(e: MouseEvent, game: Game) {
    e.preventDefault();
    e.stopPropagation();
    ctxMenuGame = game;
    ctxMenuFocusIdx = 0;
    // 计算位置，避免菜单超出屏幕
    const menuW = 240, menuH = 340;
    ctxMenuX = Math.min(e.clientX, window.innerWidth - menuW - 8);
    ctxMenuY = Math.min(e.clientY, window.innerHeight - menuH - 8);
    ctxMenuOpen = true;
  }

  function closeContextMenu() {
    ctxMenuOpen = false;
    ctxMenuGame = null;
  }

  async function ctxAction(action: string) {
    const game = ctxMenuGame;
    if (!game) return;
    closeContextMenu();
    switch (action) {
      case 'launch': handleLaunch(game); break;
      case 'detail': openDetail(game); break;
      case 'pin': onTogglePin(game); showBsToast(game.pinned ? t('bs_unpinned_from_home') : t('bs_pinned_to_home')); break;
      case 'favorite': handleToggleFavorite(game); break;
      case 'hide': toggleHideGame(game); break;
      case 'cover': handleChangeCover(game); break;
      case 'delete': openDetail(game); setTimeout(() => { showDeleteConfirm = true; }, 100); break;
    }
  }

  // Reactive t() that re-evaluates when currentLang changes
  const t = $derived.by(() => {
    // Reference currentLang so Svelte tracks it
    void currentLang;
    return (key: string) => _t(key);
  });

  function toggleLanguage() {
    const newLang = currentLang === 'zh' ? 'en' : 'zh';
    setLocale(newLang);
    currentLang = newLang;
    saveSettings();
  }

  function saveSettings() {
    invoke('save_user_settings', { settings: {
      soundEnabled,
      screenSaverEnabled,
      showHidden,
      homePreviewMode,
      lang: currentLang,
    }}).catch(() => {});
  }

  // 游戏详情
  let detailGame: Game | null = $state(null);
  let showDeleteConfirm = $state(false);
  let showStatusPicker = $state(false);
  let showProperties = $state(false);
  let savePaths: Array<{ path: string; label: string; type: string }> = $state([]);
  let detailFocusIndex = $state(0); // 0=play, 1=pin, 2=favorite, 3=status, 4=cover, 5=properties, 6=delete

  // 手柄控制器
  let gamepadConnected = $state(false);
  let gamepadPollInterval: number | undefined;
  let prevGamepadButtons: boolean[] = [];
  let prevGamepadAxes: number[] = [];
  let gpRepeatTimers: Record<string, number> = {};
  const GP_DEADZONE = 0.4;
  const GP_REPEAT_DELAY = 400;
  const GP_REPEAT_RATE = 120;

  // 待机屏保
  let idleTimeout: ReturnType<typeof setTimeout> | null = null;
  let screenSaverActive = $state(false);
  let screenSaverTime = $state('');
  let screenSaverDate = $state('');
  let screenSaverInterval: ReturnType<typeof setInterval> | null = null;
  const IDLE_TIMEOUT_MS = 120_000; // 2分钟无操作

  // 导航音效 (Web Audio API)
  let audioCtx: AudioContext | null = null;

  function initAudio() {
    if (!audioCtx) audioCtx = new AudioContext();
    if (audioCtx.state === 'suspended') audioCtx.resume();
  }

  function playSound(type: 'move' | 'confirm' | 'back' | 'error') {
    if (!soundEnabled) return;
    try {
      initAudio();
      if (!audioCtx) return;
      const osc = audioCtx.createOscillator();
      const gain = audioCtx.createGain();
      osc.connect(gain);
      gain.connect(audioCtx.destination);
      const now = audioCtx.currentTime;

      switch (type) {
        case 'move':
          osc.type = 'sine';
          osc.frequency.setValueAtTime(800, now);
          osc.frequency.exponentialRampToValueAtTime(600, now + 0.06);
          gain.gain.setValueAtTime(0.04, now);
          gain.gain.exponentialRampToValueAtTime(0.001, now + 0.08);
          osc.start(now);
          osc.stop(now + 0.08);
          break;
        case 'confirm':
          osc.type = 'sine';
          osc.frequency.setValueAtTime(500, now);
          osc.frequency.exponentialRampToValueAtTime(900, now + 0.1);
          gain.gain.setValueAtTime(0.06, now);
          gain.gain.exponentialRampToValueAtTime(0.001, now + 0.15);
          osc.start(now);
          osc.stop(now + 0.15);
          break;
        case 'back':
          osc.type = 'sine';
          osc.frequency.setValueAtTime(600, now);
          osc.frequency.exponentialRampToValueAtTime(350, now + 0.12);
          gain.gain.setValueAtTime(0.05, now);
          gain.gain.exponentialRampToValueAtTime(0.001, now + 0.15);
          osc.start(now);
          osc.stop(now + 0.15);
          break;
        case 'error':
          osc.type = 'square';
          osc.frequency.setValueAtTime(200, now);
          gain.gain.setValueAtTime(0.03, now);
          gain.gain.exponentialRampToValueAtTime(0.001, now + 0.2);
          osc.start(now);
          osc.stop(now + 0.2);
          break;
      }
    } catch {}
  }

  function handleClickSound(e: MouseEvent) {
    const el = e.target as HTMLElement;
    if (el?.closest('button, [role="button"], .cc-card, .nav-tab, .nav-pill-btn, .filter-pill, .settings-item, .settings-nav-item, .library-item, .bigscreen-game-card')) {
      playSound('confirm');
    }
  }

  function handleGlobalContextMenu(e: Event) {
    // 如果已经有自定义菜单的contextmenu事件处理，不在这里阻止
    // 仅阻止默认浏览器右键菜单
    if (!(e as MouseEvent).defaultPrevented) {
      e.preventDefault();
    }
  }

  function handleCtxMenuClose(e: MouseEvent) {
    if (ctxMenuOpen) {
      const target = e.target as HTMLElement;
      if (!target?.closest('.ctx-menu')) {
        closeContextMenu();
      }
    }
  }

  function resetIdleTimer() {
    if (screenSaverActive) {
      screenSaverActive = false;
      if (screenSaverInterval) { clearInterval(screenSaverInterval); screenSaverInterval = null; }
    }
    if (idleTimeout) clearTimeout(idleTimeout);
    if (!screenSaverEnabled) return;
    idleTimeout = setTimeout(() => {
      screenSaverActive = true;
      updateScreenSaverClock();
      screenSaverInterval = setInterval(updateScreenSaverClock, 1000);
    }, IDLE_TIMEOUT_MS);
  }

  function updateScreenSaverClock() {
    const now = new Date();
    screenSaverTime = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    const weekday = now.toLocaleDateString([], { weekday: 'long' });
    const date = now.toLocaleDateString([], { month: 'long', day: 'numeric' });
    screenSaverDate = `${weekday}  ${date}`;
  }

  // 排序/过滤
  let sortMode = $state<SortMode>('recent');
  let filterPlatform = $state<FilterPlatform>('all');
  let filterStatus = $state<FilterStatus>('all');

  // 统计
  const stats = $derived((() => {
    const total = games.length;
    const totalMinutes = games.reduce((sum, g) => sum + (g.totalPlayTime ?? 0), 0);
    const totalHours = Math.floor(totalMinutes / 60);
    const playing = games.filter(g => g.completionStatus === 'playing').length;
    const completed = games.filter(g => g.completionStatus === 'completed').length;
    const backlog = games.filter(g => g.completionStatus === 'backlog' || !g.completionStatus || g.completionStatus === 'not_played').length;
    const mostPlayed = [...games].sort((a, b) => (b.totalPlayTime ?? 0) - (a.totalPlayTime ?? 0)).slice(0, 5);
    const platformCounts: Record<string, number> = {};
    for (const g of games) {
      const src = g.source ?? 'manual';
      platformCounts[src] = (platformCounts[src] ?? 0) + 1;
    }
    return { total, totalHours, totalMinutes, playing, completed, backlog, mostPlayed, platformCounts };
  })());

  // 首页展示游戏（pin优先 + 最近游玩，最多15）
  const displayGames = $derived(
    (() => {
      const pinned = games.filter(g => g.pinned);
      const unpinned = games.filter(g => !g.pinned);
      pinned.sort((a, b) => a.name.localeCompare(b.name));
      unpinned.sort((a, b) => {
        const timeA = a.lastPlayedAt ? new Date(a.lastPlayedAt).getTime() : 0;
        const timeB = b.lastPlayedAt ? new Date(b.lastPlayedAt).getTime() : 0;
        return timeB - timeA;
      });
      return [...pinned, ...unpinned].slice(0, 15);
    })()
  );

  let showHidden = $state(false);
  let filterTag = $state('');

  // 所有已有标签（用于筛选 UI）
  const allTags = $derived((() => {
    const tagSet = new Set<string>();
    for (const g of games) {
      for (const tag of g.tags ?? []) {
        tagSet.add(tag);
      }
    }
    return [...tagSet].sort();
  })());

  // 资源库：排序+过滤
  const filteredGames = $derived((() => {
    let list = [...games];

    if (!showHidden) {
      list = list.filter(g => !g.hidden);
    }

    if (filterTag) {
      list = list.filter(g => g.tags?.includes(filterTag));
    }

    // 平台过滤
    if (filterPlatform !== 'all') {
      list = list.filter(g => (g.source ?? 'manual') === filterPlatform);
    }

    // 状态过滤
    if (filterStatus !== 'all') {
      list = list.filter(g => (g.completionStatus ?? 'not_played') === filterStatus);
    }

    // 排序
    switch (sortMode) {
      case 'name':
        list.sort((a, b) => a.name.localeCompare(b.name));
        break;
      case 'recent':
        list.sort((a, b) => (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0));
        break;
      case 'most_played':
        list.sort((a, b) => (b.totalPlayTime ?? 0) - (a.totalPlayTime ?? 0));
        break;
      case 'added':
        list.sort((a, b) => (b.addedAt ?? 0) - (a.addedAt ?? 0));
        break;
      case 'platform':
        list.sort((a, b) => (a.source ?? 'zzz').localeCompare(b.source ?? 'zzz'));
        break;
    }
    return list;
  })());

  // 搜索（支持名称、开发商、发行商、标签）
  const searchResults = $derived(
    searchQuery.trim()
      ? games.filter(g => {
          const q = searchQuery.toLowerCase();
          return g.name.toLowerCase().includes(q)
            || (g.developers?.some(d => d.toLowerCase().includes(q)) ?? false)
            || (g.publishers?.some(p => p.toLowerCase().includes(q)) ?? false)
            || (g.tags?.some(t => t.toLowerCase().includes(q)) ?? false)
            || (g.genre?.toLowerCase().includes(q) ?? false)
            || (g.series?.toLowerCase().includes(q) ?? false);
        })
      : []
  );

  // 系统状态
  let batteryPercent = $state(100);
  let isCharging = $state(false);
  let isAC = $state(false);

  // 视频/大图缓存
  let videoCache: Record<string, { videoUrl?: string; hlsUrl?: string; heroUrl?: string }> = $state({});
  let detailVideoEl: HTMLVideoElement | undefined = $state(undefined);
  let homeVideoEl: HTMLVideoElement | undefined = $state(undefined);
  let hlsInstance: Hls | null = null;
  let homeHlsInstance: Hls | null = null;

  let ribbonRef: HTMLDivElement | undefined = $state(undefined);

  // Y键长按置顶
  let yHoldStart: number | null = null;
  let yHoldInterval: number | null = null;
  let yHoldProgress = $state(0);
  const Y_HOLD_DURATION = 1000;

  // Toast
  let toastMessage = $state('');
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  function showBsToast(msg: string) {
    toastMessage = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { toastMessage = ''; toastTimer = null; }, 2000);
  }

  let timeInterval: number | undefined;
  let pendingTimeouts = new Set<ReturnType<typeof setTimeout>>();
  let batteryInterval: number | undefined;

  // 时间格式化
  function formatPlayTime(minutes: number | undefined): string {
    if (!minutes || minutes <= 0) return t('bs_never_played');
    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    if (hours > 0) return t('bs_hours_short').replace('{h}', String(hours)) + ' ' + t('bs_minutes_short').replace('{m}', String(mins));
    return t('bs_minutes_short').replace('{m}', String(mins));
  }

  function formatLastPlayed(ts: number | undefined): string {
    if (!ts) return t('bs_never_played');
    const diff = Date.now() - ts;
    if (diff < 60000) return t('bs_just_now');
    const days = Math.floor(diff / 86400000);
    if (days === 0) return t('bs_just_now');
    return t('bs_days_ago').replace('{days}', String(days));
  }

  function getStatusLabel(status: CompletionStatus | undefined): string {
    const key = `bs_status_${status ?? 'not_played'}`;
    return t(key);
  }

  function getStatusColor(status: CompletionStatus | undefined): string {
    switch (status) {
      case 'playing': return '#22c55e';
      case 'completed': return '#6366f1';
      case 'on_hold': return '#f59e0b';
      case 'dropped': return '#ef4444';
      case 'backlog': return '#06b6d4';
      default: return 'rgba(255,255,255,0.3)';
    }
  }

  function getPlatformLabel(source: string | undefined): string {
    switch (source) {
      case 'steam': return 'Steam';
      case 'epic': return 'Epic Games';
      case 'ea': return 'EA';
      case 'ubisoft': return 'Ubisoft';
      case 'xbox': return 'Xbox';
      case 'gog': return 'GOG';
      default: return t('bs_filter_manual');
    }
  }

  const allStatuses: CompletionStatus[] = ['not_played', 'playing', 'completed', 'on_hold', 'dropped', 'backlog'];
  const allPlatforms: FilterPlatform[] = ['all', 'steam', 'epic', 'ea', 'ubisoft', 'xbox', 'gog', 'manual'];
  const allSorts: { mode: SortMode; key: string }[] = [
    { mode: 'name', key: 'bs_sort_name' },
    { mode: 'recent', key: 'bs_sort_recent' },
    { mode: 'most_played', key: 'bs_sort_most_played' },
    { mode: 'added', key: 'bs_sort_added' },
  ];

  // 打开游戏详情
  function openDetail(game: Game) {
    detailGame = game;
    detailFocusIndex = 0;
    showDeleteConfirm = false;
    showStatusPicker = false;
    showProperties = false;
    savePaths = [];
    currentView = 'detail';
    fetchMetadataIfNeeded(game);
    if (!game.installSize && game.installLocation) {
      invoke<number | null>('detect_install_size', { gameId: game.id }).then(size => {
        if (size && detailGame?.id === game.id) {
          detailGame = { ...detailGame, installSize: size };
          const idx = games.findIndex(g => g.id === game.id);
          if (idx >= 0) games[idx] = { ...games[idx], installSize: size };
        }
      }).catch(() => {});
    }
  }

  // 更新游戏属性并通知
  async function updateGame(game: Game, changes: Partial<Game>) {
    Object.assign(game, changes);
    const idx = games.findIndex(g => g.id === game.id);
    if (idx >= 0) {
      games[idx] = { ...games[idx], ...changes };
    }
    try {
      await invoke('update_game', { gameId: game.id, updates: changes });
    } catch (e) {
      console.error('Save failed:', e);
    }
  }

  async function handleChangeCover(game: Game) {
    try {
      const coverPath = await invoke<string | null>('select_cover_image', { gameId: game.id });
      if (coverPath) {
        Object.assign(game, { cover: coverPath });
        const idx = games.findIndex(g => g.id === game.id);
        if (idx >= 0) games[idx] = { ...games[idx], cover: coverPath };
        showBsToast('✓');
      }
    } catch (e) {
      console.error('Cover change failed:', e);
    }
  }

  // 初始化/清理 HLS 视频播放器
  function setupHlsVideo(videoEl: HTMLVideoElement, steamAppId: string, target: 'detail' | 'home') {
    if (target === 'detail') cleanupHls('detail');
    else cleanupHls('home');

    const cache = videoCache[steamAppId];
    if (!cache) return;

    const hlsUrl = cache.hlsUrl;
    const mp4Url = cache.videoUrl;

    if (hlsUrl && Hls.isSupported()) {
      console.log(`[VIDEO] Using HLS (${target}) for`, steamAppId);
      const hls = new Hls({ startLevel: -1, maxBufferLength: 30 });
      hls.loadSource(hlsUrl);
      hls.attachMedia(videoEl);
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        videoEl.play().catch(() => {});
      });
      hls.on(Hls.Events.ERROR, (_event: string, data: { fatal: boolean }) => {
        if (data.fatal && mp4Url) {
          console.log(`[VIDEO] HLS failed (${target}), falling back to MP4`);
          hls.destroy();
          if (target === 'detail') hlsInstance = null;
          else homeHlsInstance = null;
          videoEl.src = mp4Url;
          videoEl.play().catch(() => {});
        }
      });
      if (target === 'detail') hlsInstance = hls;
      else homeHlsInstance = hls;
    } else if (mp4Url) {
      console.log(`[VIDEO] Using MP4 (${target}) for`, steamAppId);
      videoEl.src = mp4Url;
      videoEl.play().catch(() => {});
    }
  }

  // 保留兼容接口
  function setupDetailVideo(videoEl: HTMLVideoElement, steamAppId: string) {
    setupHlsVideo(videoEl, steamAppId, 'detail');
  }

  function cleanupHls(target?: 'detail' | 'home') {
    if (!target || target === 'detail') {
      if (hlsInstance) { hlsInstance.destroy(); hlsInstance = null; }
    }
    if (!target || target === 'home') {
      if (homeHlsInstance) { homeHlsInstance.destroy(); homeHlsInstance = null; }
    }
  }

  // 当 video 元素出现或 videoCache 更新时初始化播放器
  $effect(() => {
    const el = detailVideoEl;
    const appId = detailGame?.steamAppId;
    if (el && appId && videoCache[appId]) {
      setupDetailVideo(el, appId);
    }
    return () => cleanupHls('detail');
  });

  // 主页视频背景
  $effect(() => {
    const el = homeVideoEl;
    const game = displayGames[focusedIndex];
    const appId = game?.steamAppId;
    if (el && appId && homePreviewMode === 'video' && videoCache[appId]) {
      setupHlsVideo(el, appId, 'home');
    }
    return () => cleanupHls('home');
  });

  // 通过 Rust 后端获取 Steam 视频 URL（绕过 CORS）
  async function fetchSteamVideoDirect(steamAppId: string) {
    try {
      const result = await invoke<{ videos: Array<{ hlsUrl?: string; mp4Max?: string; thumbnail?: string; name?: string }> }>('fetch_steam_videos', { steamAppId });
      if (result.videos?.length) {
        const v = result.videos[0];
        console.log('[VIDEO] Got video from fetch_steam_videos:', v.hlsUrl, v.mp4Max);
        videoCache[steamAppId] = {
          hlsUrl: v.hlsUrl || undefined,
          videoUrl: v.mp4Max || undefined,
          heroUrl: v.thumbnail || undefined,
        };
      }
    } catch (e) {
      console.log('[VIDEO] fetch_steam_videos failed for', steamAppId, e);
    }
  }

  async function fetchMetadataIfNeeded(game: Game) {
    const hasMetadata = !!(game.description && game.developers?.length);
    const hasVideo = !!(game.steamAppId && (videoCache[game.steamAppId]?.hlsUrl || videoCache[game.steamAppId]?.videoUrl));

    // 如果元数据和视频都已有则跳过
    if (hasMetadata && hasVideo) return;

    if (game.steamAppId) {
      try {
        const result = await invoke<Record<string, unknown>>(
          'fetch_steam_metadata', { gameId: game.id, steamAppId: game.steamAppId }
        );
        // 提取视频信息到 videoCache (从返回结果或独立获取)
        if (result.movies && Array.isArray(result.movies) && (result.movies as unknown[]).length > 0) {
          const m = (result.movies as Record<string, string>[])[0];
          console.log('[VIDEO] Got movie from backend:', m.hlsUrl, m.mp4Max);
          videoCache[game.steamAppId] = {
            hlsUrl: m.hlsUrl || undefined,
            videoUrl: m.mp4Max || undefined,
            heroUrl: m.thumbnail || undefined,
          };
        }
        // 如果后端还没返回 movies，通过专用命令获取
        if (!videoCache[game.steamAppId]?.hlsUrl && !videoCache[game.steamAppId]?.videoUrl) {
          await fetchSteamVideoDirect(game.steamAppId);
        }
        if (result.updated) {
          const idx = games.findIndex(g => g.id === game.id);
          if (idx >= 0) {
            const updates: Partial<Game> = {};
            if (result.description) updates.description = result.description as string;
            if (result.genre) updates.genre = result.genre as string;
            if (result.releaseYear) updates.releaseYear = result.releaseYear as number;
            if (result.releaseDate) updates.releaseDate = result.releaseDate as string;
            if (result.developers) updates.developers = result.developers as string[];
            if (result.publishers) updates.publishers = result.publishers as string[];
            if (result.criticScore) updates.criticScore = result.criticScore as number;
            if (result.backgroundImage) updates.backgroundImage = result.backgroundImage as string;
            games[idx] = { ...games[idx], ...updates };
            if (detailGame?.id === game.id) detailGame = games[idx];
          }
        }
      } catch (_) {}
    } else {
      try {
        const result = await invoke<Record<string, unknown>>(
          'fetch_igdb_metadata', { gameId: game.id, gameName: game.name }
        );
        if (result.found && result.updated) {
          const idx = games.findIndex(g => g.id === game.id);
          if (idx >= 0) {
            const updates: Partial<Game> = {};
            if (result.description) updates.description = result.description as string;
            if (result.genre) updates.genre = result.genre as string;
            if (result.releaseYear) updates.releaseYear = result.releaseYear as number;
            if (result.releaseDate) updates.releaseDate = result.releaseDate as string;
            if (result.developers) updates.developers = result.developers as string[];
            if (result.publishers) updates.publishers = result.publishers as string[];
            if (result.criticScore) updates.criticScore = result.criticScore as number;
            if (result.communityScore) updates.communityScore = result.communityScore as number;
            games[idx] = { ...games[idx], ...updates };
            if (detailGame?.id === game.id) detailGame = games[idx];
          }
        }
      } catch (_) {}
    }

    if (!game.cover) {
      try {
        if (game.steamAppId) {
          const cover = await invoke<string | null>('download_game_cover', { gameId: game.id });
          if (cover) {
            const idx = games.findIndex(g => g.id === game.id);
            if (idx >= 0) { games[idx] = { ...games[idx], cover }; }
            if (detailGame?.id === game.id) detailGame = { ...detailGame, cover };
          }
        } else {
          const cover = await invoke<string | null>('search_cover_online', { gameId: game.id, gameName: game.name });
          if (cover) {
            const idx = games.findIndex(g => g.id === game.id);
            if (idx >= 0) { games[idx] = { ...games[idx], cover }; }
            if (detailGame?.id === game.id) detailGame = { ...detailGame, cover };
          }
        }
      } catch (_) {}
    }
  }

  async function toggleProperties() {
    showProperties = !showProperties;
    if (showProperties && detailGame && savePaths.length === 0) {
      try {
        savePaths = await invoke<Array<{ path: string; label: string; type: string }>>(
          'find_save_paths', { gameName: detailGame.name, steamAppId: detailGame.steamAppId ?? null }
        );
      } catch (_) { savePaths = []; }
    }
  }

  async function openFolder(path: string) {
    try {
      await invoke('open_folder', { folderPath: path });
    } catch (_) {
      showBsToast('Folder not found');
    }
  }

  async function handleDeleteGame(game: Game) {
    try {
      await invoke('delete_game', { gameId: game.id });
      showBsToast(t('bs_delete_game'));
      onGamesChanged();
      currentView = 'library';
    } catch (e) {
      console.error('Delete failed:', e);
    }
  }

  async function handleToggleFavorite(game: Game) {
    const newVal = !game.favorite;
    await updateGame(game, { favorite: newVal });
    showBsToast(newVal ? t('bs_favorite') : t('bs_unfavorite'));
  }

  async function handleSetStatus(game: Game, status: CompletionStatus) {
    await updateGame(game, { completionStatus: status });
    showStatusPicker = false;
    showBsToast(getStatusLabel(status));
  }

  function startYHold() {
    if (currentView !== 'library') return;
    if (libraryFocusIndex >= filteredGames.length) return;
    yHoldStart = Date.now();
    yHoldProgress = 0;
    if (yHoldInterval) clearInterval(yHoldInterval);
    yHoldInterval = window.setInterval(() => {
      if (yHoldStart) {
        const elapsed = Date.now() - yHoldStart;
        yHoldProgress = Math.min(100, (elapsed / Y_HOLD_DURATION) * 100);
        if (elapsed >= Y_HOLD_DURATION) {
          const game = filteredGames[libraryFocusIndex];
          if (game) {
            onTogglePin(game);
            const isPinned = !game.pinned;
            showBsToast(isPinned ? t('bs_pinned_to_home') : t('bs_unpinned_from_home'));
          }
          clearYHold();
        }
      }
    }, 50);
  }

  function clearYHold() {
    if (yHoldInterval) {
      clearInterval(yHoldInterval);
      yHoldInterval = null;
    }
    yHoldStart = null;
    yHoldProgress = 0;
  }

  function handleKeyUp(e: KeyboardEvent) {
    if (e.key === 'y' || e.key === 'Y') clearYHold();
  }

  // ============ 手柄控制器支持 ============
  function gpAction(action: string) {
    // 模拟键盘事件或直接执行对应操作
    switch (action) {
      case 'up': playSound('move'); handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowUp' })); break;
      case 'down': playSound('move'); handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown' })); break;
      case 'left': playSound('move'); handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowLeft' })); break;
      case 'right': playSound('move'); handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowRight' })); break;
      case 'confirm': playSound('confirm'); handleKeyDown(new KeyboardEvent('keydown', { key: 'Enter' })); break;
      case 'back': playSound('back'); handleKeyDown(new KeyboardEvent('keydown', { key: 'Escape' })); break;
      case 'search': currentView = 'search'; searchQuery = ''; break;
      case 'tab': controlCenterOpen = !controlCenterOpen; break;
      case 'launch': {
        // X按钮 = 快速启动当前聚焦游戏
        if (currentView === 'home' && focusedIndex < displayGames.length) {
          handleLaunch(displayGames[focusedIndex]);
        } else if (currentView === 'library' && libraryFocusIndex < filteredGames.length) {
          handleLaunch(filteredGames[libraryFocusIndex]);
        } else if (currentView === 'detail' && detailGame) {
          handleLaunch(detailGame);
        }
        break;
      }
      case 'pin': {
        // Y按钮 = 长按置顶（同键盘逻辑）
        if (!yHoldStart) startYHold();
        break;
      }
      case 'pin_release': clearYHold(); break;
      case 'lb': {
        // LB = 切换到上一个导航视图
        const views: Array<typeof currentView> = ['home', 'library', 'settings'];
        const idx = views.indexOf(currentView);
        if (idx > 0) currentView = views[idx - 1];
        break;
      }
      case 'rb': {
        // RB = 切换到下一个导航视图
        const views: Array<typeof currentView> = ['home', 'library', 'settings'];
        const idx = views.indexOf(currentView);
        if (idx >= 0 && idx < views.length - 1) currentView = views[idx + 1];
        break;
      }
    }
  }

  function gpStartRepeat(action: string) {
    if (gpRepeatTimers[action]) return;
    gpAction(action);
    const delay = setTimeout(() => {
      gpRepeatTimers[action] = setInterval(() => gpAction(action), GP_REPEAT_RATE) as unknown as number;
    }, GP_REPEAT_DELAY) as unknown as number;
    gpRepeatTimers['_delay_' + action] = delay;
  }

  function gpStopRepeat(action: string) {
    if (gpRepeatTimers['_delay_' + action]) {
      clearTimeout(gpRepeatTimers['_delay_' + action]);
      delete gpRepeatTimers['_delay_' + action];
    }
    if (gpRepeatTimers[action]) {
      clearInterval(gpRepeatTimers[action]);
      delete gpRepeatTimers[action];
    }
  }

  function gpStopAllRepeats() {
    for (const key of Object.keys(gpRepeatTimers)) {
      if (key.startsWith('_delay_')) clearTimeout(gpRepeatTimers[key]);
      else clearInterval(gpRepeatTimers[key]);
    }
    gpRepeatTimers = {};
  }

  function pollGamepad() {
    const gamepads = navigator.getGamepads();
    let gp: Gamepad | null = null;
    for (const g of gamepads) {
      if (g && g.connected) { gp = g; break; }
    }
    if (!gp) {
      if (gamepadConnected) { gamepadConnected = false; gpStopAllRepeats(); }
      prevGamepadButtons = [];
      prevGamepadAxes = [];
      return;
    }
    gamepadConnected = true;

    const buttons = gp.buttons.map(b => b.pressed);
    const axes = gp.axes.map(a => a);

    // 任何输入都重置待机计时器
    const hasInput = buttons.some((b, i) => b && !(prevGamepadButtons[i] ?? false))
      || Math.abs(axes[0] ?? 0) > GP_DEADZONE || Math.abs(axes[1] ?? 0) > GP_DEADZONE;
    if (hasInput) {
      resetIdleTimer();
      if (screenSaverActive) { prevGamepadButtons = buttons; prevGamepadAxes = axes; return; }
    }

    // 按钮映射 (Xbox / PS 标准映射)
    // 0=A/Cross, 1=B/Circle, 2=X/Square, 3=Y/Triangle
    // 4=LB/L1, 5=RB/R1, 6=LT/L2, 7=RT/R2
    // 8=Back/Select, 9=Start, 10=LS, 11=RS
    // 12=DpadUp, 13=DpadDown, 14=DpadLeft, 15=DpadRight

    const wasPressed = (i: number) => prevGamepadButtons[i] ?? false;
    const justPressed = (i: number) => buttons[i] && !wasPressed(i);
    const justReleased = (i: number) => !buttons[i] && wasPressed(i);

    // A / Cross = 确认
    if (justPressed(0)) gpAction('confirm');
    // B / Circle = 返回
    if (justPressed(1)) gpAction('back');
    // X / Square = 快速启动
    if (justPressed(2)) gpAction('launch');
    // Y / Triangle = 置顶（按住）
    if (justPressed(3)) gpAction('pin');
    if (justReleased(3)) gpAction('pin_release');

    // LB / L1 = 上一个标签
    if (justPressed(4)) gpAction('lb');
    // RB / R1 = 下一个标签
    if (justPressed(5)) gpAction('rb');

    // Back/Select = 搜索
    if (justPressed(8)) gpAction('search');
    // Start = 控制中心
    if (justPressed(9)) gpAction('tab');

    // D-pad 方向键（支持长按重复）
    if (buttons[12] && !wasPressed(12)) gpStartRepeat('up');
    if (!buttons[12] && wasPressed(12)) gpStopRepeat('up');
    if (buttons[13] && !wasPressed(13)) gpStartRepeat('down');
    if (!buttons[13] && wasPressed(13)) gpStopRepeat('down');
    if (buttons[14] && !wasPressed(14)) gpStartRepeat('left');
    if (!buttons[14] && wasPressed(14)) gpStopRepeat('left');
    if (buttons[15] && !wasPressed(15)) gpStartRepeat('right');
    if (!buttons[15] && wasPressed(15)) gpStopRepeat('right');

    // 左摇杆 = 方向（带死区 + 重复）
    const lx = axes[0] ?? 0;
    const ly = axes[1] ?? 0;
    const prevLx = prevGamepadAxes[0] ?? 0;
    const prevLy = prevGamepadAxes[1] ?? 0;

    // 水平
    if (lx > GP_DEADZONE && prevLx <= GP_DEADZONE) gpStartRepeat('right');
    if (lx <= GP_DEADZONE && prevLx > GP_DEADZONE) gpStopRepeat('right');
    if (lx < -GP_DEADZONE && prevLx >= -GP_DEADZONE) gpStartRepeat('left');
    if (lx >= -GP_DEADZONE && prevLx < -GP_DEADZONE) gpStopRepeat('left');

    // 垂直
    if (ly > GP_DEADZONE && prevLy <= GP_DEADZONE) gpStartRepeat('down');
    if (ly <= GP_DEADZONE && prevLy > GP_DEADZONE) gpStopRepeat('down');
    if (ly < -GP_DEADZONE && prevLy >= -GP_DEADZONE) gpStartRepeat('up');
    if (ly >= -GP_DEADZONE && prevLy < -GP_DEADZONE) gpStopRepeat('up');

    prevGamepadButtons = buttons;
    prevGamepadAxes = axes;
  }

  onMount(() => {
    // 加载持久化设置
    invoke<Record<string, unknown>>('load_user_settings').then((s: Record<string, unknown>) => {
      if (typeof s.soundEnabled === 'boolean') soundEnabled = s.soundEnabled;
      if (typeof s.screenSaverEnabled === 'boolean') screenSaverEnabled = s.screenSaverEnabled;
      if (typeof s.showHidden === 'boolean') showHidden = s.showHidden;
      if (s.homePreviewMode === 'hero' || s.homePreviewMode === 'video') homePreviewMode = s.homePreviewMode;
      if (s.lang === 'zh' || s.lang === 'en') { setLocale(s.lang as string); currentLang = s.lang as string; }
    }).catch(() => {});

    const loadTimeout = setTimeout(() => { pendingTimeouts.delete(loadTimeout); isLoaded = true; }, 100);
    pendingTimeouts.add(loadTimeout);

    const updateTime = () => {
      const now = new Date();
      currentTime = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    };
    updateTime();
    timeInterval = setInterval(updateTime, 1000);

    const fetchBattery = () => {
      invoke<{ has_battery: boolean; is_charging: boolean; is_ac_connected: boolean; battery_percent: number; battery_life_time: number | null; power_status: string }>('get_battery_status').then(status => {
        batteryPercent = status.battery_percent >= 0 ? status.battery_percent : 100;
        isCharging = status.is_charging;
        isAC = !status.has_battery;
      }).catch(() => {});
    };
    fetchBattery();
    batteryInterval = setInterval(fetchBattery, 30000);

    invoke<{ total: number }>('auto_import_all').then(result => {
      if (result.total > 0) {
        onGamesChanged();
        showBsToast(t('bs_imported_count').replace('{count}', String(result.total)));
      }
      // 后台下载缺失的封面
      invoke('download_steam_covers').then(() => onGamesChanged()).catch(() => {});
    }).catch(() => {});

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    window.addEventListener('mousemove', resetIdleTimer);
    window.addEventListener('mousedown', resetIdleTimer);

    // 鼠标点击音效
    window.addEventListener('click', handleClickSound);

    // 禁用默认右键菜单
    window.addEventListener('contextmenu', handleGlobalContextMenu);
    // 点击任意位置关闭右键菜单
    window.addEventListener('mousedown', handleCtxMenuClose);

    // 启动待机计时器
    resetIdleTimer();

    // 手柄轮询 (~60fps)
    gamepadPollInterval = setInterval(pollGamepad, 16);

    // 监听游戏退出事件，刷新游戏列表
    listen('game-exited', () => {
      onGamesChanged();
    });
  });

  onDestroy(() => {
    if (timeInterval) clearInterval(timeInterval);
    if (batteryInterval) clearInterval(batteryInterval);
    if (gamepadPollInterval) clearInterval(gamepadPollInterval);
    if (idleTimeout) clearTimeout(idleTimeout);
    if (screenSaverInterval) clearInterval(screenSaverInterval);
    gpStopAllRepeats();
    for (const t of pendingTimeouts) clearTimeout(t);
    pendingTimeouts.clear();
    if (yHoldInterval) clearInterval(yHoldInterval);
    if (toastTimer) clearTimeout(toastTimer);
    window.removeEventListener('keydown', handleKeyDown);
    window.removeEventListener('keyup', handleKeyUp);
    window.removeEventListener('mousemove', resetIdleTimer);
    window.removeEventListener('mousedown', resetIdleTimer);
    window.removeEventListener('click', handleClickSound);
    window.removeEventListener('contextmenu', handleGlobalContextMenu);
    window.removeEventListener('mousedown', handleCtxMenuClose);
    cleanupHls();
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (!document.hasFocus()) return;
    resetIdleTimer();
    if (screenSaverActive) return; // 屏保时任意键退出，不处理其他操作
    const activeTag = (e.target as HTMLElement)?.tagName;

    // 键盘音效（仅非合成事件触发, 合成事件由gpAction处理）
    if (e.isTrusted) {
      if (e.key.startsWith('Arrow')) playSound('move');
      else if (e.key === 'Escape') playSound('back');
      // Enter/Space 的确认音由全局 click 事件处理，避免重复
    }

    // ESC / B = 返回
    if (e.key === 'Escape' || e.key === 'b' || e.key === 'B') {
      if ((e.key === 'b' || e.key === 'B') && (activeTag === 'INPUT' || activeTag === 'TEXTAREA')) return;
      e.preventDefault();
      if (ctxMenuOpen) { closeContextMenu(); return; }
      if (showStatusPicker) { showStatusPicker = false; return; }
      if (showDeleteConfirm) { showDeleteConfirm = false; return; }
      if (controlCenterOpen) { controlCenterOpen = false; return; }
      if (currentView === 'detail') { currentView = 'library'; return; }
      if (currentView !== 'home') { currentView = 'home'; return; }
      return;
    }

    // 详情页导航
    if (currentView === 'detail') {
      const maxActions = 6; // play, pin, favorite, status, cover, properties, delete
      switch (e.key) {
        case 'ArrowLeft': e.preventDefault(); if (detailFocusIndex > 0) detailFocusIndex--; break;
        case 'ArrowRight': e.preventDefault(); if (detailFocusIndex < maxActions) detailFocusIndex++; break;
        case 'Enter': case ' ': case 'a': case 'A':
          e.preventDefault();
          if (detailGame) {
            if (detailFocusIndex === 0) handleLaunch(detailGame);
            else if (detailFocusIndex === 1) { onTogglePin(detailGame); showBsToast(!detailGame.pinned ? t('bs_pin') : t('bs_unpin')); }
            else if (detailFocusIndex === 2) handleToggleFavorite(detailGame);
            else if (detailFocusIndex === 3) showStatusPicker = !showStatusPicker;
            else if (detailFocusIndex === 4) handleChangeCover(detailGame);
            else if (detailFocusIndex === 5) toggleProperties();
            else if (detailFocusIndex === 6) showDeleteConfirm = true;
          }
          break;
      }
      return;
    }

    // 资源库视图导航
    if (currentView === 'library') {
      const total = filteredGames.length + 1;
      let cols = 5;
      if (libraryGridRef) {
        const items = libraryGridRef.querySelectorAll('.library-item');
        if (items.length >= 2) {
          const first = items[0] as HTMLElement;
          const second = items[1] as HTMLElement;
          if (first.offsetTop === second.offsetTop) {
            let count = 1;
            const firstTop = first.offsetTop;
            for (let i = 1; i < items.length; i++) {
              if ((items[i] as HTMLElement).offsetTop === firstTop) count++;
              else break;
            }
            cols = count;
          }
        }
      }

      switch(e.key) {
        case 'ArrowLeft': e.preventDefault(); if (libraryFocusIndex > 0) libraryFocusIndex--; break;
        case 'ArrowRight': e.preventDefault(); if (libraryFocusIndex < total - 1) libraryFocusIndex++; break;
        case 'ArrowUp':
          e.preventDefault();
          if (libraryFocusIndex >= cols) libraryFocusIndex -= cols;
          break;
        case 'ArrowDown':
          e.preventDefault();
          if (libraryFocusIndex + cols < total) libraryFocusIndex += cols;
          else if (libraryFocusIndex < total - 1) libraryFocusIndex = total - 1;
          break;
        case 'Enter': case ' ': case 'a': case 'A':
          e.preventDefault();
          if (libraryFocusIndex === filteredGames.length) onAddGame();
          else if (filteredGames[libraryFocusIndex]) openDetail(filteredGames[libraryFocusIndex]);
          break;
        case 'x': case 'X':
          e.preventDefault();
          if (filteredGames[libraryFocusIndex]) handleLaunch(filteredGames[libraryFocusIndex]);
          break;
        case 'y': case 'Y':
          if (!e.repeat && !yHoldStart) { e.preventDefault(); startYHold(); }
          break;
      }
      return;
    }

    // 主页导航
    if (currentView !== 'home') return;
    const totalGameCards = displayGames.length + 1;

    switch(e.key) {
      case 'ArrowLeft': e.preventDefault(); if (focusedIndex > 0) focusedIndex--; break;
      case 'ArrowRight': e.preventDefault(); if (focusedIndex < totalGameCards - 1) focusedIndex++; break;
      case 'Enter': case ' ': case 'a': case 'A':
        e.preventDefault();
        if (focusedIndex < displayGames.length) openDetail(displayGames[focusedIndex]);
        else { libraryFocusIndex = 0; currentView = 'library'; }
        break;
      case 'x': case 'X':
        e.preventDefault();
        if (focusedIndex < displayGames.length) handleLaunch(displayGames[focusedIndex]);
        break;
      case 'Tab':
        e.preventDefault();
        controlCenterOpen = !controlCenterOpen;
        break;
    }
  }

  function handleLaunch(game: Game) {
    if (launchingGame) return;
    launchingGame = game;
    const t = setTimeout(() => {
      pendingTimeouts.delete(t);
      launchingGame = null;
      onLaunchGame(game);
    }, 1500);
    pendingTimeouts.add(t);
  }

  async function handleImportSteam() {
    if (isImporting) return;
    isImporting = true;
    try {
      const newGames = await invoke<Game[]>('import_steam_games');
      if (newGames.length > 0) {
        showBsToast(t('bs_imported_count').replace('{count}', String(newGames.length)));
        onGamesChanged();
        // 后台下载封面
        invoke('download_steam_covers').then(() => onGamesChanged()).catch(() => {});
      } else {
        showBsToast(t('bs_no_new_games'));
      }
    } catch (e) {
      console.error('Import failed:', e);
      showBsToast('Import failed');
    } finally {
      isImporting = false;
    }
  }

  async function handleImportAll() {
    if (isImporting) return;
    isImporting = true;
    try {
      const result = await invoke<Record<string, unknown>>('import_all_platform_games');
      const total = (result as { total?: number }).total ?? 0;
      if (total > 0) {
        showBsToast(t('bs_imported_count').replace('{count}', String(total)));
        onGamesChanged();
      } else {
        showBsToast(t('bs_no_new_games'));
      }
    } catch (e) {
      console.error('Import all failed:', e);
      showBsToast('Import failed');
    } finally {
      isImporting = false;
    }
  }


  // 备份库
  async function handleBackup() {
    try {
      const path = await invoke<string>('backup_library');
      showBsToast((t('bs_backup_success') || 'Backup saved') + ': ' + path.split(/[/\\]/).pop());
      playSound('confirm');
    } catch (e) {
      console.error('Backup failed:', e);
      showBsToast(t('bs_backup_failed') || 'Backup failed');
      playSound('error');
    }
  }

  // 批量获取元数据
  let isFetchingMetadata = $state(false);
  async function handleBatchMetadata() {
    if (isFetchingMetadata) return;
    isFetchingMetadata = true;
    try {
      const result = await invoke<{ metadataUpdated: number; coversDownloaded: number }>('batch_fetch_metadata');
      showBsToast(`Metadata: ${result.metadataUpdated}, Covers: ${result.coversDownloaded}`);
      playSound('confirm');
      onGamesChanged();
    } catch (e) {
      console.error('Batch metadata failed:', e);
      playSound('error');
    } finally {
      isFetchingMetadata = false;
    }
  }

  // 导出库
  async function handleExport() {
    try {
      const path = await invoke<string>('export_library');
      showBsToast((t('bs_export_success') || 'Export saved') + ': ' + path.split(/[/\\]/).pop());
      playSound('confirm');
    } catch (e) {
      console.error('Export failed:', e);
      playSound('error');
    }
  }

  // 隐藏/显示游戏
  async function toggleHideGame(game: Game) {
    const newHidden = !game.hidden;
    await updateGame(game, { hidden: newHidden });
    showBsToast(newHidden ? (t('bs_game_hidden') || 'Game hidden') : (t('bs_game_shown') || 'Game shown'));
  }

  // 限制焦点
  $effect(() => {
    const maxIndex = displayGames.length;
    if (focusedIndex > maxIndex) focusedIndex = maxIndex;
  });

  // 主页滚动
  $effect(() => {
    if (ribbonRef && currentView === 'home') {
      const cards = ribbonRef.querySelectorAll('.bigscreen-game-card');
      const targetCard = cards[focusedIndex] as HTMLElement | undefined;
      if (targetCard) targetCard.scrollIntoView({ behavior: 'smooth', inline: 'nearest', block: 'nearest' });
    }
  });

  // 资源库滚动
  $effect(() => {
    if (libraryGridRef && currentView === 'library') {
      const items = libraryGridRef.querySelectorAll('.library-item');
      const targetItem = items[libraryFocusIndex] as HTMLElement | undefined;
      if (targetItem) targetItem.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    }
  });

  const focusedGame = $derived(
    focusedIndex < displayGames.length ? displayGames[focusedIndex] : null
  );

  // 加载主页 hero 大图
  $effect(() => {
    const game = focusedGame;
    if (!game?.steamAppId) return;
    const appId = game.steamAppId;
    if (videoCache[appId]?.heroUrl) return;
    invoke<{ success: boolean; heroUrl?: string }>('get_steam_video_url', { steamAppId: appId })
      .then(result => {
        if (result.success && result.heroUrl) {
          videoCache[appId] = { ...videoCache[appId], heroUrl: result.heroUrl };
          videoCache = { ...videoCache };
        }
      })
      .catch(() => {});
  });

  // 加载主页视频 URL（仅视频模式时）
  $effect(() => {
    if (homePreviewMode !== 'video') return;
    const game = focusedGame;
    if (!game?.steamAppId) return;
    const appId = game.steamAppId;
    if (videoCache[appId]?.hlsUrl || videoCache[appId]?.videoUrl) return;
    invoke<{ videos: Array<{ hlsUrl?: string; mp4Max?: string; thumbnail?: string }> }>('fetch_steam_videos', { steamAppId: appId })
      .then(result => {
        if (result.videos?.length) {
          const v = result.videos[0];
          videoCache[appId] = {
            ...videoCache[appId],
            hlsUrl: v.hlsUrl || undefined,
            videoUrl: v.mp4Max || undefined,
            heroUrl: v.thumbnail || videoCache[appId]?.heroUrl || undefined,
          };
          videoCache = { ...videoCache };
        }
      })
      .catch(() => {});
  });

  // 详情页加载 hero 大图
  $effect(() => {
    const game = detailGame;
    if (!game?.steamAppId || currentView !== 'detail') return;
    const appId = game.steamAppId;
    if (videoCache[appId]?.heroUrl) return;
    invoke<{ success: boolean; heroUrl?: string }>('get_steam_video_url', { steamAppId: appId })
      .then(result => {
        if (result.success && result.heroUrl) {
          videoCache[appId] = { ...videoCache[appId], heroUrl: result.heroUrl };
          videoCache = { ...videoCache };
        }
      })
      .catch(() => {});
  });

  // 详情页加载视频 URL
  $effect(() => {
    const game = detailGame;
    if (!game?.steamAppId || currentView !== 'detail') return;
    const appId = game.steamAppId;
    if (videoCache[appId]?.hlsUrl || videoCache[appId]?.videoUrl) return;
    invoke<{ videos: Array<{ hlsUrl?: string; mp4Max?: string; thumbnail?: string }> }>('fetch_steam_videos', { steamAppId: appId })
      .then(result => {
        if (result.videos?.length) {
          const v = result.videos[0];
          videoCache[appId] = {
            ...videoCache[appId],
            hlsUrl: v.hlsUrl || undefined,
            videoUrl: v.mp4Max || undefined,
            heroUrl: v.thumbnail || videoCache[appId]?.heroUrl || undefined,
          };
          videoCache = { ...videoCache };
        }
      })
      .catch(() => {});
  });
</script>

<div class="bigscreen-container" class:loaded={isLoaded}>
  <!-- 背景图层 -->
  {#if currentView === 'home'}
    {#each displayGames as game, index}
      <div class="bg-layer" class:active={index === focusedIndex}>
        {#if homePreviewMode === 'video' && index === focusedIndex && game.steamAppId && (videoCache[game.steamAppId]?.hlsUrl || videoCache[game.steamAppId]?.videoUrl)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video
            bind:this={homeVideoEl}
            class="bg-image hero-image"
            loop
            muted
            playsinline
            style="object-fit: cover;"
          ></video>
        {:else if game.steamAppId && videoCache[game.steamAppId]?.heroUrl}
          <img src={videoCache[game.steamAppId].heroUrl} alt="" class="bg-image hero-image" />
        {:else if getCoverUrl(game.cover)}
          <img src={getCoverUrl(game.cover)} alt="" class="bg-image" />
        {:else}
          <div class="bg-placeholder"></div>
        {/if}
        <div class="bg-overlay-top"></div>
        <div class="bg-overlay-bottom"></div>
        <div class="bg-overlay-left"></div>
      </div>
    {/each}
  {/if}

  <!-- 顶部导航 -->
  <div class="top-nav">
    <div class="nav-tabs">
      <button class="nav-tab" class:active={currentView === 'home'} onclick={() => currentView = 'home'}>
        {t('bs_games')}
      </button>
    </div>

    <div class="nav-right">
      <button class="nav-pill-btn" aria-label="Search" onclick={() => { currentView = 'search'; searchQuery = ''; }}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="11" cy="11" r="8"/>
          <path d="m21 21-4.35-4.35"/>
        </svg>
      </button>
      <button class="nav-pill-btn" aria-label="Settings" onclick={() => currentView = 'settings'}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
          <circle cx="12" cy="12" r="3"/>
        </svg>
      </button>
      <div class="nav-status">
        <div class="battery-widget" class:ac={isAC} class:charging={isCharging && !isAC} class:low={batteryPercent <= 20 && !isAC && !isCharging}>
          <span class="battery-percent">{isAC ? 'AC' : `${batteryPercent}%`}</span>
          {#if isAC}
            <div class="ac-plug">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 2v6M8 2v6M16 2v6M8 8h8a4 4 0 0 1 4 4v2a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-2a4 4 0 0 1 4-4ZM12 16v6"/>
              </svg>
              <div class="ac-dot"></div>
            </div>
          {:else}
            <div class="battery-shell">
              <div class="battery-tip"></div>
              <div class="battery-fill" style="width: {Math.max(5, batteryPercent)}%"></div>
              {#if isCharging}
                <div class="charging-bolt">
                  <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>
                </div>
              {/if}
            </div>
          {/if}
        </div>
        <div class="user-time-group">
          {#if gamepadConnected}
            <div class="gamepad-indicator" title="Controller connected">
              <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
                <path d="M7.97 16L5 19c-.9.9-2.37.9-3.27 0-.9-.9-.9-2.37 0-3.27L4.97 12.5C4.36 11.78 4 10.93 4 10V5c0-1.66 1.34-3 3-3h10c1.66 0 3 1.34 3 3v5c0 .93-.36 1.78-.97 2.5L22.27 15.73c.9.9.9 2.37 0 3.27-.9.9-2.37.9-3.27 0L16.03 16H7.97zM8 7c-.55 0-1 .45-1 1s.45 1 1 1 1-.45 1-1-.45-1-1-1zm8 0c-.55 0-1 .45-1 1s.45 1 1 1 1-.45 1-1-.45-1-1-1z"/>
              </svg>
            </div>
          {/if}
          <div class="user-avatar"></div>
          <span class="time-display">{currentTime}</span>
        </div>
      </div>
    </div>
  </div>

  <!-- 主内容区: 首页 -->
  {#if currentView === 'home'}
    <div class="main-content" class:loaded={isLoaded}>
      <div class="game-ribbon" bind:this={ribbonRef}>
        {#each displayGames as game, index}
          <button
            class="bigscreen-game-card"
            class:focused={index === focusedIndex}
            onclick={() => { focusedIndex = index; }}
            ondblclick={() => handleLaunch(game)}
            oncontextmenu={(e) => openContextMenu(e, game)}
          >
            <div class="card-image-wrapper">
              {#if getCoverUrl(game.cover)}
                <img src={getCoverUrl(game.cover)} alt={game.name} class="card-image" />
              {:else}
                <div class="card-placeholder">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <rect x="2" y="6" width="20" height="12" rx="2"/>
                    <line x1="6" y1="12" x2="10" y2="12"/>
                    <line x1="8" y1="10" x2="8" y2="14"/>
                    <circle cx="17" cy="12" r="1"/>
                  </svg>
                </div>
              {/if}
              {#if game.pinned}
                <span class="pin-badge" title="{t('bs_pinned_to_home')}">📌</span>
              {/if}
            </div>
            {#if index !== focusedIndex}
              <span class="card-title">{game.name}</span>
            {/if}
          </button>
        {/each}

        <!-- 游戏库入口 -->
        <button class="bigscreen-game-card library-card" class:focused={focusedIndex === displayGames.length} onclick={() => { libraryFocusIndex = 0; currentView = 'library'; }}>
          <div class="card-image-wrapper">
            <div class="library-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/>
                <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>
              </svg>
              <span>{t('bs_library')}</span>
            </div>
          </div>
        </button>
      </div>

      <!-- 焦点游戏信息 -->
      {#if focusedGame}
        <div class="hero-content">
          <h1 class="game-logo">{focusedGame.name}</h1>
          <div class="game-meta">
            {#if focusedGame.source}
              <span class="meta-tag">{getPlatformLabel(focusedGame.source)}</span>
              <span class="meta-dot">•</span>
            {/if}
            {#if focusedGame.completionStatus && focusedGame.completionStatus !== 'not_played'}
              <span class="meta-status" style="color: {getStatusColor(focusedGame.completionStatus)}">{getStatusLabel(focusedGame.completionStatus)}</span>
              <span class="meta-dot">•</span>
            {/if}
            {#if focusedGame.totalPlayTime && focusedGame.totalPlayTime > 0}
              <span class="meta-tag">{formatPlayTime(focusedGame.totalPlayTime)}</span>
              <span class="meta-dot">•</span>
            {/if}
            {#if focusedGame.lastPlayedAt}
              <span class="meta-tag">{formatLastPlayed(focusedGame.lastPlayedAt)}</span>
              <span class="meta-dot">•</span>
            {/if}
            <span class="meta-badge">KOGAMES</span>
          </div>

          <div class="action-row">
            <button class="play-btn" onclick={() => handleLaunch(focusedGame)}>
              <svg viewBox="0 0 24 24" fill="currentColor">
                <polygon points="5 3 19 12 5 21 5 3"/>
              </svg>
              {focusedGame.lastPlayedAt ? t('bs_continue_game') : t('bs_start_game')}
            </button>
            <button class="detail-btn" onclick={() => openDetail(focusedGame)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>
              </svg>
            </button>
          </div>
        </div>
      {:else if games.length === 0}
        <div class="hero-content empty-state">
          <h2>{t('bs_no_games')}</h2>
          <p>{t('bs_no_games_hint')}</p>
        </div>
      {/if}

      <!-- 底部统计栏 -->
      {#if stats.total > 0}
        <div class="home-stats">
          <span>{t('bs_total_games').replace('{count}', String(stats.total))}</span>
          {#if stats.totalHours > 0}
            <span class="stats-dot">•</span>
            <span>{t('bs_total_playtime').replace('{hours}', String(stats.totalHours))}</span>
          {/if}
          {#if stats.playing > 0}
            <span class="stats-dot">•</span>
            <span style="color: #22c55e">{stats.playing} {t('bs_status_playing')}</span>
          {/if}
        </div>
      {/if}
    </div>

  <!-- 资源库视图 -->
  {:else if currentView === 'library'}
    <div class="library-view">
      <!-- 资源库动态背景 -->
      {#if filteredGames[libraryFocusIndex] && getCoverUrl(filteredGames[libraryFocusIndex]?.cover)}
        <div class="library-bg">
          <img src={getCoverUrl(filteredGames[libraryFocusIndex]?.cover)} alt="" />
        </div>
      {/if}
      <div class="library-header">
        <button class="back-btn" aria-label="{t('bs_cancel')}" onclick={() => currentView = 'home'}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M19 12H5M12 19l-7-7 7-7"/>
          </svg>
        </button>
        <h2>{t('bs_game_library')}</h2>
        <span class="library-count">{filteredGames.length}</span>
      </div>

      <!-- 排序/过滤栏 -->
      <div class="filter-bar">
        <div class="filter-section">
          <div class="filter-pills">
            {#each allSorts as s}
              <button class="filter-pill" class:active={sortMode === s.mode} onclick={() => sortMode = s.mode}>
                {t(s.key)}
              </button>
            {/each}
          </div>
        </div>
        <div class="filter-section">
          <div class="filter-pills">
            {#each allPlatforms as p}
              <button class="filter-pill platform-pill" class:active={filterPlatform === p} onclick={() => filterPlatform = p}>
                {p === 'all' ? t('bs_filter_all') : p === 'manual' ? t('bs_filter_manual') : getPlatformLabel(p)}
              </button>
            {/each}
          </div>
        </div>
        <div class="filter-section">
          <div class="filter-pills">
            <button class="filter-pill" class:active={filterStatus === 'all'} onclick={() => filterStatus = 'all'}>{t('bs_filter_all')}</button>
            {#each allStatuses as s}
              <button class="filter-pill status-pill" class:active={filterStatus === s} onclick={() => filterStatus = s}
                style="--status-color: {getStatusColor(s)}">
                {getStatusLabel(s)}
              </button>
            {/each}
          </div>
        </div>
        {#if allTags.length > 0}
        <div class="filter-section">
          <div class="filter-pills">
            <button class="filter-pill" class:active={filterTag === ''} onclick={() => filterTag = ''}>{t('bs_filter_all')}</button>
            {#each allTags.slice(0, 10) as tag}
              <button class="filter-pill" class:active={filterTag === tag} onclick={() => filterTag = filterTag === tag ? '' : tag}
                style="--status-color: #6366f1">
                {tag}
              </button>
            {/each}
          </div>
        </div>
        {/if}
      </div>

      <div class="library-grid" bind:this={libraryGridRef}>
        {#each filteredGames as game, index}
          <div
            class="library-item"
            class:focused={libraryFocusIndex === index}
            role="button"
            tabindex="0"
            onclick={() => { libraryFocusIndex = index; openDetail(game); }}
            oncontextmenu={(e) => openContextMenu(e, game)}
            onkeydown={() => {}}
          >
            <div class="library-cover">
              {#if getCoverUrl(game.cover)}
                <img src={getCoverUrl(game.cover)} alt={game.name} />
              {:else}
                <div class="cover-placeholder">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <rect x="2" y="6" width="20" height="12" rx="2"/>
                  </svg>
                </div>
              {/if}
              {#if game.pinned}
                <span class="pin-badge">📌</span>
              {/if}
              {#if game.favorite}
                <span class="fav-badge">❤️</span>
              {/if}
              {#if game.completionStatus && game.completionStatus !== 'not_played'}
                <span class="status-dot" style="background: {getStatusColor(game.completionStatus)}"></span>
              {/if}
              {#if yHoldProgress > 0 && libraryFocusIndex === index}
                <div class="y-hold-overlay">
                  <div class="y-hold-bar" style="width: {yHoldProgress}%"></div>
                </div>
              {/if}
            </div>
            <span class="library-title">{game.name}</span>
            {#if game.totalPlayTime && game.totalPlayTime > 0}
              <span class="library-playtime">{formatPlayTime(game.totalPlayTime)}</span>
            {/if}
          </div>
        {/each}
        <!-- 添加游戏按钮 -->
        <div
          class="library-item add-game-item"
          class:focused={libraryFocusIndex === filteredGames.length}
          role="button"
          tabindex="0"
          onclick={() => { libraryFocusIndex = filteredGames.length; onAddGame(); }}
          onkeydown={() => {}}
        >
          <div class="library-cover add-game-cover">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="12" y1="5" x2="12" y2="19"/>
              <line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
          </div>
          <span class="library-title">{t('bs_add_game')}</span>
        </div>
      </div>
    </div>

  <!-- 游戏详情视图 -->
  {:else if currentView === 'detail' && detailGame}
    <div class="detail-view">
      <!-- 背景 -->
      <div class="detail-bg">
        {#if detailGame.steamAppId && (videoCache[detailGame.steamAppId]?.hlsUrl || videoCache[detailGame.steamAppId]?.videoUrl)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video
            bind:this={detailVideoEl}
            class="detail-bg-video"
            loop
            muted
            playsinline
          ></video>
        {:else if detailGame.backgroundImage}
          <img src={detailGame.backgroundImage} alt="" class="detail-bg-img" />
        {:else if detailGame.steamAppId && videoCache[detailGame.steamAppId]?.heroUrl}
          <img src={videoCache[detailGame.steamAppId].heroUrl} alt="" class="detail-bg-img" />
        {:else if getCoverUrl(detailGame.cover)}
          <img src={getCoverUrl(detailGame.cover)} alt="" class="detail-bg-img cover-bg" />
        {:else}
          <div class="detail-bg-gradient"></div>
        {/if}
        <div class="detail-bg-overlay"></div>
      </div>

      <!-- 内容 -->
      <div class="detail-content">
        <button class="back-btn detail-back" onclick={() => currentView = 'library'}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M19 12H5M12 19l-7-7 7-7"/>
          </svg>
        </button>

        <div class="detail-info">
          <!-- 封面 + 信息 -->
          <div class="detail-layout">
            <div class="detail-cover-wrap">
              {#if getCoverUrl(detailGame.cover)}
                <img src={getCoverUrl(detailGame.cover)} alt={detailGame.name} class="detail-cover-img" />
              {:else}
                <div class="detail-cover-placeholder">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <rect x="2" y="6" width="20" height="12" rx="2"/>
                    <line x1="6" y1="12" x2="10" y2="12"/>
                    <line x1="8" y1="10" x2="8" y2="14"/>
                    <circle cx="17" cy="12" r="1"/>
                  </svg>
                </div>
              {/if}
            </div>

            <div class="detail-meta">
              <h1 class="detail-title">{detailGame.name}</h1>

              <div class="detail-tags">
                {#if detailGame.source}
                  <span class="detail-tag platform">{getPlatformLabel(detailGame.source)}</span>
                {/if}
                <span class="detail-tag status" style="background: {getStatusColor(detailGame.completionStatus)}; color: #fff">
                  {getStatusLabel(detailGame.completionStatus)}
                </span>
                {#if detailGame.genre}
                  <span class="detail-tag platform">{detailGame.genre}</span>
                {/if}
                {#if detailGame.releaseYear}
                  <span class="detail-tag platform">{detailGame.releaseYear}</span>
                {/if}
                {#if detailGame.favorite}
                  <span class="detail-tag fav">❤️ {t('bs_favorite')}</span>
                {/if}
              </div>

              {#if detailGame.description}
                <p class="detail-description">{detailGame.description}</p>
              {/if}

              <div class="detail-stats-row">
                <div class="detail-stat">
                  <span class="stat-label">{t('bs_play_time')}</span>
                  <span class="stat-value">{formatPlayTime(detailGame.totalPlayTime)}</span>
                </div>
                <div class="detail-stat">
                  <span class="stat-label">{t('bs_last_played')}</span>
                  <span class="stat-value">{formatLastPlayed(detailGame.lastPlayedAt)}</span>
                </div>
                {#if detailGame.source}
                  <div class="detail-stat">
                    <span class="stat-label">{t('bs_platform')}</span>
                    <span class="stat-value">{getPlatformLabel(detailGame.source)}</span>
                  </div>
                {/if}
                {#if detailGame.playCount}
                  <div class="detail-stat">
                    <span class="stat-label">{t('bs_play_count')}</span>
                    <span class="stat-value">{detailGame.playCount}{t('bs_total_sessions')}</span>
                  </div>
                {/if}
                {#if detailGame.installSize}
                  <div class="detail-stat">
                    <span class="stat-label">{t('bs_install_size')}</span>
                    <span class="stat-value">{(detailGame.installSize / 1073741824).toFixed(1)} GB</span>
                  </div>
                {/if}
              </div>

              {#if detailGame.developers?.length || detailGame.publishers?.length || detailGame.criticScore || detailGame.releaseDate}
              <div class="detail-stats-row" style="margin-top: 0.8rem;">
                {#if detailGame.developers?.length}
                  <div class="detail-stat">
                    <span class="stat-label">{t('bs_developers')}</span>
                    <span class="stat-value">{detailGame.developers.join(', ')}</span>
                  </div>
                {/if}
                {#if detailGame.publishers?.length}
                  <div class="detail-stat">
                    <span class="stat-label">{t('bs_publishers')}</span>
                    <span class="stat-value">{detailGame.publishers.join(', ')}</span>
                  </div>
                {/if}
                {#if detailGame.criticScore}
                  <div class="detail-stat">
                    <span class="stat-label">{t('bs_critic_score')}</span>
                    <span class="stat-value" style="color: {detailGame.criticScore >= 75 ? '#4ade80' : detailGame.criticScore >= 50 ? '#facc15' : '#f87171'}">{detailGame.criticScore}</span>
                  </div>
                {/if}
                {#if detailGame.releaseDate}
                  <div class="detail-stat">
                    <span class="stat-label">{t('bs_release_date')}</span>
                    <span class="stat-value">{detailGame.releaseDate}</span>
                  </div>
                {/if}
              </div>
              {/if}

              {#if detailGame.tags?.length}
              <div style="margin-top: 0.8rem; display: flex; flex-wrap: wrap; gap: 0.4rem;">
                {#each detailGame.tags as tag}
                  <span style="background: rgba(255,255,255,0.1); padding: 0.2rem 0.6rem; border-radius: 1rem; font-size: 0.8rem; color: rgba(255,255,255,0.7);">{tag}</span>
                {/each}
              </div>
              {/if}

              {#if detailGame.series}
              <div style="margin-top: 0.5rem; font-size: 0.85rem; color: rgba(255,255,255,0.5);">
                {t('bs_series')}: {detailGame.series}
              </div>
              {/if}
            </div>
          </div>

          <!-- 操作按钮 -->
          <div class="detail-actions">
            <button class="action-btn play" class:focused={detailFocusIndex === 0}
              onclick={() => handleLaunch(detailGame!)}>
              <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
              {detailGame.lastPlayedAt ? t('bs_continue') : t('bs_play')}
            </button>
            <button class="action-btn" class:focused={detailFocusIndex === 1}
              onclick={() => { onTogglePin(detailGame!); showBsToast(!detailGame!.pinned ? t('bs_pin') : t('bs_unpin')); }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 17v5M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.89A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.89A2 2 0 0 0 5 15.24Z"/>
              </svg>
              {detailGame.pinned ? t('bs_unpin') : t('bs_pin')}
            </button>
            <button class="action-btn" class:focused={detailFocusIndex === 2}
              onclick={() => handleToggleFavorite(detailGame!)}>
              <svg viewBox="0 0 24 24" fill={detailGame.favorite ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2">
                <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"/>
              </svg>
              {detailGame.favorite ? t('bs_unfavorite') : t('bs_favorite')}
            </button>
            <button class="action-btn" class:focused={detailFocusIndex === 3}
              onclick={() => showStatusPicker = !showStatusPicker}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="9 11 12 14 22 4"/>
                <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/>
              </svg>
              {t('bs_change_status')}
            </button>
            <button class="action-btn" class:focused={detailFocusIndex === 4}
              onclick={() => handleChangeCover(detailGame!)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
                <circle cx="8.5" cy="8.5" r="1.5"/>
                <polyline points="21 15 16 10 5 21"/>
              </svg>
              {t('bs_change_cover')}
            </button>
            <button class="action-btn" class:focused={detailFocusIndex === 5}
              onclick={toggleProperties}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
              </svg>
              {t('bs_properties')}
            </button>
            <button class="action-btn" class:focused={detailFocusIndex === 6}
              onclick={() => toggleHideGame(detailGame!)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                {#if detailGame.hidden}
                  <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                  <circle cx="12" cy="12" r="3"/>
                {:else}
                  <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/>
                  <line x1="1" y1="1" x2="23" y2="23"/>
                {/if}
              </svg>
              {detailGame.hidden ? t('bs_show_game') : t('bs_hide_game')}
            </button>
            <button class="action-btn danger" class:focused={detailFocusIndex === 7}
              onclick={() => showDeleteConfirm = true}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="3 6 5 6 21 6"/>
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
              </svg>
              {t('bs_delete_game')}
            </button>
          </div>

          <!-- 状态选择器 -->
          {#if showStatusPicker}
            <div class="status-picker">
              {#each allStatuses as s}
                <button class="status-option" class:active={detailGame.completionStatus === s || (!detailGame.completionStatus && s === 'not_played')}
                  style="--dot-color: {getStatusColor(s)}"
                  onclick={() => handleSetStatus(detailGame!, s)}>
                  <span class="status-dot-sm"></span>
                  {getStatusLabel(s)}
                </button>
              {/each}
            </div>
          {/if}

          <!-- 删除确认 -->
          {#if showDeleteConfirm}
            <div class="delete-confirm">
              <p>{t('bs_delete_confirm')}</p>
              <div class="confirm-actions">
                <button class="confirm-btn cancel" onclick={() => showDeleteConfirm = false}>{t('bs_cancel')}</button>
                <button class="confirm-btn danger" onclick={() => handleDeleteGame(detailGame!)}>{t('bs_confirm')}</button>
              </div>
            </div>
          {/if}

          <!-- 属性面板 -->
          {#if showProperties && detailGame}
            <div class="properties-panel">
              <div class="prop-section">
                <h4 class="prop-title">{t('bs_local_files')}</h4>
                {#if detailGame.installLocation}
                  <button class="prop-item" onclick={() => openFolder(detailGame!.installLocation!)}>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
                    <div class="prop-text">
                      <span class="prop-label">{t('bs_install_path')}</span>
                      <span class="prop-value">{detailGame.installLocation}</span>
                    </div>
                  </button>
                {/if}
                {#if detailGame.path}
                  <div class="prop-item static">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>
                    <div class="prop-text">
                      <span class="prop-label">{t('bs_exe_path')}</span>
                      <span class="prop-value">{detailGame.path}</span>
                    </div>
                  </div>
                {/if}
              </div>

              {#if savePaths.length > 0}
                <div class="prop-section">
                  <h4 class="prop-title">{t('bs_save_files')}</h4>
                  {#each savePaths as sp}
                    <button class="prop-item" onclick={() => openFolder(sp.path)}>
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        {#if sp.type === 'steam_cloud'}
                          <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z"/>
                        {:else}
                          <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/>
                          <polyline points="17 21 17 13 7 13 7 21"/>
                          <polyline points="7 3 7 8 15 8"/>
                        {/if}
                      </svg>
                      <div class="prop-text">
                        <span class="prop-label">{sp.label}</span>
                        <span class="prop-value">{sp.path}</span>
                      </div>
                    </button>
                  {/each}
                </div>
              {/if}

              <div class="prop-section">
                <h4 class="prop-title">{t('bs_tags')}</h4>
                <div style="display: flex; flex-wrap: wrap; gap: 0.4rem; margin-bottom: 0.5rem;">
                  {#each (detailGame.tags ?? []) as tag, i}
                    <span style="background: rgba(99,102,241,0.3); padding: 0.2rem 0.5rem; border-radius: 1rem; font-size: 0.8rem; display: flex; align-items: center; gap: 0.3rem;">
                      {tag}
                      <button style="background: none; border: none; color: rgba(255,255,255,0.5); cursor: pointer; padding: 0; font-size: 0.9rem;" onclick={() => {
                        const newTags = [...(detailGame!.tags ?? [])];
                        newTags.splice(i, 1);
                        updateGame(detailGame!, { tags: newTags });
                        detailGame!.tags = newTags;
                      }}>×</button>
                    </span>
                  {/each}
                </div>
                <div style="display: flex; gap: 0.3rem;">
                  <input type="text" placeholder={t('bs_tags') + '...'} style="flex: 1; background: rgba(255,255,255,0.1); border: 1px solid rgba(255,255,255,0.2); border-radius: 0.5rem; padding: 0.3rem 0.6rem; color: white; font-size: 0.8rem; outline: none;" onkeydown={(e: KeyboardEvent) => {
                    if (e.key === 'Enter') {
                      const input = e.currentTarget as HTMLInputElement;
                      const val = input.value.trim();
                      if (val && detailGame) {
                        const newTags = [...(detailGame.tags ?? []), val];
                        updateGame(detailGame, { tags: newTags });
                        detailGame.tags = newTags;
                        input.value = '';
                      }
                    }
                  }} />
                </div>
              </div>

              <div class="prop-section">
                <h4 class="prop-title">{t('bs_notes')}</h4>
                <textarea style="width: 100%; min-height: 60px; background: rgba(255,255,255,0.1); border: 1px solid rgba(255,255,255,0.2); border-radius: 0.5rem; padding: 0.5rem; color: white; font-size: 0.8rem; outline: none; resize: vertical;" value={detailGame.notes ?? ''} onblur={(e: FocusEvent) => {
                  const val = (e.currentTarget as HTMLTextAreaElement).value;
                  if (val !== (detailGame?.notes ?? '')) {
                    updateGame(detailGame!, { notes: val });
                    detailGame!.notes = val;
                  }
                }}></textarea>
              </div>

              {#if detailGame.steamAppId}
                <div class="prop-section">
                  <h4 class="prop-title">Steam</h4>
                  <div class="prop-item static">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>
                    <div class="prop-text">
                      <span class="prop-label">App ID</span>
                      <span class="prop-value">{detailGame.steamAppId}</span>
                    </div>
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    </div>

  <!-- 设置视图 - PS5风格双栏布局 -->
  {:else if currentView === 'settings'}
    <div class="settings-view">
      <!-- 左侧分类导航栏 -->
      <div class="settings-sidebar">
        <div class="settings-sidebar-header">
          <button class="back-btn" onclick={() => currentView = 'home'} aria-label="{t('bs_cancel')}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M19 12H5M12 19l-7-7 7-7"/>
            </svg>
          </button>
          <h2>{t('bs_settings')}</h2>
        </div>
        <nav class="settings-nav">
          <button class="settings-nav-item" class:active={settingsTab === 'stats'} onclick={() => settingsTab = 'stats'}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M18 20V10"/><path d="M12 20V4"/><path d="M6 20v-6"/>
            </svg>
            <span>{t('bs_stats')}</span>
          </button>

          <button class="settings-nav-item" class:active={settingsTab === 'import'} onclick={() => settingsTab = 'import'}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="7 10 12 15 17 10"/>
              <line x1="12" y1="15" x2="12" y2="3"/>
            </svg>
            <span>{t('bs_import_games')}</span>
          </button>
          <button class="settings-nav-item" class:active={settingsTab === 'data'} onclick={() => settingsTab = 'data'}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <ellipse cx="12" cy="5" rx="9" ry="3"/>
              <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/>
              <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/>
            </svg>
            <span>{t('bs_data_management')}</span>
          </button>
          <button class="settings-nav-item" class:active={settingsTab === 'about'} onclick={() => settingsTab = 'about'}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10"/>
              <path d="M12 16v-4"/>
              <path d="M12 8h.01"/>
            </svg>
            <span>{t('bs_about')}</span>
          </button>

        </nav>
      </div>

      <!-- 右侧内容面板 -->
      <div class="settings-content">
        {#if settingsTab === 'stats'}
          <div class="settings-panel" >
            <div class="stats-dashboard">
              <div class="stat-card">
                <span class="stat-value">{stats.total}</span>
                <span class="stat-label">{t('bs_total_games_label')}</span>
              </div>
              <div class="stat-card">
                <span class="stat-value">{stats.totalHours}</span>
                <span class="stat-label">{t('bs_total_hours_label')}</span>
              </div>
              <div class="stat-card accent">
                <span class="stat-value">{stats.playing}</span>
                <span class="stat-label">{t('bs_playing_label')}</span>
              </div>
              <div class="stat-card success">
                <span class="stat-value">{stats.completed}</span>
                <span class="stat-label">{t('bs_completed_label')}</span>
              </div>
              <div class="stat-card">
                <span class="stat-value">{stats.backlog}</span>
                <span class="stat-label">{t('bs_backlog_label')}</span>
              </div>
            </div>

            {#if stats.mostPlayed.length > 0 && stats.mostPlayed[0].totalPlayTime}
              <div class="most-played">
                <h4>{t('bs_most_played_label')}</h4>
                {#each stats.mostPlayed as game}
                  {#if game.totalPlayTime && game.totalPlayTime > 0}
                    <div class="mp-item">
                      <div class="mp-cover">
                        {#if getCoverUrl(game.cover)}
                          <img src={getCoverUrl(game.cover)} alt="" />
                        {/if}
                      </div>
                      <span class="mp-name">{game.name}</span>
                      <span class="mp-time">{formatPlayTime(game.totalPlayTime)}</span>
                      <div class="mp-bar">
                        <div class="mp-fill" style="width: {Math.min(100, (game.totalPlayTime / (stats.mostPlayed[0]?.totalPlayTime ?? 1)) * 100)}%"></div>
                      </div>
                    </div>
                  {/if}
                {/each}
              </div>
            {/if}

            {#if Object.keys(stats.platformCounts).length > 0}
              <div class="platform-breakdown">
                <h4>{t('bs_platform_dist')}</h4>
                <div class="platform-bars">
                  {#each Object.entries(stats.platformCounts).sort((a, b) => b[1] - a[1]) as [platform, count]}
                    <div class="pb-item">
                      <span class="pb-label">{getPlatformLabel(platform)}</span>
                      <div class="pb-bar">
                        <div class="pb-fill" style="width: {Math.min(100, (count / stats.total) * 100)}%"></div>
                      </div>
                      <span class="pb-count">{count}</span>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          </div>

        {:else if settingsTab === 'import'}
          <div class="settings-panel">
            <button class="settings-item" onclick={handleImportSteam} disabled={isImporting}>
              <div class="settings-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2z"/>
                  <path d="M12 6v6l4 2"/>
                </svg>
              </div>
              <div class="settings-text">
                <h3>{t('bs_import_steam')}</h3>
                <p>{isImporting ? t('bs_importing') : 'Steam'}</p>
              </div>
              <svg class="settings-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6"/>
              </svg>
            </button>

            <button class="settings-item" onclick={handleImportAll} disabled={isImporting}>
              <div class="settings-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
                  <line x1="8" y1="21" x2="16" y2="21"/>
                  <line x1="12" y1="17" x2="12" y2="21"/>
                </svg>
              </div>
              <div class="settings-text">
                <h3>{t('bs_import_all')}</h3>
                <p>{isImporting ? t('bs_importing') : 'Epic / EA / Ubisoft / Xbox / GOG'}</p>
              </div>
              <svg class="settings-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6"/>
              </svg>
            </button>

            <button class="settings-item" onclick={handleBatchMetadata} disabled={isFetchingMetadata}>
              <div class="settings-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                  <polyline points="7 10 12 15 17 10"/>
                  <line x1="12" y1="15" x2="12" y2="3"/>
                </svg>
              </div>
              <div class="settings-text">
                <h3>{isFetchingMetadata ? '...' : (t('bs_auto_import') || 'Batch Metadata')}</h3>
                <p>{t('bs_auto_import_desc') || 'Download metadata & covers for all games'}</p>
              </div>
              <svg class="settings-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6"/>
              </svg>
            </button>
          </div>

        {:else if settingsTab === 'data'}
          <div class="settings-panel">
            <button class="settings-item" onclick={handleBackup}>
              <div class="settings-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/>
                  <polyline points="17 21 17 13 7 13 7 21"/>
                  <polyline points="7 3 7 8 15 8"/>
                </svg>
              </div>
              <div class="settings-text">
                <h3>{t('bs_backup')}</h3>
                <p>ZIP</p>
              </div>
              <svg class="settings-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6"/>
              </svg>
            </button>

            <button class="settings-item" onclick={handleExport}>
              <div class="settings-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                  <polyline points="17 8 12 3 7 8"/>
                  <line x1="12" y1="3" x2="12" y2="15"/>
                </svg>
              </div>
              <div class="settings-text">
                <h3>{t('bs_export_library')}</h3>
                <p>JSON</p>
              </div>
              <svg class="settings-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6"/>
              </svg>
            </button>
          </div>

        {:else if settingsTab === 'about'}
          <div class="settings-panel">
            <button class="settings-item" onclick={() => invoke('open_url', { url: 'https://github.com/kobolingfeng/KoGames' })}>
              <div class="settings-icon">
                <svg viewBox="0 0 24 24" fill="currentColor">
                  <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z"/>
                </svg>
              </div>
              <div class="settings-text">
                <h3>{t('bs_github_repo')}</h3>
                <p>{t('bs_github_star')}</p>
              </div>
              <svg class="settings-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6"/>
              </svg>
            </button>

            <button class="settings-item" onclick={() => { showSponsor = !showSponsor; }}>
              <div class="settings-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"/>
                </svg>
              </div>
              <div class="settings-text">
                <h3>{t('bs_sponsor')}</h3>
                <p>{t('bs_sponsor_desc')}</p>
              </div>
              <svg class="settings-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6"/>
              </svg>
            </button>

            {#if showSponsor}
              <div class="sponsor-panel">
                <div class="sponsor-grid">
                  <div class="sponsor-card">
                    <h4>{t('bs_wechat_donate')}</h4>
                    <img src="/docs/wechat_donate.png" alt="WeChat Donate" class="sponsor-qr" />
                  </div>
                  <div class="sponsor-card">
                    <h4>{t('bs_ldxp_donate')}</h4>
                    <img src="/docs/ldxp_qrcode.png" alt="LDXP QR" class="sponsor-qr" />
                  </div>
                  <div class="sponsor-card">
                    <h4>{t('bs_paypal_donate')}</h4>
                    <a href="#" class="paypal-link" onclick={(e) => { e.preventDefault(); invoke('open_url', { url: 'https://paypal.me/koboling' }); }}>
                      paypal.me/koboling
                    </a>
                  </div>
                </div>
              </div>
            {/if}
          </div>

        {/if}
      </div>
    </div>

  <!-- 搜索视图 -->
  {:else if currentView === 'search'}
    <div class="search-view">
      <div class="search-header">
        <div class="search-input-wrapper">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="search-icon">
            <circle cx="11" cy="11" r="8"/>
            <path d="m21 21-4.35-4.35"/>
          </svg>
          <input
            type="text"
            class="search-input"
            placeholder={t('bs_search_games')}
            bind:value={searchQuery}
          />
          <button class="search-cancel" onclick={() => currentView = 'home'}>{t('bs_cancel')}</button>
        </div>
      </div>

      <div class="search-results">
        {#if searchQuery.trim()}
          <div class="search-count">{t('bs_found_results').replace('{count}', String(searchResults.length))}</div>
          <div class="search-grid">
            {#each searchResults as game}
              <div
                class="search-item"
                role="button"
                tabindex="0"
                onclick={() => handleLaunch(game)}
                onkeydown={() => {}}
              >
                <div class="search-cover">
                  {#if getCoverUrl(game.cover)}
                    <img src={getCoverUrl(game.cover)} alt={game.name} />
                  {:else}
                    <div class="cover-placeholder">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <rect x="2" y="6" width="20" height="12" rx="2"/>
                      </svg>
                    </div>
                  {/if}
                </div>
                <div class="search-info">
                  <span class="search-title">{game.name}</span>
                  {#if game.source}
                    <span class="search-source">{game.source}</span>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="search-hint">{t('bs_search_hint')}</div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- 控制中心 - PS5风格 -->
  <div class="control-center" class:open={controlCenterOpen}>
    <div class="cc-header">
      <h3>{t('bs_control_center')}</h3>
      <button class="cc-close" aria-label="{t('bs_cancel')}" onclick={() => controlCenterOpen = false}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 6L6 18M6 6l12 12"/>
        </svg>
      </button>
    </div>

    <div class="cc-grid">
      <!-- 音效开关 -->
      <button class="cc-card" class:cc-active={soundEnabled} onclick={() => { soundEnabled = !soundEnabled; playSound('confirm'); saveSettings(); }}>
        <div class="cc-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            {#if soundEnabled}
              <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
              <path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"/>
            {:else}
              <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
              <line x1="23" y1="9" x2="17" y2="15"/>
              <line x1="17" y1="9" x2="23" y2="15"/>
            {/if}
          </svg>
        </div>
        <span class="cc-label">{t('bs_nav_sound')}</span>
        <span class="cc-status">{soundEnabled ? 'ON' : 'OFF'}</span>
      </button>

      <!-- 屏保开关 -->
      <button class="cc-card" class:cc-active={screenSaverEnabled} onclick={() => { screenSaverEnabled = !screenSaverEnabled; resetIdleTimer(); saveSettings(); }}>
        <div class="cc-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>
          </svg>
        </div>
        <span class="cc-label">{t('bs_screensaver')}</span>
        <span class="cc-status">{screenSaverEnabled ? 'ON' : 'OFF'}</span>
      </button>

      <!-- 语言切换 -->
      <button class="cc-card" onclick={toggleLanguage}>
        <div class="cc-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10"/>
            <line x1="2" y1="12" x2="22" y2="12"/>
            <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
          </svg>
        </div>
        <span class="cc-label">{t('bs_language')}</span>
        <span class="cc-status">{currentLang === 'zh' ? '中文' : 'EN'}</span>
      </button>

      <!-- 显示隐藏游戏 -->
      <button class="cc-card" class:cc-active={showHidden} onclick={() => { showHidden = !showHidden; saveSettings(); }}>
        <div class="cc-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            {#if showHidden}
              <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
              <circle cx="12" cy="12" r="3"/>
            {:else}
              <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/>
              <line x1="1" y1="1" x2="23" y2="23"/>
            {/if}
          </svg>
        </div>
        <span class="cc-label">{t('bs_show_hidden')}</span>
        <span class="cc-status">{showHidden ? 'ON' : 'OFF'}</span>
      </button>

      <!-- 主页预览模式 -->
      <button class="cc-card" class:cc-active={homePreviewMode === 'video'} onclick={() => { homePreviewMode = homePreviewMode === 'hero' ? 'video' : 'hero'; saveSettings(); }}>
        <div class="cc-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            {#if homePreviewMode === 'video'}
              <polygon points="5,3 19,12 5,21" fill="currentColor" stroke="none"/>
            {:else}
              <rect x="2" y="4" width="20" height="16" rx="2"/>
              <circle cx="8" cy="10" r="2"/>
              <path d="M22 20L16 14 12 18 8 14 2 20"/>
            {/if}
          </svg>
        </div>
        <span class="cc-label">{homePreviewMode === 'video' ? t('bs_preview_video') : t('bs_preview_hero')}</span>
        <span class="cc-status">{homePreviewMode === 'video' ? '🎬' : '🖼️'}</span>
      </button>


      <!-- 桌面模式 -->
      <button class="cc-card" onclick={onSwitchMode}>
        <div class="cc-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
            <line x1="8" y1="21" x2="16" y2="21"/>
            <line x1="12" y1="17" x2="12" y2="21"/>
          </svg>
        </div>
        <span class="cc-label">{t('bs_desktop_mode') || 'Desktop'}</span>
        <span class="cc-status">→</span>
      </button>

      <!-- 电池状态 -->
      <div class="cc-card cc-battery" class:cc-charging={isCharging}>
        <div class="cc-icon">
          {#if isAC}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 2v6M8 2v6M16 2v6M8 8h8a4 4 0 0 1 4 4v2a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-2a4 4 0 0 1 4-4ZM12 16v6"/>
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="1" y="6" width="18" height="12" rx="2" ry="2"/>
              <line x1="23" y1="13" x2="23" y2="11"/>
            </svg>
          {/if}
        </div>
        <span class="cc-label">{t('bs_battery')}</span>
        <span class="cc-status">{isAC ? 'AC' : `${batteryPercent}%`} {isCharging ? '⚡' : ''}</span>
      </div>

      <!-- 退出 -->
      <button class="cc-card cc-danger" onclick={onExit}>
        <div class="cc-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>
            <polyline points="16 17 21 12 16 7"/>
            <line x1="21" y1="12" x2="9" y2="12"/>
          </svg>
        </div>
        <span class="cc-label">{t('bs_exit')}</span>
        <span class="cc-status">{t('bs_exit_app')}</span>
      </button>
    </div>
  </div>

  <!-- 浮动返回按钮 -->
  <div class="float-btn-container">
    <button class="float-btn" onclick={() => {
      if (currentView !== 'home') {
        currentView = 'home';
      } else {
        controlCenterOpen = !controlCenterOpen;
      }
    }}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
        {#if currentView !== 'home'}
          <path d="M19 12H5m7-7l-7 7 7 7"/>
        {:else}
          <path d="M6 4v16"/>
          <path d="M19 4l-11 8 11 8"/>
        {/if}
      </svg>
    </button>
  </div>

  <!-- 手柄按键提示 -->
  {#if gamepadConnected}
    <div class="gamepad-hints">
      <div class="gp-hint"><span class="gp-btn">A</span> {t('bs_confirm') ?? '确认'}</div>
      <div class="gp-hint"><span class="gp-btn">B</span> {t('bs_back') ?? '返回'}</div>
      <div class="gp-hint"><span class="gp-btn">X</span> {t('bs_launch') ?? '启动'}</div>
      <div class="gp-hint"><span class="gp-btn">Y</span> {t('bs_pin') ?? '置顶'}</div>
      <div class="gp-hint"><span class="gp-btn">LB</span><span class="gp-btn">RB</span> {t('bs_switch_tab') ?? '切换'}</div>
    </div>
  {/if}

  <!-- Toast -->
  {#if toastMessage}
    <div class="bs-toast">{toastMessage}</div>
  {/if}

  <!-- 启动动画 -->
  {#if launchingGame}
    <div class="launch-overlay">
      <div class="launch-content">
        {#if getCoverUrl(launchingGame.cover)}
          <img src={getCoverUrl(launchingGame.cover)} alt="" class="launch-cover" />
        {/if}
        <h2>{t('bs_launching')}</h2>
        <p>{launchingGame.name}</p>
      </div>
    </div>
  {/if}

  <!-- 右键菜单 -->
  {#if ctxMenuOpen && ctxMenuGame}
    <div class="ctx-menu" style="left: {ctxMenuX}px; top: {ctxMenuY}px;">
      <div class="ctx-menu-header">
        <span class="ctx-menu-title">{ctxMenuGame.name}</span>
      </div>
      <div class="ctx-menu-divider"></div>
      <button class="ctx-menu-item" onclick={() => ctxAction('launch')}>
        <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5,3 19,12 5,21"/></svg>
        <span>{t('bs_play')}</span>
      </button>
      <button class="ctx-menu-item" onclick={() => ctxAction('detail')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
        <span>{t('bs_game_detail')}</span>
      </button>
      <div class="ctx-menu-divider"></div>
      <button class="ctx-menu-item" onclick={() => ctxAction('pin')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 17v5"/><path d="M9 2h6l-1 7h4l-5 6h-2l-5-6h4z"/></svg>
        <span>{ctxMenuGame.pinned ? t('bs_unpin') : t('bs_pin')}</span>
      </button>
      <button class="ctx-menu-item" onclick={() => ctxAction('favorite')}>
        <svg viewBox="0 0 24 24" fill={ctxMenuGame.favorite ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 1 0-7.78 7.78L12 21.23l8.84-8.84a5.5 5.5 0 0 0 0-7.78z"/></svg>
        <span>{ctxMenuGame.favorite ? t('bs_unfavorite') : t('bs_favorite')}</span>
      </button>
      <button class="ctx-menu-item" onclick={() => ctxAction('hide')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          {#if ctxMenuGame.hidden}
            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>
          {:else}
            <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/><path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/><line x1="1" y1="1" x2="23" y2="23"/>
          {/if}
        </svg>
        <span>{ctxMenuGame.hidden ? t('bs_show_game') : t('bs_hide_game')}</span>
      </button>
      <button class="ctx-menu-item" onclick={() => ctxAction('cover')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>
        <span>{t('bs_change_cover')}</span>
      </button>
      <div class="ctx-menu-divider"></div>
      <button class="ctx-menu-item ctx-menu-danger" onclick={() => ctxAction('delete')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
        <span>{t('bs_delete_game')}</span>
      </button>
    </div>
  {/if}

  <!-- 待机屏保 -->
  {#if screenSaverActive}
    <div class="screensaver" onclick={resetIdleTimer} role="presentation">
      <div class="ss-particles">
        {#each Array(30) as _, i}
          <div class="ss-particle" style="
            --x: {Math.random() * 100}%;
            --y: {Math.random() * 100}%;
            --size: {2 + Math.random() * 4}px;
            --duration: {8 + Math.random() * 12}s;
            --delay: {-Math.random() * 20}s;
            --drift: {-50 + Math.random() * 100}px;
            --opacity: {0.15 + Math.random() * 0.35};
          "></div>
        {/each}
      </div>
      <div class="ss-clock">
        <div class="ss-time">{screenSaverTime}</div>
        <div class="ss-date">{screenSaverDate}</div>
      </div>
      <div class="ss-hint">{t('bs_confirm') ?? '确认'}</div>
    </div>
  {/if}
</div>

<style>
  .bigscreen-container {
    position: fixed;
    inset: 0;
    z-index: 9999;
    background: #0a0a0f;
    color: #fff;
    font-family: system-ui, -apple-system, 'Segoe UI', sans-serif;
    overflow: hidden;
    opacity: 0;
    transition: opacity 0.6s cubic-bezier(0.16, 1, 0.3, 1);
    pointer-events: auto;
    -webkit-font-smoothing: antialiased;
  }
  .bigscreen-container.loaded { opacity: 1; }

  /* 背景 */
  .bg-layer { position: absolute; inset: 0; opacity: 0; transition: opacity 1s cubic-bezier(0.16, 1, 0.3, 1); filter: brightness(0.9); }
  .bg-layer.active { opacity: 1; }
  .bg-image { width: 100%; height: 100%; object-fit: cover; transform: scale(1.05); transition: transform 8s cubic-bezier(0.16, 1, 0.3, 1); }
  .bg-layer.active .bg-image { transform: scale(1.0); }
  .bg-placeholder { width: 100%; height: 100%; background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%); }
  .bg-overlay-top { position: absolute; inset: 0; background: linear-gradient(to bottom, rgba(0,0,0,0.6) 0%, transparent 40%); }
  .bg-overlay-bottom { position: absolute; inset: 0; background: linear-gradient(to top, #101010 0%, rgba(0,0,0,0.4) 40%, transparent 60%); }
  .bg-overlay-left { position: absolute; inset: 0; background: linear-gradient(to right, rgba(0,0,0,0.6) 0%, transparent 50%); }

  /* 导航 */
  .top-nav { position: absolute; top: 0; left: 0; right: 0; z-index: 50; display: flex; justify-content: space-between; align-items: flex-start; padding: 40px 48px; animation: slideDown 0.7s ease; }
  @keyframes slideDown { from { transform: translateY(-40px); opacity: 0; } to { transform: translateY(0); opacity: 1; } }
  .nav-tabs { display: flex; gap: 32px; }
  .nav-tab { background: none; border: none; color: rgba(255,255,255,0.5); font-size: 20px; font-weight: 500; padding-bottom: 4px; cursor: pointer; transition: all 0.3s; }
  .nav-tab.active { color: #fff; border-bottom: 2px solid #fff; font-weight: 700; }
  .nav-tab:hover { color: #fff; }
  .nav-right { display: flex; align-items: center; gap: 24px; }
  .nav-pill-btn { display: flex; align-items: center; gap: 8px; padding: 8px 12px; background: rgba(0,0,0,0.25); backdrop-filter: blur(20px) saturate(1.2); border: 1px solid rgba(255,255,255,0.08); border-radius: 9999px; cursor: pointer; transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1); }
  .nav-pill-btn:hover { background: rgba(255,255,255,0.12); border-color: rgba(255,255,255,0.2); transform: scale(1.04); }
  .nav-pill-btn svg { width: 18px; height: 18px; color: rgba(255,255,255,0.8); }
  .nav-status { display: flex; align-items: center; gap: 12px; margin-left: 8px; }
  .user-avatar { width: 32px; height: 32px; border-radius: 50%; background: linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%); border: 2px solid rgba(255,255,255,0.2); box-shadow: 0 2px 8px rgba(0,0,0,0.3); }

  /* 电池 */
  .battery-widget { display: flex; align-items: center; gap: 8px; padding: 6px 12px; background: rgba(0,0,0,0.2); backdrop-filter: blur(12px); border-radius: 9999px; border: 1px solid rgba(255,255,255,0.1); color: rgba(255,255,255,0.9); transition: all 0.3s; }
  .battery-widget:hover { background: rgba(255,255,255,0.1); }
  .battery-widget.ac { color: #60a5fa; }
  .battery-widget.charging { color: #4ade80; }
  .battery-widget.low { color: #ef4444; }
  .ac-plug { position: relative; display: flex; align-items: center; justify-content: center; }
  .ac-plug svg { width: 18px; height: 18px; filter: drop-shadow(0 0 8px rgba(96,165,250,0.6)); }
  .ac-dot { position: absolute; top: -4px; right: -4px; width: 6px; height: 6px; background: #93c5fd; border-radius: 50%; animation: pulse 2s infinite; box-shadow: 0 0 5px rgba(96,165,250,1); }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
  .battery-percent { font-size: 14px; font-weight: 600; font-family: ui-monospace, monospace; letter-spacing: -0.5px; }
  .battery-shell { position: relative; width: 26px; height: 14px; border-radius: 3px; border: 1.5px solid currentColor; opacity: 0.6; display: flex; align-items: center; padding: 1.5px; }
  .battery-tip { position: absolute; right: -4px; top: 50%; transform: translateY(-50%); width: 2px; height: 5px; border-radius: 0 1px 1px 0; background: currentColor; }
  .battery-fill { height: 100%; border-radius: 1px; background: currentColor; transition: width 0.5s ease-out; }
  .battery-widget.charging .battery-fill { box-shadow: 0 0 10px rgba(74,222,128,0.4); }
  .charging-bolt { position: absolute; top: -8px; right: -6px; width: 14px; height: 14px; background: #4ade80; border-radius: 50%; display: flex; align-items: center; justify-content: center; border: 1px solid #000; box-shadow: 0 2px 8px rgba(0,0,0,0.3); animation: bounce 2s infinite; }
  .charging-bolt svg { width: 10px; height: 10px; color: #000; }
  @keyframes bounce { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-2px); } }
  .user-time-group { display: flex; align-items: center; gap: 12px; margin-left: 8px; padding-left: 16px; border-left: 1px solid rgba(255,255,255,0.1); }
  .time-display { font-size: 20px; font-weight: 300; font-family: ui-monospace, monospace; color: #fff; text-shadow: 0 2px 10px rgba(0,0,0,0.5); }

  /* 主内容 */
  .main-content { position: relative; z-index: 10; height: 100%; display: flex; flex-direction: column; justify-content: space-between; padding: 112px 48px 48px; opacity: 0; transform: translateY(40px); transition: all 1s ease; }
  .main-content.loaded { opacity: 1; transform: translateY(0); }

  /* 游戏卡片横条 */
  .game-ribbon { display: flex; gap: 24px; padding: 16px 0 32px 8px; overflow-x: auto; overflow-y: visible; scrollbar-width: none; background: transparent; border: none; }
  .game-ribbon::-webkit-scrollbar { display: none; }
  .bigscreen-game-card { width: 192px; height: 300px; flex-shrink: 0; background: transparent; border: none; cursor: pointer; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; outline: none; overflow: visible; }
  .card-image-wrapper { width: 192px; height: 256px; border-radius: 16px; overflow: hidden; box-shadow: 0 8px 32px rgba(0,0,0,0.4); transition: transform 0.35s cubic-bezier(0.34, 1.56, 0.64, 1), opacity 0.3s ease-out, box-shadow 0.35s ease-out; transform: scale(0.85); opacity: 0.6; position: relative; }
  .bigscreen-game-card:hover .card-image-wrapper { opacity: 1; transform: scale(0.92); }
  .bigscreen-game-card.focused .card-image-wrapper { transform: scale(1); opacity: 1; box-shadow: 0 20px 60px rgba(0,0,0,0.5), 0 0 40px rgba(99,102,241,0.15), 0 0 0 3px rgba(255,255,255,0.9); outline: none; }
  .card-image { width: 100%; height: 100%; object-fit: cover; }
  .card-placeholder { width: 100%; height: 100%; background: linear-gradient(135deg, #2a2a4a 0%, #1a1a3e 100%); display: flex; align-items: center; justify-content: center; }
  .card-placeholder svg { width: 48px; height: 48px; color: rgba(255,255,255,0.3); }
  .card-title { font-size: 14px; font-weight: 500; color: #fff; text-shadow: 0 2px 8px rgba(0,0,0,0.8); opacity: 0; transition: opacity 0.3s; white-space: nowrap; max-width: 100%; overflow: hidden; text-overflow: ellipsis; }
  .bigscreen-game-card:not(.focused):hover .card-title { opacity: 1; }
  .pin-badge { position: absolute; top: 8px; right: 8px; font-size: 16px; filter: drop-shadow(0 2px 4px rgba(0,0,0,0.5)); }
  .library-card .card-image-wrapper { background: rgba(255,255,255,0.1); border: 2px dashed rgba(255,255,255,0.2); display: flex; align-items: center; justify-content: center; }
  .library-card:hover .card-image-wrapper { background: rgba(255,255,255,0.15); border-color: rgba(255,255,255,0.4); }
  .library-icon { display: flex; flex-direction: column; align-items: center; gap: 8px; color: rgba(255,255,255,0.6); }
  .library-icon svg { width: 32px; height: 32px; }
  .library-icon span { font-size: 14px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.1em; }

  /* Hero */
  .hero-content { max-width: 900px; margin-top: auto; animation: fadeSlideUp 0.7s cubic-bezier(0.16, 1, 0.3, 1); }
  @keyframes fadeSlideUp { from { opacity: 0; transform: translateY(20px); } to { opacity: 1; transform: translateY(0); } }
  .game-logo { font-size: clamp(32px, 5vw, 72px); font-weight: 900; text-transform: uppercase; font-style: italic; letter-spacing: -0.02em; line-height: 1; text-shadow: 0 4px 30px rgba(0,0,0,0.5); margin-bottom: 16px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  .game-meta { display: flex; align-items: center; gap: 16px; font-size: 14px; font-weight: 500; color: rgba(255,255,255,0.9); margin-bottom: 24px; }
  .meta-tag { text-transform: uppercase; letter-spacing: 0.1em; }
  .meta-dot { color: rgba(255,255,255,0.4); }
  .meta-badge { background: rgba(255,255,255,0.2); padding: 4px 8px; border-radius: 4px; font-size: 12px; backdrop-filter: blur(10px); }
  .action-row { display: flex; gap: 16px; align-items: center; }
  .play-btn { display: flex; align-items: center; gap: 12px; background: #fff; color: #000; border: none; padding: 16px 48px; border-radius: 9999px; font-size: 18px; font-weight: 700; cursor: pointer; transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1); box-shadow: 0 0 30px rgba(255,255,255,0.2); }
  .play-btn:hover { transform: scale(1.06); box-shadow: 0 0 50px rgba(255,255,255,0.3); }
  .play-btn svg { width: 20px; height: 20px; }
  .detail-btn { width: 56px; height: 56px; display: flex; align-items: center; justify-content: center; background: rgba(255,255,255,0.1); backdrop-filter: blur(10px); border: 1px solid rgba(255,255,255,0.2); border-radius: 50%; cursor: pointer; transition: all 0.3s; }
  .detail-btn:hover { background: rgba(255,255,255,0.2); transform: scale(1.05); }
  .detail-btn svg { width: 24px; height: 24px; color: #fff; }
  .meta-status { font-weight: 600; }
  .empty-state { text-align: center; }
  .empty-state h2 { font-size: 32px; font-weight: 300; margin-bottom: 12px; opacity: 0.6; }
  .empty-state p { font-size: 16px; opacity: 0.4; }
  .home-stats { display: flex; align-items: center; gap: 12px; padding: 12px 0; margin-top: 16px; font-size: 14px; color: rgba(255,255,255,0.5); font-weight: 400; }
  .stats-dot { color: rgba(255,255,255,0.2); }

  /* 资源库 */
  .library-view { position: fixed; inset: 0; z-index: 60; background: #0d0d14; display: flex; flex-direction: column; animation: slideFromRight 0.4s cubic-bezier(0.16, 1, 0.3, 1); }
  .library-bg {
    position: absolute; inset: 0; z-index: 0; overflow: hidden;
    transition: opacity 0.6s ease;
  }
  .library-bg img {
    width: 100%; height: 100%; object-fit: cover;
    filter: blur(60px) saturate(0.6) brightness(0.25);
    transform: scale(1.2);
  }
  .library-view .library-header,
  .library-view .filter-bar,
  .library-view .library-grid { position: relative; z-index: 1; }
  @keyframes slideFromRight { from { transform: translateX(100%); } to { transform: translateX(0); } }
  .library-header { display: flex; align-items: center; gap: 24px; padding: 40px 64px 16px; }
  .library-header h2 { font-size: 32px; font-weight: 300; }
  .library-count { font-size: 16px; color: rgba(255,255,255,0.3); font-weight: 400; margin-left: 8px; }
  .back-btn { width: 48px; height: 48px; background: rgba(255,255,255,0.05); border: none; border-radius: 50%; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.3s; flex-shrink: 0; }
  .back-btn:hover { background: rgba(255,255,255,0.1); }
  .back-btn svg { width: 24px; height: 24px; color: #fff; }

  /* 过滤栏 */
  .filter-bar { display: flex; flex-wrap: wrap; gap: 12px; padding: 8px 64px 16px; border-bottom: 1px solid rgba(255,255,255,0.05); }
  .filter-section { display: flex; align-items: center; gap: 8px; }
  .filter-pills { display: flex; gap: 6px; flex-wrap: wrap; }
  .filter-pill { padding: 6px 14px; background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.08); border-radius: 9999px; color: rgba(255,255,255,0.5); font-size: 13px; font-weight: 500; cursor: pointer; transition: all 0.2s; white-space: nowrap; }
  .filter-pill:hover { background: rgba(255,255,255,0.1); color: rgba(255,255,255,0.8); }
  .filter-pill.active { background: rgba(255,255,255,0.15); color: #fff; border-color: rgba(255,255,255,0.3); }
  .status-pill.active { background: var(--status-color); border-color: var(--status-color); color: #fff; }

  .library-grid { flex: 1; overflow-y: auto; padding: 24px 64px 64px; display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 24px 16px; }
  .library-item { display: flex; flex-direction: column; gap: 12px; cursor: pointer; transition: transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1); background: none; border: none; text-align: left; color: inherit; }
  .library-item:hover { transform: scale(1.03); }
  .library-item.focused { transform: scale(1.06); }
  .library-item.focused .library-cover { box-shadow: 0 20px 50px rgba(0,0,0,0.6), 0 0 30px rgba(99,102,241,0.2); outline: 3px solid rgba(255,255,255,0.9); }
  .library-cover { aspect-ratio: 2/3; border-radius: 12px; overflow: hidden; background: #181822; box-shadow: 0 8px 24px rgba(0,0,0,0.4); transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1); position: relative; }
  .library-item:hover .library-cover { box-shadow: 0 15px 40px rgba(0,0,0,0.6), 0 0 20px rgba(255,255,255,0.05); outline: 3px solid rgba(255,255,255,0.8); }
  .library-cover img { width: 100%; height: 100%; object-fit: cover; }
  .cover-placeholder { width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; background: linear-gradient(135deg, #2a2a4a 0%, #1a1a3e 100%); }
  .cover-placeholder svg { width: 32px; height: 32px; color: rgba(255,255,255,0.3); }
  .library-title { font-size: 14px; font-weight: 500; color: rgba(255,255,255,0.9); padding: 0 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  .library-playtime { font-size: 12px; color: rgba(255,255,255,0.3); padding: 0 4px; }
  .fav-badge { position: absolute; top: 8px; left: 8px; font-size: 14px; filter: drop-shadow(0 2px 4px rgba(0,0,0,0.5)); }
  .status-dot { position: absolute; bottom: 8px; right: 8px; width: 10px; height: 10px; border-radius: 50%; border: 2px solid rgba(0,0,0,0.5); box-shadow: 0 2px 4px rgba(0,0,0,0.3); }
  .y-hold-overlay { position: absolute; bottom: 0; left: 0; right: 0; height: 4px; background: rgba(0,0,0,0.5); }
  .y-hold-bar { height: 100%; background: #6366f1; transition: width 0.05s linear; }
  .add-game-cover { display: flex; align-items: center; justify-content: center; background: rgba(255,255,255,0.05); border: 2px dashed rgba(255,255,255,0.2); }
  .add-game-cover svg { width: 48px; height: 48px; color: rgba(255,255,255,0.4); transition: all 0.2s; }
  .add-game-item:hover .add-game-cover, .add-game-item.focused .add-game-cover { background: rgba(255,255,255,0.1); border-color: rgba(255,255,255,0.4); }
  .add-game-item:hover .add-game-cover svg, .add-game-item.focused .add-game-cover svg { color: rgba(255,255,255,0.8); transform: scale(1.1); }

  /* 游戏详情 */
  .detail-view { position: fixed; inset: 0; z-index: 70; background: #000; animation: detailEnter 0.6s cubic-bezier(0.16, 1, 0.3, 1); }
  .detail-bg { position: absolute; inset: 0; }
  .detail-bg-img { width: 100%; height: 100%; object-fit: cover; }
  .detail-bg-video { width: 100%; height: 100%; object-fit: cover; }
  .detail-bg-img.cover-bg { filter: blur(30px) brightness(0.4); transform: scale(1.2); }
  .detail-bg-gradient { width: 100%; height: 100%; background: linear-gradient(135deg, #1a1a2e 0%, #16213e 50%, #0f3460 100%); }
  .detail-bg-overlay { position: absolute; inset: 0; background: linear-gradient(to top, rgba(0,0,0,0.95) 0%, rgba(0,0,0,0.6) 40%, rgba(0,0,0,0.3) 100%); }
  .detail-content { position: relative; z-index: 10; height: 100%; display: flex; flex-direction: column; padding: 40px 64px; }
  .detail-back { position: absolute; top: 40px; left: 64px; z-index: 20; }
  .detail-info { flex: 1; display: flex; flex-direction: column; justify-content: flex-end; padding-bottom: 40px; max-width: 1200px; }
  .detail-layout { display: flex; gap: 48px; margin-bottom: 40px; align-items: flex-end; }
  .detail-cover-wrap { flex-shrink: 0; width: 200px; height: 300px; border-radius: 16px; overflow: hidden; box-shadow: 0 20px 60px rgba(0,0,0,0.6); animation: coverFloat 0.8s cubic-bezier(0.16, 1, 0.3, 1) 0.1s both; }
  .detail-cover-img { width: 100%; height: 100%; object-fit: cover; }
  .detail-cover-placeholder { width: 100%; height: 100%; background: linear-gradient(135deg, #2a2a4a, #1a1a3e); display: flex; align-items: center; justify-content: center; }
  .detail-cover-placeholder svg { width: 48px; height: 48px; color: rgba(255,255,255,0.3); }
  .detail-meta { flex: 1; animation: fadeSlideUp 0.7s cubic-bezier(0.16, 1, 0.3, 1) 0.15s both; }
  .detail-title { font-size: clamp(28px, 4vw, 56px); font-weight: 800; line-height: 1.1; margin-bottom: 16px; text-shadow: 0 4px 20px rgba(0,0,0,0.5); }
  .detail-tags { display: flex; gap: 10px; flex-wrap: wrap; margin-bottom: 16px; }
  .detail-tag { padding: 6px 14px; border-radius: 8px; font-size: 13px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; }
  .detail-tag.platform { background: rgba(255,255,255,0.1); color: rgba(255,255,255,0.8); }
  .detail-tag.status { border-radius: 8px; }
  .detail-tag.fav { background: rgba(239, 68, 68, 0.15); color: #f87171; }
  .detail-description { font-size: 14px; line-height: 1.6; color: rgba(255,255,255,0.5); margin-bottom: 20px; max-width: 600px; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .detail-stats-row { display: flex; gap: 48px; }
  .detail-stat { display: flex; flex-direction: column; gap: 4px; }
  .stat-label { font-size: 12px; color: rgba(255,255,255,0.4); text-transform: uppercase; letter-spacing: 0.1em; font-weight: 600; }
  .stat-value { font-size: 18px; font-weight: 500; color: rgba(255,255,255,0.9); }

  /* 详情操作按钮 */
  .detail-actions { display: flex; gap: 12px; flex-wrap: wrap; margin-bottom: 24px; animation: fadeSlideUp 0.7s cubic-bezier(0.16, 1, 0.3, 1) 0.25s both; }
  .action-btn { display: flex; align-items: center; gap: 10px; padding: 14px 24px; background: rgba(255,255,255,0.08); backdrop-filter: blur(10px); border: 2px solid rgba(255,255,255,0.1); border-radius: 16px; color: #fff; font-size: 15px; font-weight: 500; cursor: pointer; transition: all 0.25s; }
  .action-btn:hover { background: rgba(255,255,255,0.15); border-color: rgba(255,255,255,0.3); }
  .action-btn.focused { background: rgba(255,255,255,0.2); border-color: #6366f1; box-shadow: 0 0 20px rgba(99,102,241,0.3); }
  .action-btn.play { background: #fff; color: #000; border-color: #fff; padding: 14px 40px; font-weight: 700; font-size: 17px; }
  .action-btn.play:hover { transform: scale(1.04); box-shadow: 0 0 40px rgba(255,255,255,0.35); }
  .action-btn.play.focused { box-shadow: 0 0 40px rgba(255,255,255,0.5), 0 0 80px rgba(255,255,255,0.15); }
  .action-btn.danger { color: #f87171; border-color: rgba(239,68,68,0.2); }
  .action-btn.danger:hover { background: rgba(239,68,68,0.15); border-color: rgba(239,68,68,0.5); }
  .action-btn svg { width: 20px; height: 20px; }

  /* 状态选择器 */
  .status-picker { display: flex; gap: 8px; flex-wrap: wrap; padding: 16px 0; animation: fadeSlideUp 0.2s ease; }
  .status-option { display: flex; align-items: center; gap: 8px; padding: 10px 20px; background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); border-radius: 12px; color: rgba(255,255,255,0.7); font-size: 14px; cursor: pointer; transition: all 0.2s; }
  .status-option:hover { background: rgba(255,255,255,0.1); color: #fff; }
  .status-option.active { background: rgba(255,255,255,0.15); border-color: var(--dot-color); color: #fff; }
  .status-dot-sm { width: 8px; height: 8px; border-radius: 50%; background: var(--dot-color); flex-shrink: 0; }

  /* 删除确认 */
  .delete-confirm { padding: 20px 24px; background: rgba(239,68,68,0.1); border: 1px solid rgba(239,68,68,0.2); border-radius: 16px; animation: fadeSlideUp 0.2s ease; max-width: 400px; }
  .delete-confirm p { font-size: 16px; margin-bottom: 16px; color: rgba(255,255,255,0.8); }
  .confirm-actions { display: flex; gap: 12px; }
  .confirm-btn { padding: 10px 24px; border-radius: 10px; font-size: 15px; font-weight: 500; cursor: pointer; transition: all 0.2s; border: none; }
  .confirm-btn.cancel { background: rgba(255,255,255,0.1); color: #fff; }
  .confirm-btn.cancel:hover { background: rgba(255,255,255,0.2); }
  .confirm-btn.danger { background: #ef4444; color: #fff; }
  .confirm-btn.danger:hover { background: #dc2626; }

  /* 属性面板 */
  .properties-panel { max-width: 600px; max-height: 300px; overflow-y: auto; padding: 16px 0; animation: fadeSlideUp 0.3s cubic-bezier(0.16, 1, 0.3, 1); scrollbar-width: thin; scrollbar-color: rgba(255,255,255,0.1) transparent; }
  .prop-section { margin-bottom: 16px; }
  .prop-title { font-size: 12px; color: rgba(255,255,255,0.4); text-transform: uppercase; letter-spacing: 0.1em; font-weight: 600; margin-bottom: 8px; }
  .prop-item { display: flex; align-items: flex-start; gap: 12px; padding: 10px 16px; border-radius: 12px; width: 100%; background: rgba(255,255,255,0.04); border: 1px solid rgba(255,255,255,0.06); cursor: pointer; transition: all 0.2s; margin-bottom: 6px; color: inherit; text-align: left; }
  .prop-item:hover { background: rgba(255,255,255,0.08); border-color: rgba(255,255,255,0.12); }
  .prop-item.static { cursor: default; }
  .prop-item.static:hover { background: rgba(255,255,255,0.04); border-color: rgba(255,255,255,0.06); }
  .prop-item svg { width: 18px; height: 18px; flex-shrink: 0; margin-top: 2px; color: rgba(255,255,255,0.5); }
  .prop-text { flex: 1; min-width: 0; }
  .prop-label { display: block; font-size: 13px; font-weight: 500; color: rgba(255,255,255,0.7); margin-bottom: 2px; }
  .prop-value { display: block; font-size: 12px; color: rgba(255,255,255,0.35); word-break: break-all; font-family: ui-monospace, monospace; }

  /* 设置 - PS5风格双栏布局 */
  .settings-view {
    position: fixed; inset: 0; z-index: 60;
    background: #0d0d14;
    display: flex; flex-direction: row;
    animation: slideFromRight 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .settings-sidebar {
    width: 340px; flex-shrink: 0;
    display: flex; flex-direction: column;
    background: rgba(255,255,255,0.02);
    border-right: 1px solid rgba(255,255,255,0.06);
    padding: 48px 0 48px 0;
  }
  .settings-sidebar-header {
    display: flex; align-items: center; gap: 20px;
    padding: 0 32px 40px;
  }
  .settings-sidebar-header h2 {
    font-size: 28px; font-weight: 300; color: #fff;
  }
  .settings-nav {
    display: flex; flex-direction: column; gap: 4px;
    padding: 0 16px; flex: 1;
  }
  .settings-nav-item {
    display: flex; align-items: center; gap: 16px;
    padding: 16px 20px; border-radius: 14px;
    background: none; border: none;
    color: rgba(255,255,255,0.5); font-size: 16px; font-weight: 400;
    cursor: pointer; transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
    text-align: left; position: relative;
  }
  .settings-nav-item svg {
    width: 22px; height: 22px; flex-shrink: 0;
    transition: all 0.25s;
  }
  .settings-nav-item span {
    transition: all 0.25s;
  }
  .settings-nav-item:hover {
    background: rgba(255,255,255,0.04);
    color: rgba(255,255,255,0.8);
  }
  .settings-nav-item.active {
    background: rgba(255,255,255,0.08);
    color: #fff; font-weight: 500;
  }
  .settings-nav-item.active::before {
    content: ''; position: absolute; left: 0; top: 50%;
    transform: translateY(-50%);
    width: 4px; height: 24px; border-radius: 2px;
    background: #6366f1;
  }
  .settings-nav-item.active svg { color: #818cf8; }

  .settings-content {
    flex: 1; overflow-y: auto;
    padding: 48px 64px 64px;
    animation: settingsFadeIn 0.3s ease;
  }
  @keyframes settingsFadeIn { from { opacity: 0; transform: translateX(12px); } to { opacity: 1; transform: translateX(0); } }
  .settings-panel {
    max-width: 720px;
  }
  .settings-item {
    width: 100%; display: flex; align-items: center; gap: 24px;
    padding: 20px 24px; background: rgba(255,255,255,0.03);
    border: 1px solid rgba(255,255,255,0.04); border-radius: 16px;
    cursor: pointer; transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1);
    text-align: left; color: inherit; margin-bottom: 8px;
  }
  .settings-item:hover { background: rgba(255,255,255,0.06); border-color: rgba(255,255,255,0.08); transform: translateX(4px); }
  .settings-item:disabled { opacity: 0.5; cursor: not-allowed; }
  .settings-icon {
    width: 48px; height: 48px;
    background: rgba(255,255,255,0.06); border-radius: 14px;
    display: flex; align-items: center; justify-content: center;
    transition: all 0.3s; flex-shrink: 0;
  }
  .settings-item:hover .settings-icon { background: rgba(255,255,255,0.1); transform: scale(1.05); }
  .settings-icon svg { width: 24px; height: 24px; color: rgba(255,255,255,0.7); }
  .settings-text { flex: 1; }
  .settings-text h3 { font-size: 18px; font-weight: 500; margin-bottom: 4px; color: rgba(255,255,255,0.9); }
  .settings-text p { font-size: 13px; color: rgba(255,255,255,0.35); }
  .settings-arrow { width: 20px; height: 20px; color: rgba(255,255,255,0.15); transition: all 0.3s; flex-shrink: 0; }
  .settings-item:hover .settings-arrow { color: rgba(255,255,255,0.5); transform: translateX(4px); }

  /* 统计仪表板 */
  .stats-dashboard {
    display: grid; grid-template-columns: repeat(5, 1fr); gap: 14px;
    margin-bottom: 32px;
  }
  .stat-card {
    background: rgba(255, 255, 255, 0.04); border-radius: 16px;
    padding: 24px 16px; text-align: center;
    border: 1px solid rgba(255, 255, 255, 0.05);
    transition: all 0.3s;
  }
  .stat-card:hover { background: rgba(255, 255, 255, 0.06); transform: translateY(-2px); }
  .stat-value { display: block; font-size: 36px; font-weight: 600; color: #fff; line-height: 1.2; }
  .stat-label { display: block; font-size: 12px; color: rgba(255, 255, 255, 0.4); margin-top: 8px; letter-spacing: 0.5px; }
  .stat-card.accent .stat-value { color: #818cf8; }
  .stat-card.success .stat-value { color: #34d399; }

  /* 最常游玩 */
  .most-played, .platform-breakdown { margin-bottom: 32px; }
  .most-played h4, .platform-breakdown h4 { font-size: 13px; font-weight: 500; color: rgba(255, 255, 255, 0.4); margin-bottom: 16px; text-transform: uppercase; letter-spacing: 0.5px; }
  .mp-item {
    display: grid; grid-template-columns: 44px 1fr auto 120px;
    align-items: center; gap: 14px; padding: 10px 0;
  }
  .mp-cover { width: 44px; height: 44px; border-radius: 10px; overflow: hidden; background: rgba(255,255,255,0.05); }
  .mp-cover img { width: 100%; height: 100%; object-fit: cover; }
  .mp-name { font-size: 15px; font-weight: 500; color: rgba(255, 255, 255, 0.8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mp-time { font-size: 13px; color: rgba(255, 255, 255, 0.4); white-space: nowrap; }
  .mp-bar { height: 4px; background: rgba(255, 255, 255, 0.06); border-radius: 2px; overflow: hidden; }
  .mp-fill { height: 100%; background: linear-gradient(90deg, #6366f1, #818cf8); border-radius: 2px; transition: width 0.6s ease; }

  /* 平台分布 */
  .platform-bars { display: flex; flex-direction: column; gap: 10px; }
  .pb-item { display: grid; grid-template-columns: 90px 1fr 40px; align-items: center; gap: 14px; }
  .pb-label { font-size: 14px; color: rgba(255, 255, 255, 0.6); text-transform: capitalize; }
  .pb-bar { height: 6px; background: rgba(255, 255, 255, 0.06); border-radius: 3px; overflow: hidden; }
  .pb-fill { height: 100%; background: linear-gradient(90deg, rgba(99, 102, 241, 0.6), rgba(99, 102, 241, 0.9)); border-radius: 3px; transition: width 0.6s ease; }
  .pb-count { font-size: 14px; color: rgba(255, 255, 255, 0.5); text-align: right; }

  /* 控制中心 - PS5风格 */
  .control-center { position: absolute; bottom: 0; left: 0; right: 0; z-index: 100; background: rgba(18,18,28,0.95); backdrop-filter: blur(60px) saturate(1.4); border-top-left-radius: 32px; border-top-right-radius: 32px; padding: 32px 48px 48px; transform: translateY(100%); opacity: 0; transition: all 0.5s cubic-bezier(0.16, 1, 0.3, 1); box-shadow: 0 -20px 80px rgba(0,0,0,0.6); border-top: 1px solid rgba(255,255,255,0.08); }
  .control-center.open { transform: translateY(0); opacity: 1; }
  .cc-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 28px; max-width: 1200px; margin-left: auto; margin-right: auto; }
  .cc-header h3 { font-size: 20px; font-weight: 500; letter-spacing: 0.05em; color: rgba(255,255,255,0.9); }
  .cc-close { width: 36px; height: 36px; background: rgba(255,255,255,0.08); border: none; border-radius: 50%; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.3s; }
  .cc-close:hover { background: rgba(255,255,255,0.15); }
  .cc-close svg { width: 20px; height: 20px; color: #fff; }
  .cc-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; max-width: 1200px; margin: 0 auto; }
  .cc-card { background: rgba(255,255,255,0.04); border: 1px solid rgba(255,255,255,0.06); border-radius: 20px; padding: 20px; display: flex; flex-direction: column; align-items: center; gap: 10px; cursor: pointer; transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1); text-align: center; position: relative; overflow: hidden; }
  .cc-card::before { content: ''; position: absolute; inset: 0; border-radius: 20px; opacity: 0; transition: opacity 0.3s; background: radial-gradient(ellipse at 50% 0%, rgba(99,102,241,0.15), transparent 70%); }
  .cc-card:hover { background: rgba(255,255,255,0.08); transform: translateY(-2px); box-shadow: 0 8px 30px rgba(0,0,0,0.3); }
  .cc-card:hover::before { opacity: 1; }
  .cc-card.cc-active { background: rgba(99,102,241,0.12); border-color: rgba(99,102,241,0.3); }
  .cc-card.cc-active .cc-icon { color: #818cf8; }
  .cc-card.cc-active .cc-status { color: #818cf8; }
  .cc-card.cc-danger { background: rgba(239,68,68,0.06); border-color: rgba(239,68,68,0.15); }
  .cc-card.cc-danger:hover { background: rgba(239,68,68,0.15); }
  .cc-card.cc-danger .cc-icon { color: rgba(239,68,68,0.8); }
  .cc-card.cc-danger .cc-label { color: rgba(239,68,68,0.7); }
  .cc-card.cc-charging .cc-icon { color: #22c55e; }
  .cc-card.cc-charging .cc-status { color: #22c55e; }
  .cc-icon { width: 32px; height: 32px; color: rgba(255,255,255,0.7); transition: color 0.3s; }
  .cc-icon svg { width: 100%; height: 100%; }
  .cc-label { font-size: 12px; color: rgba(255,255,255,0.5); font-weight: 500; letter-spacing: 0.04em; }
  .cc-status { font-size: 13px; color: rgba(255,255,255,0.8); font-weight: 600; }

  /* 浮动按钮 */
  .float-btn-container { position: absolute; bottom: 32px; right: 32px; z-index: 50; }
  .float-btn { width: 64px; height: 64px; background: rgba(255,255,255,0.08); backdrop-filter: blur(20px) saturate(1.5); border: 1px solid rgba(255,255,255,0.15); border-radius: 50%; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1); box-shadow: 0 4px 30px rgba(0,0,0,0.3); }
  .float-btn:hover { background: rgba(255,255,255,0.15); transform: scale(1.12); border-color: rgba(255,255,255,0.4); box-shadow: 0 0 30px rgba(99,102,241,0.2); }
  .float-btn svg { width: 28px; height: 28px; color: #fff; }

  /* 启动动画 */
  .launch-overlay { position: fixed; inset: 0; z-index: 200; background: #000; display: flex; align-items: center; justify-content: center; animation: fadeIn 0.3s ease; }
  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
  .launch-content { text-align: center; }
  .launch-cover { width: 128px; height: 192px; border-radius: 12px; box-shadow: 0 8px 32px rgba(0,0,0,0.5); margin-bottom: 32px; animation: launchPulse 1.5s infinite; }
  @keyframes launchPulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.7; } }
  .launch-content h2 { font-size: 24px; font-weight: 300; letter-spacing: 0.2em; text-transform: uppercase; margin-bottom: 8px; }
  .launch-content p { font-size: 14px; color: rgba(255,255,255,0.4); }

  /* 搜索 */
  .search-view { position: fixed; inset: 0; z-index: 60; background: #0d0d14; display: flex; flex-direction: column; animation: slideFromRight 0.4s cubic-bezier(0.16, 1, 0.3, 1); }
  .search-header { padding: 80px 64px 32px; border-bottom: 1px solid rgba(255,255,255,0.05); }
  .search-input-wrapper { display: flex; align-items: center; gap: 16px; background: rgba(255,255,255,0.05); border-radius: 16px; padding: 16px 24px; border: 1px solid rgba(255,255,255,0.06); transition: border-color 0.3s; }
  .search-input-wrapper:focus-within { border-color: rgba(99,102,241,0.4); }
  .search-icon { width: 24px; height: 24px; color: rgba(255,255,255,0.4); flex-shrink: 0; }
  .search-input { flex: 1; background: none; border: none; outline: none; color: #fff; font-size: 20px; }
  .search-input::placeholder { color: rgba(255,255,255,0.3); }
  .search-cancel { background: none; border: none; color: rgba(255,255,255,0.6); font-size: 16px; cursor: pointer; padding: 8px 16px; }
  .search-cancel:hover { color: #fff; }
  .search-results { flex: 1; overflow-y: auto; padding: 32px 64px; }
  .search-count { font-size: 14px; color: rgba(255,255,255,0.5); margin-bottom: 24px; }
  .search-hint { text-align: center; color: rgba(255,255,255,0.3); font-size: 18px; margin-top: 100px; }
  .search-grid { display: flex; flex-direction: column; gap: 8px; }
  .search-item { display: flex; align-items: center; gap: 20px; padding: 12px 16px; border-radius: 12px; cursor: pointer; transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1); }
  .search-item:hover { background: rgba(255,255,255,0.06); transform: translateX(4px); }
  .search-cover { width: 64px; height: 64px; border-radius: 8px; overflow: hidden; flex-shrink: 0; background: #202020; }
  .search-cover img { width: 100%; height: 100%; object-fit: cover; }
  .search-info { display: flex; flex-direction: column; gap: 4px; }
  .search-title { font-size: 18px; font-weight: 500; }
  .search-source { font-size: 13px; color: rgba(255,255,255,0.4); text-transform: capitalize; }

  /* Toast */
  .bs-toast { position: fixed; bottom: 120px; left: 50%; transform: translateX(-50%); background: rgba(15,15,25,0.88); backdrop-filter: blur(30px) saturate(1.3); color: #fff; padding: 14px 36px; border-radius: 9999px; font-size: 16px; font-weight: 500; z-index: 300; animation: toastIn 0.4s cubic-bezier(0.16, 1, 0.3, 1); border: 1px solid rgba(255,255,255,0.08); box-shadow: 0 8px 40px rgba(0,0,0,0.4); }
  @keyframes toastIn { from { opacity: 0; transform: translateX(-50%) translateY(20px) scale(0.95); } to { opacity: 1; transform: translateX(-50%) translateY(0) scale(1); } }

  /* PS5 追加动画 */
  @keyframes detailEnter { from { opacity: 0; transform: scale(1.02); } to { opacity: 1; transform: scale(1); } }
  @keyframes coverFloat { from { opacity: 0; transform: translateY(30px); } to { opacity: 1; transform: translateY(0); } }

  /* 手柄指示器 */
  .gamepad-indicator {
    display: flex; align-items: center; justify-content: center;
    width: 32px; height: 32px; border-radius: 50%;
    background: rgba(99, 102, 241, 0.2); color: #818cf8;
    animation: gpPulse 2s ease infinite;
  }
  .gamepad-indicator svg { width: 18px; height: 18px; }

  @keyframes gpPulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(99, 102, 241, 0.3); }
    50% { box-shadow: 0 0 0 6px rgba(99, 102, 241, 0); }
  }

  /* 手柄按键提示 */
  .gamepad-hints {
    position: fixed; bottom: 24px; left: 50%; transform: translateX(-50%);
    display: flex; gap: 20px; z-index: 200;
    background: rgba(10, 10, 20, 0.75); backdrop-filter: blur(20px) saturate(1.3);
    padding: 10px 28px; border-radius: 9999px;
    border: 1px solid rgba(255, 255, 255, 0.06);
    animation: fadeSlideUp 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .gp-hint {
    display: flex; align-items: center; gap: 6px;
    color: rgba(255, 255, 255, 0.6); font-size: 13px; font-weight: 500;
    white-space: nowrap;
  }
  .gp-btn {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 24px; height: 24px; padding: 0 6px;
    background: rgba(255, 255, 255, 0.1); border-radius: 6px;
    color: rgba(255, 255, 255, 0.85); font-size: 11px; font-weight: 700;
    letter-spacing: 0.5px;
    border: 1px solid rgba(255, 255, 255, 0.12);
  }

  /* 待机屏保 */
  .screensaver {
    position: fixed; inset: 0; z-index: 9999;
    background: #040408; cursor: none;
    display: flex; align-items: center; justify-content: center;
    animation: ssIn 1.2s ease;
  }
  @keyframes ssIn { from { opacity: 0; } to { opacity: 1; } }

  .ss-particles { position: absolute; inset: 0; overflow: hidden; }
  .ss-particle {
    position: absolute;
    left: var(--x); top: var(--y);
    width: var(--size); height: var(--size);
    border-radius: 50%;
    background: radial-gradient(circle, rgba(99, 102, 241, var(--opacity)), transparent 70%);
    animation: ssFloat var(--duration) ease-in-out var(--delay) infinite;
  }
  @keyframes ssFloat {
    0%, 100% { transform: translate(0, 0) scale(1); opacity: var(--opacity); }
    25% { transform: translate(var(--drift), -40px) scale(1.3); opacity: calc(var(--opacity) * 1.5); }
    50% { transform: translate(calc(var(--drift) * -0.5), -80px) scale(0.8); opacity: var(--opacity); }
    75% { transform: translate(calc(var(--drift) * 0.7), -30px) scale(1.1); opacity: calc(var(--opacity) * 0.7); }
  }

  .ss-clock {
    position: relative; z-index: 2; text-align: center;
    animation: ssClockIn 1.5s cubic-bezier(0.16, 1, 0.3, 1) 0.3s both;
  }
  @keyframes ssClockIn { from { opacity: 0; transform: translateY(30px) scale(0.95); } to { opacity: 1; transform: translateY(0) scale(1); } }

  .ss-time {
    font-size: 120px; font-weight: 200; letter-spacing: -4px;
    color: rgba(255, 255, 255, 0.9);
    text-shadow: 0 0 60px rgba(99, 102, 241, 0.15);
    line-height: 1;
  }
  .ss-date {
    font-size: 20px; font-weight: 300; letter-spacing: 2px;
    color: rgba(255, 255, 255, 0.35); margin-top: 12px;
    text-transform: uppercase;
  }
  .ss-hint {
    position: absolute; bottom: 48px; left: 50%; transform: translateX(-50%);
    font-size: 14px; color: rgba(255, 255, 255, 0.2); letter-spacing: 1px;
    animation: ssPulse 3s ease infinite;
  }
  @keyframes ssPulse { 0%, 100% { opacity: 0.2; } 50% { opacity: 0.5; } }

  /* Sponsor panel */
  .sponsor-panel {
    padding: 16px 20px;
  }
  .sponsor-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 20px;
  }
  .sponsor-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 20px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 16px;
    border: 1px solid rgba(255, 255, 255, 0.06);
  }
  .sponsor-card h4 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.8);
  }
  .sponsor-qr {
    width: 160px;
    height: 160px;
    border-radius: 12px;
    object-fit: contain;
    background: #fff;
    padding: 4px;
  }
  .paypal-link {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 160px;
    height: 160px;
    background: rgba(0, 112, 186, 0.15);
    border-radius: 12px;
    color: #60a5fa;
    font-size: 14px;
    font-weight: 600;
    text-decoration: none;
    transition: background 0.2s;
  }
  .paypal-link:hover {
    background: rgba(0, 112, 186, 0.25);
  }

  /* 右键菜单 */
  .ctx-menu {
    position: fixed;
    z-index: 10000;
    min-width: 220px;
    max-width: 280px;
    background: rgba(18, 18, 28, 0.92);
    backdrop-filter: blur(40px) saturate(1.4);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 16px;
    padding: 6px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.05);
    animation: ctx-menu-in 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  }
  @keyframes ctx-menu-in {
    from { opacity: 0; transform: scale(0.92) translateY(-4px); }
    to { opacity: 1; transform: scale(1) translateY(0); }
  }
  .ctx-menu-header {
    padding: 10px 14px 6px;
  }
  .ctx-menu-title {
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.5);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }
  .ctx-menu-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 4px 8px;
  }
  .ctx-menu-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 14px;
    background: none;
    border: none;
    border-radius: 10px;
    color: rgba(255, 255, 255, 0.85);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
    text-align: left;
  }
  .ctx-menu-item:hover {
    background: rgba(99, 102, 241, 0.2);
    color: #fff;
  }
  .ctx-menu-item svg {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
    opacity: 0.7;
  }
  .ctx-menu-item:hover svg {
    opacity: 1;
  }
  .ctx-menu-danger {
    color: rgba(239, 68, 68, 0.85);
  }
  .ctx-menu-danger:hover {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
  }
  .ctx-menu-danger svg {
    color: rgba(239, 68, 68, 0.7);
  }
  .ctx-menu-danger:hover svg {
    color: #ef4444;
    opacity: 1;
  }
</style>
