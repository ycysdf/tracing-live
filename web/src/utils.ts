import {Accessor, createSignal, startTransition} from "solid-js";
import {RECORD_FIELDS} from "~/consts";
import {TracingRecordDto} from "~/openapi";

export function useIsolateTransition(): [Accessor<boolean>, (fn: () => void) => Promise<void>] {
  let [pending, setPending] = createSignal<boolean>();
  return [
    pending,
    async f => {
      setPending(true);
      await startTransition(f);
      setPending(false);
    }
  ]
}


export interface MultiSelection<T> {
  isSelect(_n: T): boolean;

  select(_n: T): void;

  toggle(_n: T): void;

  toggleSingle(_n: T): void;

  clear(): void;

  setSelected: SetValue<T[]>;
  selected: Accessor<T[]>;
}

type SetValue<T> = (_a: T) => void;

export function createMultiSelection<T>(selected: Accessor<T[]>, setSelected: SetValue<T[]>): MultiSelection<T> {
  return {
    isSelect: (n: T): boolean => {
      return selected().includes(n)
    },
    select: (n: T) => {
      if (!(selected().includes(n))) {
        setSelected([n]);
      }
    },
    toggle: (n: T) => {
      let curSelected = selected();
      if (!(curSelected.includes(n))) {
        setSelected([n]);
      } else {
        setSelected(curSelected.filter(a => a != n))
      }
    },
    toggleSingle: (n: T) => {
      let curSelected = selected();
      if (!(curSelected.includes(n))) {
        setSelected([...curSelected, n]);
      } else {
        setSelected(curSelected.filter(a => a != n))
      }
    },
    setSelected,
    selected,
    clear() {
      setSelected([]);
    }
  }
}

export const AUTO_EXPAND = 1 << 1;
export const FORK = 1 << 2;

export function getFlags(record: TracingRecordDto, flag: number): boolean {
  let flags: number = (record.fields[RECORD_FIELDS.flags] ?? 0);
  if (flags & flag) {
    return true
  } else {
    return false
  }
}