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
import {createStore, produce, reconcile} from "solid-js/store";

export interface RecordsTreeData {
  records: TracingTreeRecordDto[];
  more_loading: boolean
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
}): [Accessor<RecordsTreeData>, { fetchMore: Setter<number>, notMoreData: Accessor<boolean> }] {


  let [notMoreData, setNotMoreData] = createSignal<boolean>(false);
  let [store, setStore] = createStore<RecordsTreeData>({
    records: [],
    more_loading: false
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
      parent_span_t_id: options.parentSpanTId() as any,
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
  });

  let [moreRecordId, fetchMore] = createSignal(null);

  createAsync(async () => {
    let moreRecordIdValue = moreRecordId();

    if (moreRecordIdValue != null) {
      setStore('more_loading', true);
      let param: ListTreeRecordsRequest = untrack(() => recordsRequest());
      param.cursor = CursorInfoToJSON({
        id: moreRecordIdValue,
        isBefore: true
      });
      let olderData = (await HttpClient.listTreeRecords(param)) ?? [];
      if (olderData.length <= count) {
        setNotMoreData(true);
      }
      if (olderData.length == (count + 1)) {
        olderData.shift();
      }
      setStore(produce(n => {
        n.records.unshift(...olderData);
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
    fetchMore(null);
    let param = recordsRequest();
    let r: TracingTreeRecordDto[] = (await HttpClient.listTreeRecords(param)) ?? [];
    if (r.length <= count) {
      setNotMoreData(true);
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
            n.variant = record.variant;
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
        } else if (record.record.kind == TracingKind.SpanRecord) {
          setStore('records', a => a.record.spanTId == record.record.spanTId && a.record.kind == TracingKind.SpanCreate, produce(n => {
            for (let key in record.record.fields) {
              n.record.fields[key] = record.record.fields[key];
              (n.variant as TracingTreeRecordVariantDtoOneOf).spanRun.fields[key] = record.record.fields[key];
            }
          }))
        } else {

          startTransition(() => {
            setStore(produce(n => {
              n.records.push(record)
            }))
          });
          options.onLivedAdded?.call(null);
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
  });
  return [r, {
    fetchMore,
    notMoreData
  }]
}
