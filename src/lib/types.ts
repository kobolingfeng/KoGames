export type CompletionStatus = 'not_played' | 'playing' | 'completed' | 'on_hold' | 'dropped' | 'backlog';

export type SortMode = 'name' | 'recent' | 'most_played' | 'added' | 'platform';
export type FilterPlatform = 'all' | 'steam' | 'epic' | 'ea' | 'ubisoft' | 'xbox' | 'gog' | 'manual';
export type FilterStatus = 'all' | CompletionStatus;

export interface Game {
  id: string;
  name: string;
  path?: string;
  steamAppId?: string;
  source?: string;
  cover?: string;
  addedAt?: number;
  lastPlayedAt?: number;
  installLocation?: string;
  pinned?: boolean;
  completionStatus?: CompletionStatus;
  totalPlayTime?: number;  // minutes
  description?: string;
  genre?: string;
  releaseYear?: number;
  favorite?: boolean;
}
