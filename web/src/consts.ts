import {TracingKind, TracingLevel} from "./openapi";
import {HumanizerOptions} from "humanize-duration";

export const NULL_UUID = "00000000-0000-0000-0000-000000000000";

export const NULL_STR = 'NULL';

export const BASE_URL = import.meta.env.dev ? "https://127.0.0.1" : "";

export const LevelColors = {
  [TracingLevel.Trace]: '#A0A0A0',
  [TracingLevel.Debug]: '#4B8EFB',
  [TracingLevel.Info]: '#00BFFF',
  [TracingLevel.Warn]: '#FFD700',
  [TracingLevel.Error]: '#FF4500',
};

export const durationOptions: HumanizerOptions = {
  largest: 3,
  maxDecimalPoints: 0,
  language: "shortEn",
  languages: {
    shortEn: {
      y: () => "year",
      mo: () => "month",
      w: () => "week",
      d: () => "d",
      h: () => "h",
      m: () => "m",
      s: () => "s",
      ms: () => "ms",
    },
  },
  // timeAdverb: true,
  delimiter: " ",
  spacer: "",
};

// (counter) => `year${counter === 1 ? "" : "s"}`,
//   (counter) => `month${counter === 1 ? "" : "s"}`,
//   (counter) => `week${counter === 1 ? "" : "s"}`,
//   (counter) => `day${counter === 1 ? "" : "s"}`,
//   () => `h`,
//   () => `m`,
//   () => `s`,
//   () => `ms`,
//   "%s",
//   "%s ago",


export function getLevelColor(level?: TracingLevel | null): string {
  return level == null ? 'transparent' : LevelColors[level]
}

export const ALL_LEVELS = [
  TracingLevel.Trace,
  TracingLevel.Debug,
  TracingLevel.Info,
  TracingLevel.Warn,
  TracingLevel.Error,
];
export const TREE_KINDS = [
  "SPAN_CREATE",
  "SPAN_ENTER",
  "SPAN_LEAVE",
  "SPAN_CLOSE",
  "SPAN_RECORD",
  "EVENT",
]

export const KINDS = [
  ...TREE_KINDS,
  "APP_START",
  "APP_STOP",
]

export const RECORD_FIELDS = {
  flags: '__data.flags',
  empty_children: '__data.empty_children',
  related_name: '__data.related_name',
}

export const EXPANDABLE_KINDS:TracingKind[] = [TracingKind.SpanCreate, TracingKind.AppStart];