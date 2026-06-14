import { TracingKind, TracingLevel } from '../../api';

export const NULL_UUID = '00000000-0000-0000-0000-000000000000';
export const NULL_STR = 'NULL';

export const BASE_URL = '';

export const LevelColors: Record<TracingLevel, string> = {
  [TracingLevel.Trace]: '#A0A0A0',
  [TracingLevel.Debug]: '#4B8EFB',
  [TracingLevel.Info]: '#00BFFF',
  [TracingLevel.Warn]: '#FFD700',
  [TracingLevel.Error]: '#FF4500',
};

export const durationOptions = {
  largest: 3,
  maxDecimalPoints: 0,
  language: 'shortEn',
  languages: {
    shortEn: {
      y: () => 'year',
      mo: () => 'month',
      w: () => 'week',
      d: () => 'd',
      h: () => 'h',
      m: () => 'm',
      s: () => 's',
      ms: () => 'ms',
    },
  },
  delimiter: ' ',
  spacer: '',
};

export function getLevelColor(level?: TracingLevel | null): string {
  return level == null ? 'transparent' : LevelColors[level];
}

export const ALL_LEVELS: TracingLevel[] = [
  TracingLevel.Trace,
  TracingLevel.Debug,
  TracingLevel.Info,
  TracingLevel.Warn,
  TracingLevel.Error,
];

export const TREE_KINDS: TracingKind[] = [
  TracingKind.SpanCreate,
  TracingKind.SpanEnter,
  TracingKind.SpanLeave,
  TracingKind.SpanClose,
  TracingKind.SpanRecord,
  TracingKind.Event,
];

export const KINDS: TracingKind[] = [
  ...TREE_KINDS,
  TracingKind.AppStart,
  TracingKind.AppStop,
];

export const RECORD_FIELDS = {
  flags: '__data.flags',
  empty_children: '__data.empty_children',
  related_name: '__data.related_name',
};

export const EXPANDABLE_KINDS: TracingKind[] = [TracingKind.SpanCreate, TracingKind.AppStart];

export const AUTO_EXPAND = 1 << 1;
export const FORK = 1 << 2;
