/* eslint-disable no-unused-vars */
import {Accessor, createMemo, createResource, createSignal, onCleanup, onMount, startTransition} from "solid-js";
import qs from "qs";
import {BASE_URL, RECORD_FIELDS} from "~/consts";
import {TracingRecordDto} from "~/openapi";

export interface ListWithEventSourceOption<T> {
  fetchFirst: any,

  params(arg: T): any,

  parse(json: string): T,

  path: string
}


export function useListWithEventSource<T>({
                                            fetchFirst,
                                            params,
                                            path,
                                            parse
                                          }: ListWithEventSourceOption<T>): Accessor<T[]> {

  const [firstData, _] = createResource(() => fetchFirst(), {
    initialValue: []
  });
  const [afterData, setAfterData] = createSignal<T[]>();

  let curData = createMemo(() => {
    return [...(afterData() ?? []), ...firstData()]
  });

  onMount(() => {
    let r = firstData();
    let params_value = params(r.length > 0 ? r[r.length - 1] : null);
    let params_string = Object.keys(params_value).length > 0 ? `?${qs.stringify(params_value)}` : '';
    const eventSource = new EventSource(`${BASE_URL}/${path}${params_string}`);
    eventSource.onmessage = (event) => {
      let data: T = parse(event.data);
      setAfterData(n => [data, ...(n ?? [])])
    };

    onCleanup(() => {
      eventSource.close();
    });
  });
  return curData
}

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