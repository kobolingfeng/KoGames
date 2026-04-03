<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import BigScreen from './components/BigScreen.svelte';
  import DesktopMode from './components/DesktopMode.svelte';
  import { t } from './lib/i18n/index';
  import type { Game } from './lib/types';

  let games = $state<Game[]>([]);
  let appMode = $state<'bigscreen' | 'desktop'>('bigscreen');

  async function loadGames() {
    try {
      games = await invoke<Game[]>('get_games');
    } catch (e) {
      console.error('Failed to load games:', e);
      games = [];
    }
  }

  async function saveGames(updated: Game[]) {
    try {
      await invoke('save_games', { games: updated });
      games = updated;
    } catch (e) {
      console.error('Failed to save games:', e);
    }
  }

  async function handleLaunchGame(game: Game) {
    try {
      await invoke('launch_game', {
        gameId: game.id,
        gamePath: game.path ?? null,
        steamAppId: game.steamAppId ?? null,
      });
    } catch (e) {
      console.error('Failed to launch game:', e);
    }
  }

  async function handleAddGame() {
    try {
      const newGame = await invoke<Game | null>('select_game_exe');
      if (newGame) {
        await loadGames();
      }
    } catch (e) {
      console.error('Failed to add game:', e);
    }
  }

  function handleTogglePin(game: Game) {
    const updated = games.map(g => {
      if (g.id === game.id) {
        return { ...g, pinned: !g.pinned };
      }
      return g;
    });
    saveGames(updated);
  }

  async function handleExit() {
    try {
      const { exit } = await import('@tauri-apps/plugin-process');
      await exit(0);
    } catch {
      window.close();
    }
  }

  function switchMode() {
    appMode = appMode === 'bigscreen' ? 'desktop' : 'bigscreen';
  }

  onMount(() => {
    loadGames();
  });
</script>

{#if appMode === 'bigscreen'}
  <BigScreen
    {games}
    onExit={handleExit}
    onLaunchGame={handleLaunchGame}
    onAddGame={handleAddGame}
    onTogglePin={handleTogglePin}
    onGamesChanged={loadGames}
    onSwitchMode={switchMode}
    {t}
  />
{:else}
  <DesktopMode
    {games}
    onLaunchGame={handleLaunchGame}
    onAddGame={handleAddGame}
    onGamesChanged={loadGames}
    onSwitchMode={switchMode}
    {t}
  />
{/if}
