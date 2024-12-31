import {
  CursorInfoToJSON,
  ListTreeRecordsRequest, TracingKind, TracingRecordScene,
  TracingTreeRecordDto,
  TracingTreeRecordDtoFromJSON, type TracingTreeRecordVariantDtoOneOf
} from "~/openapi";
import {BASE_URL, NULL_UUID} from "~/consts";
import {HttpClient} from "~/http_client";
import {
  Accessor, createEffect,
  createMemo,
  createSignal,
  getOwner,
  onCleanup,
  runWithOwner,
  Setter, startTransition,
  untrack
} from "solid-js";
import qs from "qs";
import {TracingTreeFilter} from "~/routes/traces";
import {createAsync} from "@solidjs/router";
import {createStore, produce, reconcile, unwrap} from "solid-js/store";

export interface RecordsTreeData {
  records: TracingTreeRecordDto[];
  more_loading: boolean;
  is_end: boolean
}

export function useRecordsTreeLive(options: {
  filter: TracingTreeFilter,
  appRunId?: Accessor<string>,
  spanTId: Accessor<string | null>,
  parentSpanTId: Accessor<string | null>,
  isEnd: Accessor<boolean>,
  search: Accessor<string>,
  onLivedAdded?(): void,
  count?: number,
  scene: Accessor<TracingRecordScene | null>,
  kinds?: TracingKind[],
}): [Accessor<RecordsTreeData>, { fetchMoreOlder: Setter<number>,fetchMore: Setter<number>, notMoreOlderData: Accessor<boolean> }] {


  let [notMoreOlderData, setNotMoreOlderData] = createSignal<boolean>(false);
  let [store, setStore] = createStore<RecordsTreeData>({
    records: [],
    more_loading: false,
    is_end: true
  });

  let count = options.count ?? 50;
  let recordsRequest = createMemo<ListTreeRecordsRequest>(() => {
    let spanTId = options.spanTId();
    let parentSpanTId = options.parentSpanTId();
    let scene = options.scene();
    let appRunId = options.appRunId ? options.appRunId() : null;
    let fields = [];
    let param: ListTreeRecordsRequest = {
      scene: options.scene(),
      count: count + 1,
      kinds: options.kinds ?? [],
      levels: options.filter.selectedLevels,
      app_build_ids: options.filter.selectedAppIds?.map(n => [n, null]),
      node_ids: options.filter.selectedNodeIds,
      parent_span_t_ids: options.parentSpanTId() != null ? [options.parentSpanTId() as any] : [],
      search: options.search ? options.search() : undefined,
    };
    if (parentSpanTId == null) {
      if (scene == "Tree") {
        param.parent_id = NULL_UUID;
      }
    }
    if (spanTId != null) {
      param.parent_id = undefined;
      fields.push({
        name: '__data.span_t_id',
        op: 'Equal',
        value: spanTId
      })
    }

    if (appRunId == null && scene == "Tree") {
      param.app_run_ids = [];
      param.kinds = [TracingKind.AppStart, TracingKind.AppStop]
      param.parent_id = undefined;
      param.levels = undefined;
    } else {
      if (appRunId != null) {
        param.app_run_ids = [appRunId];
      }
    }
    param.fields = fields;
    return param
  }, undefined, {
    equals: (a, b) => JSON.stringify(a) == JSON.stringify(b)
  });

  let [olderMoreRecordId, fetchMoreOlder] = createSignal(null,{
    equals: false
  });
  let [moreRecordId, fetchMore] = createSignal(null,{
    equals: false
  });
  // let [moreRecordId, fetchMore] = createSignal(null);
  createAsync(async () => {
    let _moreRecordId = moreRecordId();

    if (_moreRecordId != null) {
      setStore('more_loading', true);
      let param: ListTreeRecordsRequest = untrack(() => recordsRequest());
      param.cursor = CursorInfoToJSON({
        id: _moreRecordId,
        isBefore: false
      });
      let newData = (await HttpClient.listTreeRecords(param)) ?? [];
      let is_end = false;
      if (newData.length <= count) {
        is_end = true;
      }
      if (newData.length == (count + 1)) {
        newData.pop();
      }
      setStore(produce(n => {
        n.records.push(...newData);
        if (n.records.length > count * 2) {
          setNotMoreOlderData(false);
        }
        n.is_end = is_end;
        while (n.records.length > count * 2) {
          n.records.shift();
        }
        n.more_loading = false;
      }))
    }
  });
  createAsync(async () => {
    let moreRecordIdValue = olderMoreRecordId();

    if (moreRecordIdValue != null) {
      setStore('more_loading', true);
      let param: ListTreeRecordsRequest = untrack(() => recordsRequest());
      param.cursor = CursorInfoToJSON({
        id: moreRecordIdValue,
        isBefore: true
      });
      let olderData = (await HttpClient.listTreeRecords(param)) ?? [];
      if (olderData.length <= count) {
        setNotMoreOlderData(true);
      }
      if (olderData.length == (count + 1)) {
        olderData.shift();
      }
      setStore(produce(n => {
        n.records.unshift(...olderData);
        if (n.records.length > count * 2) {

          n.records = n.records.slice(0, count * 2);
          n.is_end = false;
        }
        n.more_loading = false;
      }))
    }
  });

  let eventSource: EventSource;

  createEffect(() => {
    if (options.isEnd()) {
      setTimeout(() => {
        eventSource?.close();
      }, 3000)
    }
  });

  let r = createAsync(async () => {
    let owner = getOwner();
    fetchMoreOlder(null);
    // if (!store.is_end){
    //   eventSource?.close();
    //   return store;
    // }
    let param = recordsRequest();
    let r: TracingTreeRecordDto[] = (await HttpClient.listTreeRecords(param)) ?? [];
    if (r.length <= count) {
      setNotMoreOlderData(true);
    }
    if (r.length == (count + 1)) {
      r.shift();
    }
    setStore('records', reconcile(r));

    // return store;
    if (!options.isEnd()) {
      let cursor_id = r.at(-1)?.record.id;
      param.cursor = cursor_id ? CursorInfoToJSON({
        id: cursor_id,
        isBefore: false
      }) : null;
      let params_string = Object.keys(param).length > 0 ? `?${qs.stringify(param)}` : '';

      eventSource = new EventSource(`${BASE_URL}/records_subscribe${params_string}`);
      eventSource.onopen = () => {
      };
      eventSource.onerror = (err) => {
        console.warn("eventSource error", err);
        eventSource.close();
      }
      eventSource.onmessage = (event) => {
        let record = TracingTreeRecordDtoFromJSON(JSON.parse(event.data));
        if (record.record.kind == TracingKind.SpanClose) {
          setStore('records', n => n.record.spanTId == record.record.spanTId && n.record.kind == TracingKind.SpanCreate, produce(n => {
            n.end = record.end;
            delete record.variant['spanRun']['relatedEvents'];
            n.variant['spanRun'] = {...n.variant['spanRun'], ...record.variant['spanRun']};
          }))

        } else if (record.record.kind == TracingKind.AppStop) {
          setStore('records', n => n.record.appRunId == record.record.appRunId && n.record.kind == TracingKind.AppStart, produce(n => {
            n.end = record.end;
            n.variant = record.variant;
          }))
        } else if (record.record.kind == TracingKind.RepEvent) {
          setStore(produce(n => {
            if (n.records.at(-1).record.repeatedCount == null) {
              n.records.at(-1).record.repeatedCount = 0;
            }
            n.records.at(-1).record.repeatedCount += 1;
          }))
        } else if (record.record.kind == TracingKind.RelatedEvent) {
          // @ts-ignore
          setStore('records', a => a.record.spanTId == record.record.spanTId && a.record.kind == TracingKind.SpanCreate, 'variant', 'spanRun', produce((n: any) => {
            n.relatedEvents.push(record.record);
          }))
        } else if (record.record.kind == TracingKind.SpanRecord) {
          setStore('records', a => a.record.spanTId == record.record.spanTId && a.record.kind == TracingKind.SpanCreate, produce(n => {
            for (let key in record.record.fields) {
              n.record.fields[key] = record.record.fields[key];
              if ((n.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun?.fields) {
                (n.variant as TracingTreeRecordVariantDtoOneOf).spanRun.fields[key] = record.record.fields[key];
              }
            }
          }))
        } else {
          if (store.is_end) {
            startTransition(() => {
              setStore(produce(n => {
                n.records.push(record)
                if (n.records.length > count * 2){
                  setNotMoreOlderData(false);
                }
                while (n.records.length > count * 2){
                  n.records.shift();
                }
              }))
            });
            options.onLivedAdded?.call(null);
          }
        }
      };

      runWithOwner(owner, () => {
        onCleanup(() => {
          console.log('close event source');
          eventSource.close();
        })
      });
    }
    return store;
  })
  return [r, {
    fetchMoreOlder,
    fetchMore,
    notMoreOlderData
  }]
}
