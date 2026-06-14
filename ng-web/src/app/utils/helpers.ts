import { RECORD_META_KEYS } from './constants';
import type { TracingRecordDto } from '../../api';

export function getFlags(record: TracingRecordDto, flag: number): boolean {
  const flags: number = (record.fields[RECORD_META_KEYS.flags] as number) ?? 0;
  return !!(flags & flag);
}

/** Extracts the related-event name from a record's fields (or `null`). */
export function getRelatedName(fields: Record<string, unknown>): string | null {
  return (fields[RECORD_META_KEYS.relatedName] as string) ?? null;
}

export function ellipseStr(value: string): string {
  if (value.length > 1024 * 8) {
    return value.slice(0, 1024 * 8) + '< too long ! >...';
  }
  return value;
}

export function handlePropValue(value: unknown): string {
  const raw = typeof value === 'string' ? ellipseStr(value) : ellipseStr(JSON.stringify(value, null, 2) ?? 'NULL');
  return raw;
}
