export type CompletionStatus = 'not_played' | 'playing' | 'completed' | 'beaten' | 'on_hold' | 'dropped' | 'backlog' | 'wishlist' | 'up_next' | '100_percent';

export type SortMode = 'name' | 'recent' | 'most_played' | 'added' | 'platform' | 'rating' | 'release' | 'play_count';
export type GroupMode = 'none' | 'platform' | 'genre' | 'developer' | 'series' | 'status' | 'completion';
export type FilterPlatform = 'all' | 'steam' | 'epic' | 'ea' | 'ubisoft' | 'xbox' | 'gog' | 'battlenet' | 'manual';
export type FilterStatus = 'all' | CompletionStatus;

export interface GameAction {
  name: string;
  path: string;
  arguments?: string;
  workingDir?: string;
  isDefault: boolean;
}

export interface GameLink {
  name: string;
  url: string;
}

export interface GameSession {
  startedAt: number;
  endedAt: number;
  durationMinutes: number;
}

export interface FilterPreset {
  id: string;
  name: string;
  platform: FilterPlatform;
  status: FilterStatus;
  tags?: string[];
  categories?: string[];
  sortMode: SortMode;
  showHidden?: boolean;
}

export interface Game {
  id: string;
  name: string;
  path?: string;
  steamAppId?: string;
  source?: string;
  cover?: string;
  backgroundImage?: string;
  icon?: string;
  logo?: string;
  addedAt?: number;
  lastPlayedAt?: number;
  installLocation?: string;
  installSize?: number;
  pinned?: boolean;
  completionStatus?: CompletionStatus;
  totalPlayTime?: number;
  playCount?: number;
  description?: string;
  genre?: string;
  releaseYear?: number;
  releaseDate?: string;
  favorite?: boolean;
  hidden?: boolean;
  developers?: string[];
  publishers?: string[];
  tags?: string[];
  categories?: string[];
  features?: string[];
  series?: string;
  ageRating?: string;
  criticScore?: number;
  communityScore?: number;
  notes?: string;
  links?: GameLink[];
  gameActions?: GameAction[];
  preScript?: string;
  postScript?: string;
  platform?: string;
  sessions?: GameSession[];
  userRating?: number;
  purchasePrice?: number;
  purchaseDate?: string;
  purchaseStore?: string;
}
