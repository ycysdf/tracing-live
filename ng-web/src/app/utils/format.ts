export function formatDuration(ms: number): string {
  if (ms < 1000) return ms + 'ms';
  const s = Math.floor(ms / 1000);
  if (s < 60) return s + 's';
  const m = Math.floor(s / 60);
  if (m < 60) return m + 'm ' + (s % 60) + 's';
  const h = Math.floor(m / 60);
  return h + 'h ' + (m % 60) + 'm';
}

export function formatDate(date: Date | string | undefined, nullStr = ''): string {
  if (!date) return nullStr;
  return new Date(date).toLocaleString();
}

export function formatValue(value: unknown, nullStr = 'NULL'): string {
  if (value == null) return nullStr;
  if (typeof value === 'string') {
    return value.length > 8192 ? value.slice(0, 8192) + '< too long ! >...' : value;
  }
  return JSON.stringify(value, null, 2).slice(0, 8192);
}
