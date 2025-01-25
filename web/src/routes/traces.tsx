/* eslint-disable solid/reactivity */

import {createAsync, useSearchParams} from "@solidjs/router";
import {
  Accessor, batch,
  createContext,
  createEffect,
  createMemo, createSelector,
  createSignal,
  For,
  JSX, Match,
  onCleanup,
  onMount, Setter, Show,
  Signal,
  splitProps, startTransition,
  Suspense,
  Switch, untrack,
  useContext
} from "solid-js";
import {
  AppNodeRunDto,
  AppNodeRunDtoFromJSON,
  NodesPageDto,
  NodesPageRequest,
  TracingKind,
  TracingLevel,
  TracingRecordDto,
  TracingTreeRecordDto,
  TracingTreeRecordVariantDtoOneOf,
  type TracingTreeRecordVariantDtoOneOf1,
  TracingTreeRecordVariantDtoOneOf2
} from "~/openapi";
import {
  ALL_LEVELS,
  BASE_URL,
  durationOptions,
  EXPANDABLE_KINDS,
  getLevelColor,
  NULL_STR,
  RECORD_FIELDS
} from "~/consts";
import {useRecordsTreeLive} from "~/lib/use_records_live";
import {
  Dot, ChevronRight,
  ChevronDown,
  FoldVertical,
  LogIn,
  UnfoldVertical
} from "lucide-solid";
import {AsyncStorage, makePersisted, SyncStorage} from "@solid-primitives/storage";
import {Loading, LoadingPanel} from "~/components/Loading";
import {createElementBounds} from "@solid-primitives/bounds";
import {AUTO_EXPAND, createMultiSelection, getFlags} from "~/utils";
import {Button} from "~/components/ui/button";
import HTMLAttributes = JSX.HTMLAttributes;
import {Checkbox} from "~/components/ui/checkbox";
import {createStore, produce, reconcile} from "solid-js/store";
import {cn} from "~/lib/utils";
import qs from "qs";
import byteSize, {ByteSizeOptions} from "byte-size";
import humanizeDuration from "humanize-duration";
import {Key} from "@solid-primitives/keyed";
import {debounce, Scheduled} from "@solid-primitives/scheduled";
import {AppEmpty} from "~/components/Empty";
import {t} from "i18next";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger
} from "~/components/ui/context-menu";
import {HttpClient} from "~/http_client";

const CurSelectedTreeItem = createContext<Signal<SelectedTreeItem | null>>();
const CurTracePath = createContext<Signal<TracePathItem[]>>();

export function SelectedTreeItemProvider(props: {
  defaultValue?: SelectedTreeItem,
  children: JSX.Element
}) {
  let [curTracePath] = useContext(CurTracePath);
  let signal = makePersisted(createSignal(props.defaultValue), {
    name: 'cs',
    storage: useRecordIdSearchQueryStorage(),
    serialize: (n: TracePathItem) => {
      let id = n.record.record?.id?.toString();
      return JSON.stringify(id ? [id] : [])
    },
    deserialize: (n: string) => {
      return n ? {
        record: JSON.parse(n)[0],
        path: curTracePath()
      } : undefined;
    },
  });

  return (
    <CurSelectedTreeItem.Provider value={[signal[0], signal[1]]}>
      {props.children}
    </CurSelectedTreeItem.Provider>
  );
}

export type ShowMode = "Tree" | "Flatten";
export type TimeShowMode = "ServerTime" | "LocalTime" | "AppSetupTime";

export const SHOW_MODES: ShowMode[] = ["Tree", "Flatten"];

// export const TIME_SHOW_MODES: TimeShowMode[] = ["ServerTime", "LocalTime", "AppSetupTime"];

export interface TracingTreeFilter {
  // selectedKinds: string[];
  selectedLevels: TracingLevel[];
  selectedAppIds: string[];
  selectedAppVersions: string[];
  selectedNodeIds: string[];
  showMode?: ShowMode;
  timeShowMode?: TimeShowMode;
  scrollToBottomWhenAdded?: boolean;
}


export interface TraceTreeInfo {
  filter: TracingTreeFilter,
  search: Accessor<string>,
  date: Date,
  rootContainerElement: Accessor<HTMLElement>
}

export const CurTraceTreeInfo = createContext<TraceTreeInfo>();

export function TraceTreeInfoProvider(props: {
  children: JSX.Element
} & TraceTreeInfo) {
  let [otherProps, value] = splitProps(props, ['children']);
  return (
    <CurTraceTreeInfo.Provider value={value}>
      {otherProps.children}
    </CurTraceTreeInfo.Provider>
  );
}

export function NodeItem(allProps: {
  data: AppNodeRunDto,
  now: Date,
  isSelected: boolean
} & HTMLAttributes<HTMLDivElement>): JSX.Element {
  let [props, rootProps] = splitProps(allProps, ['data', 'isSelected', 'now']);
  return (
    <div {...rootProps}
         class={`p-1.5 pb-1 gap-1.5 border-gray-100 border border-t-2 flex flex-col w-[220px] cursor-pointer select-none  ${rootProps.class}`}
         classList={{
           "bg-stone-100": props.isSelected,
           "border-t-primary": props.isSelected,
           "hover:bg-stone-50": !props.isSelected,
           ...rootProps.classList
         }}>
      <div class="flex justify-between items-baseline overflow-hidden gap-1">
        <div
          class="text-lg font-bold flex-shrink-0 text-ellipsis overflow-hidden whitespace-nowrap">{(props.data.data ?? {})['name'] ?? (props.data.data ?? {})['node_name'] ?? props.data.nodeId}</div>
        <div
          class="text-xsm text-ellipsis overflow-hidden whitespace-nowrap">{(props.data.data ?? {})['second_name'] ?? (props.data.data ?? {})['os_name']}</div>
      </div>
      <div class="flex text-xsm items-center justify-between gap-2">
        <div class="text-sm">{(props.data.data ?? {})['brief'] ?? (props.data.data ?? {})['ip']}</div>
        <Show when={props.data.stopTime == null} fallback={
          <div class="text-xsm text-ellipsis overflow-hidden whitespace-nowrap">
            {/*<span title={"离线"}></span>*/}
            {humanizeDuration((props.now.getTime() - props.data.startTime.getTime()), durationOptions)}
          </div>
        }>
          <div class="text-xsm text-ellipsis overflow-hidden whitespace-nowrap">
            {/*<span title={"在线"}></span>*/}
            {humanizeDuration((props.now.getTime() - props.data.startTime.getTime()), durationOptions)}
          </div>
        </Show>
        {/*<div>{n.appVersion}</div>*/}
      </div>
    </div>
  )
}


export function useNodePageData(input: Accessor<NodesPageRequest>): [NodesPageDto, IsLoadingObj] {
  let [data, setData] = createStore<NodesPageDto>(undefined);
  let isLoading = createIsLoading();
  let nodesPageAccessor = createAsync(async () => {
    return await isLoading.loadingScoped(async () => {
      return await HttpClient.nodesPage(input())
    });
  });
  createEffect(async () => {
    let nodesPage = nodesPageAccessor();
    setData(reconcile(nodesPage));
    if (nodesPage == null) {
      return;
    }
    if (nodesPage.nodes.length == 0) {
      return;
    }
    let params_string = qs.stringify({
      ...untrack(() => input()),
      after_record_id: nodesPage.nodes[0].recordId
    });

    const eventSource = new EventSource(`${BASE_URL}/node_page_subscribe?${params_string}`);
    eventSource.onmessage = (event: MessageEvent) => {
      let data: any = JSON.parse(event.data)

      if ('NewNode' in data) {
        let newNode = AppNodeRunDtoFromJSON(data['NewNode']);
        setData(produce(n => {
          n.nodes.unshift(newNode);
        }))
      } else if ('NodeAppStart' in data) {
        let nodeAppStart = AppNodeRunDtoFromJSON(data['NodeAppStart']);
        setData('nodes', n => n.nodeId == nodeAppStart.nodeId, {
          ...nodeAppStart,
          stopTime: null,
          stopRecordId: null,
          exceptionEnd: false
        })
      } else if ('NodeAppStop' in data) {
        let nodeAppStop = AppNodeRunDtoFromJSON(data['NodeAppStop']);
        setData('nodes', n => n.nodeId == nodeAppStop.nodeId, {
          stopTime: nodeAppStop.stopTime,
          stopRecordId: nodeAppStop.stopRecordId,
          exceptionEnd: nodeAppStop.exceptionEnd
        })
      }
    };

    onCleanup(() => {
      eventSource.close();
    });
  });
  return [data, isLoading];

}

export interface LoadingScopedCallback<U = void> {
  (): Promise<U>
}

export interface IsLoadingObj {
  (): boolean,

  loadingScoped: <U>(callback: LoadingScopedCallback<U>) => Promise<U>,
  startTransition: (callback: () => unknown) => Promise<void>,
  setLoading: Scheduled<boolean[]>,
}

export function createIsLoading(debounce_time: number = 100): IsLoadingObj {
  let signal = createSignal<boolean>(false);
  let setLoading = debounce(signal[1], debounce_time);
  // let result = signal[0];
  let result = signal[0] as IsLoadingObj;
  result.loadingScoped = async (callback) => {
    setLoading(true);
    let result = await callback();
    setLoading(false);
    return result
  };
  result.startTransition = (callback) => {
    return result.loadingScoped(() => startTransition(callback))
  };
  result.setLoading = setLoading;
  return result;
}

export interface TracePathItem {
  record: TracingTreeRecordDto,
}

export function useSearchParamStorage(): SyncStorage {
  let [searchParam, setSearchParam] = useSearchParams();
  return {
    getItem(key: string): string {
      return searchParam[key] as string;
    },
    setItem(key: string, value: string) {
      setSearchParam({
        [key]: value
      });
    },
    removeItem(key: string) {
      setSearchParam({
        [key]: undefined
      });
    }
  }
}


export function useRecordIdSearchQueryStorage(): AsyncStorage {
  let [searchParam, setSearchParam] = useSearchParams();
  return {
    async getItem(key: string): Promise<string> {
      let idsStr = searchParam[key] as string;
      if (idsStr == null) {
        return null;
      }
      let ids: any[] = JSON.parse(idsStr);
      if (ids.length == 0) {
        return null;
      }
      let r = await HttpClient.listTreeRecordsByIds({
        ids
      })
      return JSON.stringify(r);
    },
    async setItem(key: string, value: string) {
      setSearchParam({
        [key]: value
      });
    },
    async removeItem(key: string) {
      setSearchParam({
        [key]: undefined
      });
    }
  }
}

export function Traces() {
  const [tracingTreeFilter, setTracingTreeFilter] = makePersisted(createStore<TracingTreeFilter>({
    selectedLevels: [...ALL_LEVELS],
    selectedAppIds: [],
    selectedNodeIds: [],
    selectedAppVersions: [],
    showMode: "Tree",
    timeShowMode: "ServerTime",
    scrollToBottomWhenAdded: false
  }, {}), {name: "tracingTreeFilter"});

  let [rootContainerElement, setRootContainerElement] = createSignal<HTMLElement>();
  // let [showAppFilter, setShowAppFilter] = createSignal<boolean>(true);
  // let [showEvent, setShowEvent] = createSignal<boolean>(true);
  let [curSearch, setCurSearch] = createSignal<string>();
  let [nodesPage, isLoadingNodes] = useNodePageData(() => ({
    app_build_ids: tracingTreeFilter.selectedAppIds?.map(n => [n, null])
  }));
  let nodeSelection = createMultiSelection(() => tracingTreeFilter.selectedNodeIds, (value: string[]) => {
    setTracingTreeFilter(produce(n => {
      n.selectedNodeIds = value;
    }));
  });
  let appSelection = createMultiSelection(() => tracingTreeFilter.selectedAppIds, (value: string[]) => {
    setTracingTreeFilter(produce(n => {
      n.selectedAppIds = value;
    }));
  });
  let levelSection = createMultiSelection(() => tracingTreeFilter.selectedLevels, (value: TracingLevel[]) => {
    setTracingTreeFilter(produce(n => {
      n.selectedLevels = value;
    }));
  });
  let [leftElement, setLeftElement] = createSignal<HTMLElement>();
  let leftElementHeight = createMemo<number>(() => {
    const bounds = createElementBounds(leftElement());
    return bounds.height;
  });

  let nodesContainerElement: HTMLDivElement;

  let [nodeSearch, _setNodeSearch] = createSignal('');
  let setNodeSearch = debounce(_setNodeSearch, 300);
  return (
    <Show when={nodesPage}>
      <CurTracePath.Provider value={makePersisted(createSignal([]), {
        name: "curTracePath",
        serialize: (n: TracePathItem[]) => {
          return JSON.stringify(n.map(n => n.record.record.id.toString()))
        },
        deserialize: (n: string) => {
          return JSON.parse(n).map(n => ({
            record: n
          }));
        },
        storage: useRecordIdSearchQueryStorage(),
      }) as any}>
        <SelectedTreeItemProvider>
          <div class="flex flex-col gap-3 overflow-hidden flex-grow">
            <div class="panel flex flex-col p-2 gap-2  flex-grow-0">
              <div class="flex flex-row border-b pb-2 gap-2 justify-between">
                <div class={"flex flex-row flex-1"}>
                  <For each={nodesPage.apps}>
                    {n => <div
                      class={"flex gap-1 text-nowrap items-center hover:bg-stone-100 cursor-pointer rounded px-2 py-1 -my-1"}
                      onClick={() => {
                        appSelection.toggle(n.id);
                        nodeSelection.clear();
                      }}>
                      <Checkbox onClick={async (e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        batch(() => {
                          nodeSelection.clear();
                          appSelection.toggle(n.id);
                        })
                      }} checked={appSelection.isSelect(n.id)}/>
                      <div class="text-sm">{n.name} ( {n.nodeCount} )</div>
                    </div>}
                  </For>
                </div>
                <input onChange={n => setNodeSearch(n.target.value)} placeholder={t("common:search")}
                       class={"p-2 px-3 border-x -m-2 outline-none text-sm"}/>
                {/*<div>{nodesPage.nodes.filter(n => n.stopTime == null).length}/{nodesPage.nodes?.length}</div>*/}
                <div class={"flex flex-1"}>
                  <div class={"flex-grow"}></div>
                  <div>{nodesPage.nodes?.length}</div>
                </div>
              </div>
              <div class="flex gap-2 overflow-x-auto small-scrollbar" classList={{
                'justify-center': isLoadingNodes(),
                '-mb-2': nodesPage?.nodes?.length > 0 && (nodeSearch() || true) && nodesContainerElement?.offsetWidth < nodesContainerElement?.scrollWidth
              }} ref={nodesContainerElement}>
                <Show when={!isLoadingNodes()} fallback={<Loading class={"self-center"}
                                                                  style={{height: nodesContainerElement ? `${nodesContainerElement.offsetHeight}px` : 'auto'}}/>}>
                  <For each={nodesPage.nodes?.filter(n => {
                    let value = (nodeSearch() ?? "").trim().toLowerCase();
                    if (value == "") {
                      return true;
                    }
                    return n.nodeId?.toLowerCase().includes(value)
                      || n.appRunId?.toLowerCase().includes(value)
                      || n.appBuildIds?.some(n => n?.some(n => n?.toLowerCase().includes(value)))
                      || Object.keys(n.data ?? {}).some(key => {
                        return n.data[key]?.toString()?.toLowerCase().includes(value);
                      })
                  })} fallback={<div class={"flex justify-center items-center text-center flex-grow"}
                                     style={{height: nodesContainerElement ? `${nodesContainerElement.offsetHeight}px` : 'auto'}}>空</div>}>
                    {(n) => <NodeItem now={nodesPage.date} class={"box-border"} data={n}
                                      isSelected={nodeSelection.isSelect(n.nodeId)}
                                      onClick={() => nodeSelection.toggle(n.nodeId)}/>}
                  </For>
                </Show>
              </div>
            </div>
            <div class="flex flex-grow gap-3 overflow-hidden">
              <TraceTreeInfoProvider rootContainerElement={rootContainerElement}
                                     search={curSearch}
                                     date={nodesPage.date}
                                     filter={tracingTreeFilter}>
                <div ref={setLeftElement} class="flex flex-col flex-grow gap-3 overflow-hidden">
                  {/*<div class="panel p-2 flex flex-row">*/}
                  {/*  <div>span 路径</div>*/}
                  {/*  <div>span fields</div>*/}
                  {/*</div>*/}
                  <div class="panel py-2 px-3 flex">
                    <div class="flex flex-grow gap-3 items-stretch">
                      <input placeholder={`${t("common:search")} (PostgreSQL Like Syntax)`}
                             class={"outline-none flex-grow flex-shrink text-sm bg-gray-50 block"}
                             style={{
                               "padding": "8px",
                               "margin": "-8px -4px",
                               "max-width": '300px'
                             }} onKeyDown={e => {
                        if (e.key === "Enter") {
                          setCurSearch((e.target as HTMLInputElement).value);
                        }
                      }}>SEARCH</input>
                      {/*  <div>时间范围</div>*/}
                      {/*  <div>预设</div>*/}
                      {/*  /!*<div>保存预设</div>*!/*/}
                      {/*  <div>高级过滤</div>*/}
                      <div class={"flex-grow"}/>
                      {/*<div class="flex items-center p-2 -my-2 rounded cursor-pointer -mr-2 select-none hover:bg-stone-100"*/}
                      {/*     onClick={() => setShowAppFilter(n => !n)}>*/}
                      {/*  <SwitchUI class={"h-4 mb-1"} checked={showAppFilter()}>*/}
                      {/*    <SwitchControl class={"h-4 w-9"}>*/}
                      {/*      <SwitchThumb class={"size-3"}/>*/}
                      {/*    </SwitchControl>*/}
                      {/*  </SwitchUI>*/}
                      {/*  <span class="pl-2 text-sm">SHOW APP FILTER</span>*/}
                      {/*</div>*/}
                      {/*<div class="flex items-center p-2 -my-2 rounded cursor-pointer -mr-2 select-none hover:bg-stone-100"*/}
                      {/*     onClick={() => setShowEvent(n => !n)}>*/}
                      {/*  <SwitchUI class={"h-4 mb-1"} checked={showEvent()}>*/}
                      {/*    <SwitchControl class={"h-4 w-9"}>*/}
                      {/*      <SwitchThumb class={"size-3"}/>*/}
                      {/*    </SwitchControl>*/}
                      {/*  </SwitchUI>*/}
                      {/*  <span class="pl-2 text-sm">Show Event</span>*/}
                      {/*</div>*/}
                      <div class="flex  border-stone-50 flex-row  -my-2 items-stretch">
                        <For each={[...ALL_LEVELS]}>
                          {n => <div onClick={() => levelSection.toggleSingle(n)}
                                     class="text-xsm border-transparent text-gray-500 select-none flex items-center  cursor-pointer font-bold p-2"
                                     title={levelSection.isSelect(n) ? "disable it" : "enable it"}
                                     style={{
                                       ...(levelSection.isSelect(n) ? {
                                         // color: "black",
                                         "border-color": getLevelColor(n),
                                         "color": getLevelColor(n),
                                         // "border-color": getLevelColor(n),
                                       } : {
                                         "color": getLevelColor(n),
                                       })
                                     }} classList={{
                            "bg-stone-100": levelSection.isSelect(n),
                            "hover:bg-stone-50": !levelSection.isSelect(n)
                          }}>{n.toUpperCase()}</div>}
                        </For>
                      </div>
                      {/*<div>*/}
                      {/*  <div class="pb-2 text-sm">类型：</div>*/}
                      {/*  <Select<string>*/}
                      {/*    multiple placeholder={"全部"}*/}
                      {/*    onChange={n => {*/}
                      {/*      setSelectedKinds(n);*/}
                      {/*    }}*/}
                      {/*    value={selectedKinds()}*/}
                      {/*    disallowEmptySelection={false}*/}
                      {/*    options={TREE_KINDS}*/}
                      {/*    itemComponent={(props) => {*/}
                      {/*      return <SelectItem*/}
                      {/*        item={props.item}>{props.item.textValue}</SelectItem>*/}
                      {/*    }}*/}
                      {/*  >*/}
                      {/*    <SelectTrigger class="w-[180px]">*/}
                      {/*      <SelectValue<string>*/}
                      {/*        class="text-ellipsis overflow-hidden whitespace-nowrap pr-2">{(state) => {*/}
                      {/*        let selectedOptions = state.selectedOptions();*/}
                      {/*        if (selectedOptions.length == TREE_KINDS.length) {*/}
                      {/*          return "全部";*/}
                      {/*        } else {*/}
                      {/*          return selectedOptions.toString()*/}
                      {/*        }*/}
                      {/*      }}</SelectValue>*/}
                      {/*    </SelectTrigger>*/}
                      {/*    <SelectContent/>*/}
                      {/*  </Select>*/}
                      {/*</div>*/}
                      {/*<div>*/}
                      {/*  <div class="pb-2 text-sm">级别：</div>*/}
                      {/*  <Select<TracingLevel>*/}
                      {/*    multiple placeholder={"全部"}*/}
                      {/*    onChange={n => {*/}
                      {/*      setSelectedLevels(n);*/}
                      {/*    }}*/}
                      {/*    value={selectedLevels()}*/}
                      {/*    disallowEmptySelection={false}*/}
                      {/*    options={ALL_LEVELS}*/}
                      {/*    itemComponent={(props) => {*/}
                      {/*      return <SelectItem class="" item={props.item}>*/}
                      {/*        <span style={{background: getLevelColor(props.item.rawValue)}}*/}
                      {/*              class="p-1 mr-2"/>*/}
                      {/*        <span>{props.item.textValue}</span>*/}
                      {/*      </SelectItem>*/}
                      {/*    }}*/}
                      {/*  >*/}
                      {/*    <SelectTrigger class="w-[180px]">*/}
                      {/*      <SelectValue<string>*/}
                      {/*        class="text-ellipsis overflow-hidden whitespace-nowrap pr-2">{(state) => {*/}
                      {/*        let selectedOptions = state.selectedOptions();*/}
                      {/*        if (selectedOptions.length == ALL_LEVELS.length) {*/}
                      {/*          return "全部";*/}
                      {/*        } else {*/}
                      {/*          return selectedOptions.toString()*/}
                      {/*        }*/}
                      {/*      }}</SelectValue>*/}
                      {/*    </SelectTrigger>*/}
                      {/*    <SelectContent/>*/}
                      {/*  </Select>*/}
                      {/*</div>*/}
                      {/*<div class={"flex-grow"}/>*/}
                      {/*<div>*/}
                      {/*  <div class="pb-2 text-sm">日期显示：</div>*/}
                      {/*  <Select<string>*/}
                      {/*    value={"对齐到服务器"}*/}
                      {/*    disallowEmptySelection={true}*/}
                      {/*    options={["对齐到服务器", "本地", "距离App运行时间"]}*/}
                      {/*    itemComponent={(props) => {*/}
                      {/*      return <SelectItem item={props.item}>*/}
                      {/*        {props.item.textValue}*/}
                      {/*      </SelectItem>*/}
                      {/*    }}*/}
                      {/*  >*/}
                      {/*    <SelectTrigger class="w-[180px]">*/}
                      {/*      <SelectValue<string[]>*/}
                      {/*        class="text-ellipsis overflow-hidden whitespace-nowrap pr-2">{(state) => {*/}
                      {/*        return state.selectedOption().toString()*/}
                      {/*      }}</SelectValue>*/}
                      {/*    </SelectTrigger>*/}
                      {/*    <SelectContent/>*/}
                      {/*  </Select>*/}
                      {/*</div>*/}
                    </div>
                  </div>

                  {/*<TraceTreeInfoProvider rootContainerElement={rootContainerElement}*/}
                  {/*                       selectedKinds={selectedKinds}*/}
                  {/*                       selectedLevels={selectedLevels}>*/}
                  {/*  <AppRunList></AppRunList>*/}
                  {/*  /!*<TracingTreeItemList ref={setRootContainerElement} layer={0} isEnd={false}*!/*/}
                  {/*  /!*                     spanTId={null}*!/*/}
                  {/*  /!*                     class="h-full flex-grow overflow-y-scroll"/>*!/*/}
                  {/*</TraceTreeInfoProvider>*/}
                  <div class="flex flex-col flex-grow overflow-hidden gap-3">
                    {/*<AppList/>*/}

                    <div class="panel py-2 overflow-hidden flex flex-col">
                      <div class="flex justify-between border-b pb-2 items-center gap-2 px-2">
                        <div class={"flex-grow"}>
                          {(() => {
                            let [curTracePath, setCurTracePath] = useContext(CurTracePath);
                            return <TracePath hasRoot={true} class={"-my-1"} path={curTracePath()}
                                              onClickItem={(path) => setCurTracePath(path)}></TracePath>
                          })()}
                        </div>
                        <div class={"flex flex-row rounded overflow-hidden border-2 border-gray-100"}>
                          <For each={SHOW_MODES}>
                            {showMode => <div class={"py-1 px-2 test-xsm cursor-pointer"}
                                              onClick={() => {
                                                setTracingTreeFilter(produce(n => {
                                                  n.showMode = showMode;
                                                }))
                                              }} classList={{
                              "hover:bg-stone-100": tracingTreeFilter.showMode != showMode,
                              "bg-primary": tracingTreeFilter.showMode == showMode,
                              "text-primary-foreground": tracingTreeFilter.showMode == showMode,
                            }}>{showMode}</div>}
                          </For>
                        </div>
                        {/*<div>滚动到底部</div>*/}
                        {/*<select class="p-2 mr-2 hover:bg-stone-100" value={tracingTreeFilter.timeShowMode}*/}
                        {/*        onChange={event => setTracingTreeFilter(produce(n => {*/}
                        {/*          n.timeShowMode = event.target.value as TimeShowMode;*/}
                        {/*        }))}>*/}
                        {/*  <For each={TIME_SHOW_MODES}>*/}
                        {/*    {n => <option class={""} value={n}>{n}</option>}*/}
                        {/*  </For>*/}
                        {/*</select>*/}
                        {/*<div></div>*/}
                      </div>
                      <Suspense fallback={<LoadingPanel/>}>
                        <Show when={useContext(CurTracePath)[0]()} keyed={true}>
                          {(path: TracePathItem[]) => {
                            let spanTId = path.length > 0 ? path.at(-1).record.record.spanTId : null;
                            let isEnd = () => path.length > 0 ? path.at(-1).record.end != null : false;
                            let appRunId = path.length > 0 ? path[0].record.record.appRunId : null;
                            return <Switch>
                              <Match when={tracingTreeFilter.showMode == "Tree"}>
                                <TracingTreeItemList ref={setRootContainerElement} layer={path.length}
                                                     isEnd={isEnd} path={path}
                                                     spanTId={spanTId} appRunId={appRunId}
                                                     class="overflow-hidden overflow-y-scroll"/>
                              </Match>
                              <Match when={tracingTreeFilter.showMode == "Flatten"}>
                                <TracingRecordTable appRunId={appRunId} spanTId={spanTId}
                                                    ref={setRootContainerElement}/>
                              </Match>
                            </Switch>
                          }}
                        </Show>
                      </Suspense>
                    </div>
                  </div>
                </div>
                <Show when={useCurSelectedTreeItem().selected()?.record} keyed>
                  {_ => <SelectedDetail class={"flex-grow-0"} style={{"max-height": `${leftElementHeight()}px`}}
                                        data={useCurSelectedTreeItem().selected()}/>}
                </Show>
              </TraceTreeInfoProvider>
            </div>
          </div>
        </SelectedTreeItemProvider>
      </CurTracePath.Provider>
    </Show>
  )
}

// const getAppRuns = cache(async () => {
//   let {appId} = useParams();
//   return await HttpClient.listAppRuns({
//     app_id: appId
//   })
// }, "appRuns");
//
// function AppList() {
//   const curAppRuns: Accessor<AppRunDto[]> = useListWithEventSource<AppRunDto>({
//     params: arg => ({
//       cursor: arg.recordId
//     }),
//     parse: n => AppRunDtoFromJSON(JSON.parse(n)),
//     path: "app_run_subscribe",
//     fetchFirst: getAppRuns
//   });
//
//   return (
//     <div class="panel flex flex-col">
//       <For each={curAppRuns()}>
//         {n => <div>
//           {n.appId}
//         </div>}
//       </For>
//     </div>
//   )
// }

function PropertyRow(allProps: {
  label: JSX.Element,
  labelContainerClass?: string
  valueContainerClass?: string
} & HTMLAttributes<HTMLTableRowElement>) {
  let [props, rootProps] = splitProps(allProps, ['label', 'labelContainerClass', 'valueContainerClass', 'children']);
  let valueElement: HTMLTableCellElement;
  let labelElement: HTMLTableCellElement;
  let [towRow, setTwoRow] = createSignal(false);
  onMount(() => {
    if (valueElement.getBoundingClientRect().height > ((valueElement.computedStyleMap().get("font-size") as CSSUnitValue)?.value ?? 16) * 3.5) {
      // valueElement.setAttribute("colspan", "2");
      // labelElement.setAttribute("colspan", "2");
      setTwoRow(true);
    }
  });
  let labelTd = (colSpan: number) =>
    <td ref={labelElement} colSpan={colSpan}
        class={cn("w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap", props.labelContainerClass)}>
      {props.label}
    </td>;
  let valueTd = (colSpan: number) =>
    <td ref={valueElement} colSpan={colSpan}
        class={cn("text-xsm", colSpan == 2 && "pl-6 pr-2", props.valueContainerClass)}>
      {colSpan == 2 ? <div
        class={"max-h-[400px] overflow-y-auto break-all whitespace-pre-wrap pr-2"}>{props.children}</div> : props.children}
    </td>;
  return (
    <>
      <Show when={towRow()} fallback={
        <tr {...rootProps}
            class={cn("align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50", rootProps.class)}>
          {labelTd(1)}
          {valueTd(1)}
        </tr>
      }>
        <tr {...rootProps}
            class={cn("align-top leading-8 hover:bg-stone-50", rootProps.class)}>
          {labelTd(2)}
        </tr>
        <tr {...rootProps}
            class={cn("align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50", rootProps.class)}>
          {valueTd(2)}
        </tr>
      </Show>
    </>
  )
}

function PropertyExpandableRow(allProps: {
  label: JSX.Element,
  tailing?: JSX.Element,
  defaultIsExpand?: boolean,
  labelContainerClass?: string
  tailingContainerClass?: string
  childrenContainerClass?: string,
  childrenContainerContainerClass?: string,
} & HTMLAttributes<HTMLTableRowElement>) {
  let [props, rootProps] = splitProps(allProps, ['label', 'tailing', 'defaultIsExpand', 'labelContainerClass', 'children', 'tailingContainerClass', 'childrenContainerClass', 'childrenContainerContainerClass']);
  let [isExpand, setIsExpand] = createSignal(props.defaultIsExpand ?? false);
  return (
    <>
      <tr {...rootProps}
          class={cn("leading-8 border-b  border-b-gray-100 hover:bg-stone-50", rootProps.class)}
          onClick={() => setIsExpand(n => !n)}>
        <td
          class={cn("w-[1%] select-none text-xsm font-bold pl-1 pr-1 min-w-[100px] text-left whitespace-nowrap text-nowrap overflow-hidden", props.labelContainerClass)}>
          <ChevronRight class={"inline-block mr-1 transition-transform"} classList={{"rotate-90": isExpand()}}
                        size={15}/>
          {props.label}
        </td>
        <td class={cn("text-sm", props.tailingContainerClass)}>
          {props.tailing}
        </td>
      </tr>
      <Show when={isExpand()}>
        <tr class={cn("border-x border-x-gray-400", props.childrenContainerContainerClass)}>
          <td colSpan={2} class={cn(props.childrenContainerClass)}>
            {props.children}
          </td>
        </tr>
      </Show>
    </>
  )
}

function PropertyTable(allProps: {} & HTMLAttributes<HTMLTableElement>) {
  return (
    <table {...allProps} class={cn("flex flex-col", allProps.class)}>
      <tbody class={""}>
      {allProps.children}
      </tbody>
    </table>
  )
}

function TracePath(allProps: {
  path: TracePathItem[],
  cur?: TracingTreeRecordDto,
  hasRoot?: boolean,
  onClickItem?: (_item: TracePathItem[]) => void,
} & HTMLAttributes<HTMLDivElement>) {
  let [props, rootProps] = splitProps(allProps, ['path', 'cur', 'hasRoot', 'children', 'onClickItem']);
  let cr = () =>
    <div class={"text-sm flex items-center cursor-pointer -mx-1 py-2 px-2 hover:bg-stone-50"}>
      <ChevronRight size={16} class={""} strokeWidth={2}></ChevronRight>
    </div>;
  return (
    <div {...rootProps} class={cn("inline-flex flex-grow-0 px-2 items-stretch", allProps.class)}>
      <Show when={props.hasRoot}>
        <div class={"text-sm cursor-pointer py-2 px-2 hover:bg-stone-50 text-nowrap overflow-hidden text-ellipsis"}
             onClick={props.onClickItem ? () => props.onClickItem([]) : null}>
          Root
        </div>
      </Show>
      {cr()}
      <For each={props.path}>
        {(n, index) => <>
          <div class={"text-sm cursor-pointer py-2 px-2 hover:bg-stone-50 text-nowrap overflow-hidden text-ellipsis"}
               onClick={props.onClickItem ? () => props.onClickItem(props.path.filter((n, i) => index() >= i)) : null}>{n.record?.record?.name}</div>
          {cr()}
        </>}
      </For>
      <Show when={props.cur}>
        <div class={"text-sm flex items-center py-1 px-1 text-nowrap overflow-hidden text-ellipsis"}>
          {props.cur.record.name}
        </div>
        {EXPANDABLE_KINDS.includes(props.cur.record.kind) && cr()}
      </Show>
    </div>
  )
}

function SelectedDetail(allProps: { data: SelectedTreeItem } & HTMLAttributes<HTMLDivElement>) {
  let [props, rootProps] = splitProps(allProps, ['data', 'children']);

  let record = props.data.record;
  type Tab = "Info" | "Enter List" | "Field Record";
  let tabs: Tab[] = ["Info"];
  switch (record.record?.kind) {
    case TracingKind.SpanCreate: {
      tabs.push("Enter List");
      tabs.push("Field Record");
    }
  }
  let [curTab, setCurTab] = makePersisted(createSignal(tabs[0]), {name: 'detailRecordItemCurTab'});
  if (!tabs.includes(curTab())) {
    setCurTab(tabs[0]);
  }
  let isCurTab = createSelector(curTab);
  let [, setCurTracePath] = useContext(CurTracePath);
  return (
    <div class="flex flex-col gap-3  w-[550px] max-w-[900px]">
      <div class={"flex items-stretch gap-2 justify-between"}>
        <TracePath class={"panel overflow-hidden"} path={props.data.path} cur={props.data.record}></TracePath>
        <div
          class={"px-4 py-2 panel flex-shrink-0 items-center text-sm justify-center flex text-center hover:bg-stone-50 cursor-pointer"}
          onClick={() => {
            setCurTracePath(EXPANDABLE_KINDS.includes(props.data.record.record.kind) ? [...props.data.path, {
              record: props.data.record,
              path: props.data.path
            }] : props.data.path)
          }}>Go
        </div>
      </div>
      <div {...rootProps}
           class={cn("panel flex-grow p-2 flex flex-col self-start flex-shrink-0 w-full", rootProps.class)}>
        <Show when={tabs.length > 1}>
          <div class={"flex gap-1 mb-1 px-2 -mx-2 bg-gray-50"}>
            <For each={tabs}>
              {n => <div class={"border-t-2 cursor-pointer p-2 px-3 "} classList={{
                "hover:bg-background": !isCurTab(n),
                "bg-background": isCurTab(n),
                "border-t-primary": isCurTab(n),
              }} onClick={() => setCurTab(n)}>{n}</div>}
            </For>
          </div>
        </Show>
        <div class={"flex-grow overflow-hidden -mx-2 flex flex-col overflow-y-auto"}>
          <Switch>
            <Match when={curTab() == "Info"}>
              <PropertyTable>
                <PropertyRow label={"Id"}>{record.record.id}</PropertyRow>
                <PropertyRow label={"RecordIndex"}>{record.record.recordIndex}</PropertyRow>
                <PropertyRow label={"Content"}>{record.record.name}</PropertyRow>
                <PropertyRow
                  label={"RecordTime"}>{record.record.recordTime?.toLocaleString()}</PropertyRow>
                <PropertyRow label={"NodeId"}>{record.record.nodeId}</PropertyRow>
                <PropertyRow
                  label={"ParentSpanTId"}>{record.record.parentSpanTId ?? NULL_STR}</PropertyRow>
                <Show
                  when={![TracingKind.AppStart, TracingKind.AppStop as TracingKind].includes(record.record.kind)}>
                  <PropertyExpandableRow defaultIsExpand={false} label={"CodeInfo"}
                                         tailing={<div
                                           class={"text-ellipsis overflow-hidden whitespace-nowrap"}>{record.record.positionInfo}</div>}>
                    <PropertyTable class={""}>
                      <PropertyRow label={"Target"}>{record.record.target}</PropertyRow>
                      <PropertyRow label={"ModulePath"}>{record.record.modulePath}</PropertyRow>
                      <PropertyRow
                        label={"PositionInfo"}>{record.record.positionInfo}</PropertyRow>
                      {/*<PropertyRow label={"ParentSpanId"}>{record.record.parentId}</PropertyRow>*/}
                    </PropertyTable>
                  </PropertyExpandableRow>
                </Show>
                <Show when={Object.keys(record.record.fields ?? {}).length != 0}>
                  <PropertyExpandableRow
                    defaultIsExpand={true}
                    label={"Fields"}>
                    <PropertyTable class={""}>
                      <TracingFields object={record.record.fields}></TracingFields>
                    </PropertyTable>
                  </PropertyExpandableRow>
                </Show>
                <Show
                  when={(record.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun}>
                  {n => <PropertyExpandableRow class={cn(Object.entries(n().fields).length == 0 && "hidden")}
                                               defaultIsExpand={true} label={"SpanLatestRecordFields"}>
                    <PropertyTable>
                      <TracingFields object={n().fields}></TracingFields>
                    </PropertyTable>
                  </PropertyExpandableRow>}
                </Show>
                <Switch>
                  <Match when={record.record.kind == TracingKind.SpanCreate}>
                    <PropertyExpandableRow childrenContainerClass={""} defaultIsExpand={true} label={"SpanInfo"}>
                      <PropertyTable class={""}>
                        <Show when={record.record.spanIdIsStable}>
                          <PropertyRow
                            label={"IsStable"}>{record.record.spanIdIsStable}</PropertyRow>
                        </Show>
                        <PropertyRow label={"SpanId"}>{record.record.spanId}</PropertyRow>
                        {/*<PropertyRow label={"ParentSpanId"}>{record.record.parentId}</PropertyRow>*/}
                      </PropertyTable>
                    </PropertyExpandableRow>
                    <PropertyExpandableRow childrenContainerClass={""} defaultIsExpand={true} label={"SpanRunInfo"}>
                      <PropertyTable class={""}>
                        <PropertyRow
                          label={"CreateRecordId"}>{record.record.id}</PropertyRow>
                        <PropertyRow
                          label={"SpanTId"}>{record.record.spanTId ?? NULL_STR}</PropertyRow>
                        <PropertyRow
                          label={"ParentSpanTId"}>{record.record.parentSpanTId ?? NULL_STR}</PropertyRow>
                        <Show
                          when={(record.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun}>
                          {n => <>
                            <PropertyRow
                              label={"TotalBusyDuration"}>{n().busyDuration != null ? humanizeDuration(n().busyDuration * 1000, durationOptions) : NULL_STR}</PropertyRow>
                            <PropertyRow
                              label={"TotalIdleDuration"}>{n().idleDuration != null ? humanizeDuration(n().idleDuration * 1000, durationOptions) : NULL_STR}</PropertyRow>
                          </>}
                        </Show>
                      </PropertyTable>
                    </PropertyExpandableRow>
                    <PropertyExpandableRow childrenContainerClass={""} defaultIsExpand={true} label={"SpanEndInfo"}>
                      <PropertyTable class={""}>
                        <Show
                          when={(record.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun}>
                          {n => <>
                            <PropertyRow
                              label={"EndDate"}>{String(record.end?.endDate.toLocaleString())}</PropertyRow>
                            <Show when={record.end?.exceptionEnd != null}>
                              <PropertyRow
                                label={"ExceptionEnd"}>{String(record.end?.exceptionEnd)}</PropertyRow>
                            </Show>
                            <PropertyRow
                              label={"CloseRecordId"}>{n().closeRecordId ?? NULL_STR}</PropertyRow>
                          </>}
                        </Show>
                      </PropertyTable>
                    </PropertyExpandableRow>
                  </Match>
                </Switch>
              </PropertyTable>
              <PropertyExpandableRow childrenContainerClass={""} defaultIsExpand={false} label={"Other"}>
                <PropertyTable class={""}>
                  <PropertyRow label={"ParentId"}>{record.record.parentId ?? NULL_STR}</PropertyRow>
                  <PropertyRow label={"Kind"}>{record.record.kind}</PropertyRow>
                  <PropertyRow label={"Level"}>{record.record.level}</PropertyRow>
                  <PropertyRow
                    label={"SpanTId"}>{record.record.spanTId ?? NULL_STR}</PropertyRow>
                  <PropertyRow
                    label={"CreationTime"}>{record.record.creationTime?.toLocaleString()}</PropertyRow>
                  {/*<PropertyRow label={"ParentSpanId"}>{record.record.parentId}</PropertyRow>*/}
                </PropertyTable>
              </PropertyExpandableRow>
              <PropertyExpandableRow childrenContainerClass={""} defaultIsExpand={false} label={"AppInfo"}>
                <PropertyTable class={""}>
                  <PropertyRow label={"AppId"}>{record.record.appId}</PropertyRow>
                  <PropertyRow label={"AppVersion"}>{record.record.appVersion}</PropertyRow>
                  <PropertyRow label={"AppRunId"}>{record.record.appRunId}</PropertyRow>
                  {/*<PropertyRow label={"ParentSpanId"}>{record.record.parentId}</PropertyRow>*/}
                </PropertyTable>
              </PropertyExpandableRow>
            </Match>
            <Match when={curTab() == "Enter List"}>
              <TracingSpanEnterList
                isEnd={record.end?.endDate != null} appRunId={record.record.appRunId}
                parentSpanTId={record.record.spanTId}/>
            </Match>
            <Match when={curTab() == "Field Record"}>
              <TracingSpanFieldList
                isEnd={record.end?.endDate != null} appRunId={record.record.appRunId}
                parentSpanTId={record.record.parentSpanTId}/>
            </Match>
          </Switch>
        </div>
        {/*<div class={"py-2 px-2 -mx-2 font-bold border-b border-b-gray-300"}>Record Property</div>*/}


        {/*  /!*<Show when={selected().record.kind == "SPAN_CREATE"}>*!/*/}
        {/*  /!*<TracingSpanFieldList*!/*/}
        {/*  /!*  isEnd={selected().span.close_info?.span_close_time != null} appRunId={appRunId}*!/*/}
        {/*  /!*  spanTId={selected().record.spanTId}/>*!/*/}
        {/*  /!*<TracingSpanEnterList*!/*/}
        {/*  /!*  isEnd={selected().span.close_info?.span_close_time != null} appRunId={appRunId}*!/*/}
        {/*  /!*  spanTId={selected().record.spanTId}/>*!/*/}
        {/*  /!*</Show>*!/*/}
        {/*  </tbody>*/}
        {/*</table>*/}
      </div>
    </div>
  )
}

function TracingSpanEnterList(all_props: {
  parentSpanTId: string,
  appRunId: string,
  isEnd: boolean,
} & HTMLAttributes<HTMLDivElement>) {
  let [props, rootProps] = splitProps(all_props, ['appRunId', 'parentSpanTId', 'isEnd']);
  let traceTreeInfo = useContext(CurTraceTreeInfo);

  let [data, actions] = useRecordsTreeLive({
    search: () => undefined,
    appRunId: () => props.appRunId,
    isEnd: () => props.isEnd,
    scene: () => "SpanEnter",
    kinds: [TracingKind.SpanEnter, TracingKind.SpanLeave],
    filter: {
      selectedLevels: [],
      selectedAppIds: [],
      selectedAppVersions: [],
      selectedNodeIds: [],
    },
    spanTId: () => null,
    parentSpanTId: () => props.parentSpanTId,
    onLivedAdded() {
      traceTreeInfo.rootContainerElement().scrollTo({
        top: traceTreeInfo.rootContainerElement().scrollHeight
      })
    }
  });
  let elementRef, fetchMoreMarketElement;
  createEffect(() => {
    if (data() != null) {
      elementRef?.lastElementChild?.scrollIntoView({
        block: 'end',
        behavior: 'smooth'
      })
    }
  })
  return (
    <Suspense fallback={<Loading/>}>
      <Show when={data() != null}>
        <div {...rootProps} class={cn("flex-grow overflow-hidden overflow-y-auto py-2", rootProps.class)}
             ref={elementRef}>
          <Show
            when={!data().more_loading && !actions.notMoreOlderData()}>
            <Button variant={'ghost'} class={"flex-shrink-0 border"} size="sm" ref={fetchMoreMarketElement}
                    onClick={async () => {
                      // let element = (fetchMoreMarketElement as HTMLElement).nextElementSibling
                      let rc = traceTreeInfo.rootContainerElement();
                      let scrollBottom = rc.scrollHeight - rc.scrollTop;
                      actions.fetchMoreOlder(data().records[0].record.id);
                      rc.scrollTo({
                        top: rc.scrollHeight - scrollBottom
                      })
                    }}>
              {t("common:loadMore")}
            </Button>
          </Show>
          <For each={data().records}>
            {item => <div
              class={"py-2 px-2 border-b hover:bg-stone-100 border-gray-100 text-sm items-center justify-between flex"}>
              <div>{item.record.recordTime?.toLocaleString()}</div>
              <Show when={item.end?.endDate != null}>
                <div>
                  {humanizeDuration((item.variant as TracingTreeRecordVariantDtoOneOf1).spanEnter.duration * 1000, durationOptions)}</div>
                <div>{item.end?.endDate?.toLocaleString()}</div>
              </Show>
            </div>}
          </For>

          <Show
            when={!data().is_end}>
            <Button variant={'ghost'} class={cn("flex-shrink-0 border", data().more_loading && "hidden")} size="sm"
                    ref={fetchMoreMarketElement}
                    onClick={async () => {
                      // let element = (fetchMoreMarketElement as HTMLElement).nextElementSibling
                      // let rc = traceTreeInfo.rootContainerElement();
                      // let scrollBottom = rc.scrollHeight - rc.scrollTop;
                      actions.fetchMore(data().records.at(-1).record.id);
                      // rc.scrollTo({
                      //   top: rc.scrollHeight - scrollBottom
                      // })
                    }}>
              {t("common:loadMore")}
            </Button>
            <Show when={data().more_loading}>
              <Loading/>
            </Show>
          </Show>
        </div>
      </Show>
    </Suspense>
  )
}


function TracingSpanFieldList(all_props: {
  parentSpanTId: string,
  appRunId: string,
  isEnd: boolean,
} & HTMLAttributes<HTMLDivElement>) {
  let [props, rootProps] = splitProps(all_props, ['appRunId', 'parentSpanTId', 'isEnd']);
  let traceTreeInfo = useContext(CurTraceTreeInfo);

  let [data, actions] = useRecordsTreeLive({
    search: () => undefined,
    appRunId: () => props.appRunId,
    isEnd: () => props.isEnd,
    scene: () => "SpanField",
    kinds: [TracingKind.SpanRecord],
    filter: {
      selectedLevels: [],
      selectedAppIds: [],
      selectedAppVersions: [],
      selectedNodeIds: [],
    },
    parentSpanTId: () => props.parentSpanTId,
    spanTId: () => null,
    onLivedAdded() {
      traceTreeInfo.rootContainerElement().scrollTo({
        top: traceTreeInfo.rootContainerElement().scrollHeight
      })
    }
  });
  let elementRef, fetchMoreMarketElement;
  createEffect(() => {
    if (data() != null) {
      elementRef?.lastElementChild?.scrollIntoView({
        block: 'end',
        behavior: 'smooth'
      })
    }
  })

  return (
    <Suspense fallback={<Loading/>}>
      <Show when={data() != null}>
        <div {...rootProps}
             class={cn("flex-grow overflow-hidden overflow-y-auto border-y-0 flex flex-col", rootProps.class)}
             ref={elementRef}>

          <Show
            when={!data().more_loading && !actions.notMoreOlderData()}>
            <Button variant={'ghost'} class={"flex-shrink-0 border"} size="sm" ref={fetchMoreMarketElement}
                    onClick={async () => {
                      // let element = (fetchMoreMarketElement as HTMLElement).nextElementSibling
                      let rc = traceTreeInfo.rootContainerElement();
                      let scrollBottom = rc.scrollHeight - rc.scrollTop;
                      actions.fetchMoreOlder(data().records[0].record.id);
                      rc.scrollTo({
                        top: rc.scrollHeight - scrollBottom
                      })
                    }}>
              {t("common:loadMore")}
            </Button>
          </Show>
          <For each={data().records}>
            {item => <PropertyExpandableRow class={"mt-1"} defaultIsExpand={true} label={`Record Fields`}
                                            tailing={<div class={"text-right text-xsm mr-2"}>
                                              {item.record.recordTime?.toLocaleString()}
                                            </div>} childrenContainerContainerClass={"table"}>
              <PropertyTable class={""}>
                <TracingFields object={item.record.fields}></TracingFields>
              </PropertyTable>
            </PropertyExpandableRow>}
          </For>

          <Show
            when={!data().is_end}>
            <Button variant={'ghost'} class={cn("flex-shrink-0 border", data().more_loading && "hidden")} size="sm"
                    ref={fetchMoreMarketElement}
                    onClick={async () => {
                      // let element = (fetchMoreMarketElement as HTMLElement).nextElementSibling
                      // let rc = traceTreeInfo.rootContainerElement();
                      // let scrollBottom = rc.scrollHeight - rc.scrollTop;
                      actions.fetchMore(data().records.at(-1).record.id);
                      // rc.scrollTo({
                      //   top: rc.scrollHeight - scrollBottom
                      // })
                    }}>
              {t("common:loadMore")}
            </Button>
            <Show when={data().more_loading}>
              <Loading/>
            </Show>
          </Show>
        </div>
      </Show>
    </Suspense>
  )
}


function ellipseStr(value: string): string {
  if (value.length > 1024 * 8) {
    return value.slice(0, 1024 * 8) + "< too long ! >..."
  } else {
    return value
  }
}

function handlePropValue(value: any): string {
  return (typeof value) == 'string' ? ellipseStr(value) : ellipseStr(JSON.stringify(value, null, 2)) ?? "NULL"
}

function TracingFields(allProps: { object: object }) {
  return (
    <For each={Object.keys(allProps.object ?? {})}>
      {key => <PropertyRow
        label={key}>{handlePropValue(allProps.object[key])}</PropertyRow>}
    </For>
  )
}

// function TracingTreeItemChildren({parentSpanTId, appRunId}: {
//   parentSpanTId: string | null,
//   appRunId: string
// }) {
//   return (
//     <TracingTreeItemList spanTId={parentSpanTId} appRunId={appRunId}></TracingTreeItemList>
//   )
// }

export interface SelectedTreeItem {
  record: TracingTreeRecordDto;
  path: TracePathItem[];
}

export interface CurSelectedTreeItemContext {
  selected: Accessor<SelectedTreeItem>,
  setSelected: Setter<SelectedTreeItem>,
  isSelected: (_key: number) => boolean,
}

function useCurSelectedTreeItem(): CurSelectedTreeItemContext {
  const context = useContext(CurSelectedTreeItem)
  if (!context) {
    throw new Error("useSelectedTreeItem: cannot find a SelectedTreeItem")
  }

  let isSelected = createSelector(() => context[0]()?.record?.record?.id, (a, b) => a != null && b != null && a == b)
  return {
    selected: context[0],
    isSelected,
    setSelected: context[1]
  }
}

function scrollToBottomIfNeed(element: HTMLElement) {
  console.log(element.scrollTop, element.scrollHeight - element.clientHeight, Math.abs(element.scrollTop - (element.scrollHeight - element.clientHeight)));
  if (Math.abs(element.scrollTop - (element.scrollHeight - element.clientHeight)) < 20) {
    return () => {
      element.scrollTo({
        top: element.scrollHeight
      })
    }
  } else {
    return () => {
    }
  }
}

// function OfflineAppRunList() {
//   return (
//     <div>
//       {moreBtn()}
//       <Key each={data().records?.filter(n => n.end != null).map((n, i) => {
//         let r = {
//           item: n,
//           defaultIsExpand: props.layer == 0 && (data().records.length - 1) == i && first,
//           i
//         };
//         if (r.defaultIsExpand) {
//           first = false;
//         }
//         return r;
//       })} by={n => n.item.record.id} fallback={<AppEmpty/>}>
//         {(n) => <TracingTreeItem isAppTable={props.isAppTable} layer={props.layer}
//                                  appRunId={props.appRunId} path={props.path}
//                                  defaultIsExpand={n().defaultIsExpand}
//                                  data={n().item} isEnd={() => n().item.end != null || props.isEnd()}/>}
//       </Key>
//     </div>
//   )
// }

function ExpandablePanel(props: {
  header: (_isExpand: Accessor<boolean>) => JSX.Element,
  defaultIsExpand?: boolean,
  children?: JSX.Element,
  onExpand?: () => void,
  headerContainerProps?: HTMLAttributes<HTMLDivElement>
}) {
  let [isExpand, setIsExpand] = createSignal(props.defaultIsExpand ?? false);
  return <>
    <div {...props.headerContainerProps} onClick={() => {
      startTransition(() => setIsExpand(n => !n)).then(() => props.onExpand?.());
    }}>
      {props.header(isExpand)}
    </div>
    <Show when={isExpand()}>
      {props.children}
    </Show>
  </>
}

function TracingTreeItemList(allProps: {
  spanTId?: string,
  appRunId?: string,
  isEnd: Accessor<boolean>,
  layer: number,
  path: TracePathItem[],
  isAppTable?: boolean,
  scrollToBottom?: boolean,
  containerInfo?: {
    fixedElement: HTMLElement,
    fixed: Accessor<boolean>
  }
} & HTMLAttributes<HTMLDivElement>) {
  let [props, forwardProps] = splitProps(allProps, ['scrollToBottom', 'appRunId', 'spanTId', 'isEnd', 'layer', 'containerInfo', 'isAppTable', 'containerInfo', 'isEnd', 'path']);

  let {rootContainerElement, filter, search} = useContext(CurTraceTreeInfo);
  let [data, actions] = useRecordsTreeLive({
    appRunId: () => props.appRunId,
    isEnd: props.isEnd,
    scene: () => "Tree",
    filter,
    search: () => (props.appRunId == null && props.isAppTable) ? undefined : search(),
    spanTId: () => null,
    parentSpanTId: () => props.spanTId ?? null,
    onLivedAdded() {
      // let apply = scrollToBottomIfNeed(rootContainerElement());
      // setTimeout(apply, 300);
    }
  });
  let [elementRef, setElementRef] = createSignal<HTMLDivElement>();


  if (props.scrollToBottom || props.layer == 0) {
    onMount(() => {
      elementRef()?.lastElementChild?.scrollIntoView({
        block: 'end',
        behavior: 'smooth'
      })
    })
  }
  let fetchMoreMarketElement!: HTMLElement;

  if (props.containerInfo != null) {

    // createEffect(() => {
    //   const bounds = createElementBounds(fetchMoreMarketElement);
    //   // console.log(props.containerInfo.fixedElement);
    //   if (props.containerInfo.fixed()) {
    //     let containerFixedElementTop = props.containerInfo.fixedElement.getBoundingClientRect().top;
    //     // console.log(`fixedElement element top: ${containerFixedElementTop}, bt: ${bounds.top}`)
    //     if ((bounds.top + 100) > containerFixedElementTop) {
    //       let _ = startLoadingMoreTransition(() => actions.fetchMore(data().records[0].record.id));
    //     }
    //   }
    // })
  }
  let isRoot = props.appRunId == null;
  let offlineAppRuns = () => data().records?.filter(n => n.end != null) ?? [];
  let elementBounds = createElementBounds(elementRef);

  let moreBtn = () =>
    <Show
      when={!isRoot && !data().more_loading && !actions.notMoreOlderData()}>
      <Button variant={'ghost'} class={"flex-shrink-0 border"} size="sm" ref={fetchMoreMarketElement}
              onClick={async () => {
                // let element = (fetchMoreMarketElement as HTMLElement).nextElementSibling
                let rc = rootContainerElement();
                let scrollBottom = rc.scrollHeight - rc.scrollTop;
                actions.fetchMoreOlder(data().records[0].record.id);
                rc.scrollTo({
                  top: rc.scrollHeight - scrollBottom
                })
              }}>
        {t("common:loadMore")}
      </Button>
    </Show>;
  let first = true;
  let appScrollEndMarker: HTMLDivElement;
  return (
    <Show when={data() != null}>
      <div {...forwardProps} class={cn("flex flex-col gap-1 pb-1.5 pt-1.5", forwardProps.class)} ref={setElementRef}>
        <Show when={data().more_loading}>
          <Loading/>
        </Show>
        {moreBtn()}
        {/*<For each={data().records.map((n, i, me) => ({*/}
        {/*  item: n,*/}
        {/*  defaultIsExpand: props.layer == 0 && me.length - 1 == i*/}
        {/*}))} by={n => n.item.record.id} fallback={<div class={"items-center justify-center p-2"}>无内容</div>}>*/}
        {/*  {(n) => <Suspense>*/}
        {/*    <TracingTreeItem isAppTable={props.isAppTable} layer={props.layer}*/}
        {/*                     appRunId={props.appRunId}*/}
        {/*                     defaultIsExpand={n.defaultIsExpand}*/}
        {/*                     data={n.item} isEnd={() => n.item.end != null || props.isEnd()}*/}
        {/*                     onMouseDown={() => setSelected(n.item)}*/}
        {/*                     selected={() => selected()?.record.id == n.item.record.id}/>*/}

        {/*  </Suspense>}*/}
        {/*</For>*/}
        <Show when={isRoot && offlineAppRuns().length > 0}>
          <ExpandablePanel headerContainerProps={{
            class: "flex-shrink-0 mb-1",
            style: {height: `${ITEM_HEIGHT}px`}
          }} onExpand={() => {
            appScrollEndMarker.scrollIntoView()
          }} header={(isExpand) => {
            return <div class={"absolute"} style={{width: elementBounds?.width ? `${elementBounds.width}px` : 'auto'}}>
              <div
                class={"flex sticky px-4 justify-between w-full border border-gray-100 bg-background z-10 items-center gap-2 leading-8 select-none hover:bg-stone-50"}
                style={{height: `${ITEM_HEIGHT}px`}}>

                <div class={"flex-grow"}></div>
                <div class="p-1 -mx-1 rounded-sm">
                  <ChevronDown class="transition-transform" classList={{
                    "rotate-180": isExpand(),
                    // "text-gray-400": props.data.record.fields[RECORD_FIELDS.empty_children] == true,
                  }} size={15}/>
                </div>
                <div class={""}>Expand App Run History ( {offlineAppRuns().length} )</div>
                <div class={"flex-grow"}></div>
              </div>
            </div>
          }}>
            <div class={"flex flex-col gap-1 pb-1.5 pt-1"}>
              {moreBtn()}
              <Key each={offlineAppRuns().map((n, i) => {
                let r = {
                  item: n,
                  defaultIsExpand: props.layer == 0 && (data().records.length - 1) == i && first,
                  i
                };
                if (r.defaultIsExpand) {
                  first = false;
                }
                return r;
              })} by={n => n.item.record.id} fallback={<AppEmpty/>}>
                {(n) => <TracingTreeItem isAppTable={props.isAppTable} layer={props.layer}
                                         appRunId={props.appRunId} path={props.path}
                                         defaultIsExpand={n().defaultIsExpand}
                                         data={n().item} isEnd={() => n().item.end != null || props.isEnd()}/>}
              </Key>
              <div ref={appScrollEndMarker}></div>
            </div>
          </ExpandablePanel>
        </Show>
        <Key each={data().records?.filter(n => isRoot ? n.end == null : true).map((n, i) => {
          let r = {
            item: n,
            defaultIsExpand: props.layer == 0 && (data().records.length - 1) == i && first,
            i
          };
          if (r.defaultIsExpand) {
            first = false;
          }
          return r;
        })} by={n => n.item.record.id} fallback={<AppEmpty/>}>
          {(n) => <TracingTreeItem isAppTable={props.isAppTable} layer={props.layer}
                                   appRunId={props.appRunId} path={props.path}
                                   defaultIsExpand={n().defaultIsExpand}
                                   data={n().item} isEnd={() => n().item.end != null || props.isEnd()}/>}
        </Key>

        <Show
          when={!data().is_end}>
          <Button variant={'ghost'} class={cn("flex-shrink-0 border", data().more_loading && "hidden")} size="sm"
                  ref={fetchMoreMarketElement}
                  onClick={() => {
                    actions.fetchMore(data().records.at(-1).record.id);
                  }}>
            {t("common:loadMore")}
          </Button>
          <Show when={data().more_loading}>
            <Loading/>
          </Show>
        </Show>
      </div>
    </Show>
  )
}

function TracingTreeItemBlock(allProps: {
  label: JSX.Element,
  labelContainerClass?: string
  valueContainerClass?: string
} & HTMLAttributes<HTMLDivElement>) {
  let [props, rootProps] = splitProps(allProps, ['label', 'labelContainerClass', 'valueContainerClass', 'children']);
  return (
    <div {...rootProps}
         class={cn("text-center border-x border-gray-100 leading-6 text-sm mt-1 min-w-[60px] max-w-[160px]", rootProps.class)}>
      <div
        class={cn("border-b border-b rounded border-gray-200 px-2", props.labelContainerClass)}>{props.label}</div>
      <div class={cn("text-xsm", props.valueContainerClass)}>{props.children}</div>
    </div>
  )
}

function createDateSignal(deltaMilliseconds: number): [Accessor<Date>, any] {
  let [date, setDate] = createSignal(new Date(Date.now() - deltaMilliseconds));
  let timeout = setInterval(() => setDate(new Date(Date.now() - deltaMilliseconds)), 1000);
  onMount(() => {
    onCleanup(() => clearInterval(timeout))
  })
  return [date, timeout]
}

function TracingTreeItemIoContent(props: {
  data: TracingTreeRecordDto,
  isEnd: Accessor<boolean>,
  deltaMilliseconds: number
}) {
  let bytesOptions: ByteSizeOptions = {
    // decimals: 2,
  };
  let [nowDate, timeout] = createDateSignal(props.deltaMilliseconds);
  let prevUpdateDate = nowDate();
  let spanData = () => (props.data.variant as TracingTreeRecordVariantDtoOneOf).spanRun
  let totalSize = createMemo(() => Number(props.data.record.fields['total_size'] ?? 0));
  let curValue = createMemo(() => {
    if (props.isEnd()) {
      clearInterval(timeout);
    }
    prevUpdateDate = untrack(nowDate);
    return Number((spanData().fields ? spanData().fields['value'] : 0) ?? 0)
  });
  let remainingSize = createMemo(() => totalSize() - curValue());
  let deltaDurationMilliSeconds = () => (!props.isEnd() ? nowDate().getTime() : (props.data.end?.endDate ?? nowDate()).getTime()) - props.data.record.recordTime.getTime();
  let avgSpeed = createMemo(() => untrack(curValue) / (deltaDurationMilliSeconds() / 1000));
  let curSpeed = createMemo(({prevDate, prevValue, value}) => {
    let now = new Date(Date.now());
    let cv = curValue();
    let deltaDate = (now.getTime() - prevDate.getTime());
    if (deltaDate < 1000) {
      return {prevDate, prevValue, value}
    }
    let r = (cv - prevValue) / (deltaDate / 1000);
    if (!Number.isFinite(r) || Number.isNaN(r) || r == null) {
      r = 0;
    }
    return {
      prevDate: now,
      prevValue: cv,
      value: r
    };
  }, {
    prevDate: new Date(Date.now()),
    prevValue: curValue(),
    value: curValue()
  });
  return (
    <>
      <TracingTreeItemBlock label={props.data.record.name.substring(3, props.data.record.name.length - 1)}>
        <span class={"font-bold"}>{props.data.record.fields['name']}</span>
      </TracingTreeItemBlock>
      {/*<div>{props.data.record.fields['type']}</div>*/}
      <Show when={!props.isEnd()} fallback={<TracingTreeItemBlock label={"总耗时"}>
        {(props.data.end?.endDate ?? props.data.end?.exceptionEnd) ? humanizeDuration((props.data.end?.endDate ?? props.data.end?.exceptionEnd).getTime() - props.data.record.recordTime.getTime(), durationOptions) : NULL_STR}
      </TracingTreeItemBlock>
      }>
        <TracingTreeItemBlock label={"已耗时"}>
          {humanizeDuration(nowDate().getTime() - props.data.record.recordTime.getTime(), durationOptions)}
        </TracingTreeItemBlock>
      </Show>
      <Show when={props.data.record.fields['total_size'] != null} fallback={<>
        <TracingTreeItemBlock label={"进度"}>
          {byteSize(curValue(), bytesOptions).toString()}
        </TracingTreeItemBlock>
      </>}>
        <TracingTreeItemBlock labelContainerClass={!props.isEnd() && "bg-blue-600/80 text-muted"}
                              label={!props.isEnd() ?
                                "进行中" : "已结束"}>
          <Show when={!props.isEnd() || curValue() != totalSize()}
                fallback={byteSize(totalSize(), bytesOptions).toString()}>
            {byteSize(curValue(), bytesOptions).toString()} / {byteSize(totalSize(), bytesOptions).toString()}
          </Show>
        </TracingTreeItemBlock>
        <Show when={!props.isEnd() && curSpeed().value != 0}>
          <TracingTreeItemBlock label={"预计剩余"}>
            {humanizeDuration((remainingSize() / curSpeed().value) * 1000, durationOptions)}
          </TracingTreeItemBlock>
        </Show>
      </Show>
      <Show when={!props.isEnd()}>
        <TracingTreeItemBlock label={"空闲时间"}>
          {humanizeDuration(nowDate().getTime() - prevUpdateDate.getTime(), durationOptions).toString()}
        </TracingTreeItemBlock>
        <TracingTreeItemBlock label={"当前速度"}>
          {byteSize(curSpeed().value, bytesOptions).toString()}/s
        </TracingTreeItemBlock>
      </Show>
      <TracingTreeItemBlock label={"平均速度"}>
        {byteSize(avgSpeed(), bytesOptions).toString()}/s
      </TracingTreeItemBlock>

      {/*<div>idle duration</div>*/}
    </>
  )
}

function TracingTreeItemBase(allProps: {
  data: TracingRecordDto,
  leading?: (_n: () => JSX.Element) => JSX.Element,
  selected: Accessor<boolean>,
  needFixed: Accessor<boolean>,
  forceError?: Accessor<boolean>
} & HTMLAttributes<HTMLDivElement>) {
  let [props, rootProps] = splitProps(allProps, ['data', 'selected', 'needFixed', 'children', 'forceError', 'leading']);

  let defaultLeading = () => <>
    <TracingLevelIcon
      level={(props.forceError && props.forceError()) ? TracingLevel.Error : props.data.level}/>
    <Show when={!EXPANDABLE_KINDS.includes(props.data.kind)}>
      <div class="p-1 flex-shrink-0 -mx-1"
           style={{color: getLevelColor(props.data.level)}}>
        <Dot size={15}/>
      </div>
    </Show>
  </>;
  let leading: JSX.Element = props.leading ? props.leading(defaultLeading) : defaultLeading();
  return <div
    {...rootProps}
    class={cn("flex bg-background z-10 items-center gap-2 px-1 leading-8 select-none", rootProps.class, props.selected() ? "bg-stone-100" : "hover:bg-stone-50", props.needFixed() && "border border-stone-200 z-30")}>
    {leading}
    {props.children}
    <div class="flex-grow self-stretch"/>
    <div class={"whitespace-nowrap flex-shrink-0 select-none"}>{props.data.recordTime?.toLocaleString()}</div>
    {/* <div>{JSON.stringify(props.data.fields)}</div> */}
  </div>
}


function getDeltaMilliseconds(data: TracingTreeRecordDto): number {
  if ((data.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun?.runElapsed != null) {
    return Date.now() - data.record.recordTime.getTime() - (data.variant as TracingTreeRecordVariantDtoOneOf).spanRun.runElapsed;
  } else if ((data.variant as TracingTreeRecordVariantDtoOneOf2)?.appRun?.runElapsed != null) {
    return Date.now() - data.record.recordTime.getTime() - (data.variant as TracingTreeRecordVariantDtoOneOf2).appRun.runElapsed;
  } else {
    return 0
  }
}

function TracingTreeItemFields(props: { fields: any }) {
  return (
    <For
      each={Object.entries(props.fields)
        .filter((n: [string, any]) => (typeof n[1]) != 'object' && !n[0].startsWith("__data"))
        .map(n => {
          let value = n[1]?.toString().trim();
          if (value.length > 80) {
            value = "...";
          }
          return [n[0], value];
        })
      }>
      {([name, value]) => <div
        class=" bg-stone-100 shadow-sm flex-shrink-0 border flex rounded-sm overflow-hidden text-xsm leading-none text-nowrap select-none">
        <div class=" p-1">{name}</div>
        <Show when={value != ""}>
          <div class="bg-background border-l p-1">{value}</div>
        </Show>
      </div>}
    </For>
  )
}

let ITEM_HEIGHT = 32;

function TracingTreeItem(allProps: {
  data: TracingTreeRecordDto,
  isEnd: Accessor<boolean>,
  path: TracePathItem[],
  appRunId: string,
  isAppTable: boolean,
  defaultIsExpand?: boolean,
  layer: number
} & HTMLAttributes<HTMLDivElement>) {
  let [props, rootProps] = splitProps(allProps, ['data', 'defaultIsExpand', 'appRunId', 'layer', 'isAppTable', 'layer', 'isEnd', 'class', 'path']);
  // let [isExpand, setIsExpand] = makePersisted(createSignal(props.defaultIsExpand || (getFlags(props.data.record, AUTO_EXPAND) ?? false)), {
  //   name: `ed-${props.data.record.id}`,
  //   storage: useSearchParamStorage(),
  //   serialize: n => n ? "1" : undefined,
  //   deserialize: n => n == "1"
  // });
  let [isExpand, setIsExpand] = createSignal(props.defaultIsExpand || (getFlags(props.data.record, AUTO_EXPAND) ?? false));
  let [target, setTarget] = createSignal<HTMLElement>();
  let [itemTarget, setItemTarget] = createSignal<HTMLElement>();
  let needFixed = () => false;
  let hasChildren = props.data.record.kind == TracingKind.AppStart || (props.data.record.spanTId != null && props.data.record.kind == TracingKind.SpanCreate);
  let fixGap = 4;
  let traceTreeInfo = useContext(CurTraceTreeInfo);
  let [curTracePath, setCurTracePath] = useContext(CurTracePath);
  let rootContainerBounds = createElementBounds(traceTreeInfo.rootContainerElement());

  let tLayer = () => props.layer - curTracePath().length;
  if (hasChildren) {
    let targetBounds = createElementBounds(target);
    let date = Date.now();
    needFixed = createMemo((prevValue) => {
      if (target == null) {
        return false;
      }
      if (isExpand()) {
        let offset: number = (ITEM_HEIGHT + fixGap) * tLayer() + rootContainerBounds.top;
        if (targetBounds.top < offset) {
          if (((Date.now() - date) > 300) && (offset - targetBounds.top + ITEM_HEIGHT) > targetBounds.height) {
            return false;
          }
          if (prevValue == false) {
            date = Date.now();
          }
          return true
        } else {
          return false
        }
      } else {
        return false
      }
    });
    createEffect(() => {
      if (needFixed()) {
        let offset = rootContainerBounds.top;
        target().style.paddingTop = `${ITEM_HEIGHT}px`;
        itemTarget().style.position = "fixed";
        itemTarget().style.top = `${offset + tLayer() * (ITEM_HEIGHT + fixGap)}px`;
        itemTarget().style.width = `${targetBounds.width}px`;
      } else {
        target().style.paddingTop = '0px';
        itemTarget().style.position = "initial";
        itemTarget().style.width = `auto`;
      }
    });
  }
  let appRunId = () => props.appRunId ?? (props.data.record.kind == TracingKind.AppStart ? props.data.record.appRunId : null);

  let typed_span_name: Accessor<string | null> = () => props.data.record.kind == TracingKind.SpanCreate && props.data.record.name.startsWith("[t:") && props.data.record.name.endsWith("]") ? props.data.record.name.substring(3, props.data.record.name.length - 1) : null;
  let expand = () => {
    if (typed_span_name() != null) {
      return;
    }
    // let apply = scrollToBottomIfNeed(traceTreeInfo.rootContainerElement());
    // let offset = traceTreeInfo.rootContainerElement().getBoundingClientRect().top - target().getBoundingClientRect().top;
    setIsExpand(n => !n);
    if (needFixed()) {
      // let top = target().offsetTop - traceTreeInfo.rootContainerElement().offsetTop;
      setTimeout(() => {
        itemTarget().scrollIntoView({
          behavior: "smooth"
        })
        // traceTreeInfo.rootContainerElement()?.scrollTo({
        //   // top: traceTreeInfo.rootContainerElement().offsetTop - target().offsetTop - offset
        //   top,
        //   behavior: 'smooth'
        //   // top: 0
        // });

      }, 300);
    } else {
      // setTimeout(() => {
      //   apply();
      // }, 100)
    }
  };
  let curSelected = useCurSelectedTreeItem();
  let selected = () => curSelected.isSelected(props.data.record.id);
  return (
    <div ref={setTarget}
         class={cn("border border-gray-100", (selected() || ((props.data.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun?.relatedEvents?.some(n => curSelected.isSelected(n.id)))) && "ring-1 ring-offset-4 z-20 ring-blue-600 shadow-blue-600", props.class)}>
      <ContextMenu>
        <ContextMenuTrigger>
          <TracingTreeItemBase ref={setItemTarget} data={props.data.record} onDblClick={() => expand()}
                               class={"border border-transparent"}
                               style={{'max-width': rootContainerBounds ? `${rootContainerBounds.width}px` : 'initial', ...(typed_span_name() == null ? {height: `${ITEM_HEIGHT}px`} : {})}}
                               onClick={/*getFlags(props.data.record, EMPTY_CHILDREN) ? null : */(e) => {
                                 curSelected.setSelected({record: props.data, path: props.path});
                                 if (!e.altKey) {
                                   return;
                                 }
                                 expand();
                               }}
                               selected={selected} needFixed={needFixed} {...rootProps}>
            <Switch fallback={<>
              <Show
                when={EXPANDABLE_KINDS.includes(props.data.record.kind) && typed_span_name() == null}>
                <div classList={{
                  "hover:bg-stone-200": true
                  // "hover:bg-stone-200": props.data.record.fields[RECORD_FIELDS.empty_children] != true
                }} onDblClick={e => e.stopPropagation()}
                     onClick={/*getFlags(props.data.record, EMPTY_CHILDREN) ? null : */(e) => {
                       setIsExpand(n => !n);
                       e.stopPropagation();
                     }}
                     class="p-1 -mx-1 rounded-sm cursor-pointer">
                  <ChevronRight class="transition-transform" classList={{
                    "rotate-90": isExpand(),
                    // "text-gray-400": props.data.record.fields[RECORD_FIELDS.empty_children] == true,
                  }} size={15}/>
                </div>
              </Show>
              <div
                class={"text-ellipsis flex-shrink-0 overflow-hidden whitespace-nowrap select-none"}>{props.data.record.name}</div>
              <Show when={props.data.record.kind == TracingKind.Event && props.data.record.repeatedCount != null}>
                <div title="重复次数"
                     class="shadow-sm border p-1 text-primary flex rounded-sm overflow-hidden text-xsm leading-none select-none">
                  x{props.data.record.repeatedCount}
                </div>
              </Show>
              <TracingTreeItemFields fields={{
                ...(props.data.record.fields ?? {}),
                ...((props.data.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun?.fields ?? {})
              }}></TracingTreeItemFields>
              {/*{JSON.stringify(props.data.record.fields)}*/}
              <Show
                when={[TracingKind.SpanCreate, TracingKind.AppStart as TracingKind].includes(props.data.record.kind)}>
                <TracingRunTimer
                  badEnd={props.data.end?.exceptionEnd != null}
                  endTimeMs={() => {
                    if (!props.isEnd()) {
                      return null;
                    }
                    return props.data.end?.endDate.getTime() ?? props.data.end?.exceptionEnd?.getTime()
                  }}
                  deltaMilliseconds={getDeltaMilliseconds(props.data)}
                  startTimeMs={props.data.record.recordTime.getTime()}
                />
              </Show>
            </>}>
              <Match when={['AsyncRead', 'AsyncWrite'].includes(typed_span_name())}>
                <TracingTreeItemIoContent isEnd={props.isEnd} data={props.data}
                                          deltaMilliseconds={getDeltaMilliseconds(props.data)}/>
              </Match>
              {/*<Match when={['Stream'].includes(typed_span_name())}>*/
              }
              {/*  /!*<TracingTreeItemIoContent data={props.data}/>*!/*/
              }
              {/*  Stream*/
              }
              {/*</Match>*/
              }
            </Switch>
          </TracingTreeItemBase>
        </ContextMenuTrigger>
        <ContextMenuContent class={"z-40 outline-none"}>
          <ContextMenuItem
            onClick={() => setCurTracePath(EXPANDABLE_KINDS.includes(props.data.record.kind) ? [...props.path, {
              record: props.data,
              path: props.path
            }] : props.path)}>
            <LogIn size={16} class={"mr-3"} strokeWidth={1}></LogIn>
            Go Here
            {/*<ContextMenuShortcut>⌘+T</ContextMenuShortcut>*/}
          </ContextMenuItem>
          <Show when={EXPANDABLE_KINDS.includes(props.data.record.kind)}>
            <ContextMenuItem
              onClick={() => setIsExpand(n => !n)}>
              <Show when={isExpand()} fallback={<>
                <UnfoldVertical size={16} class={"mr-3"} strokeWidth={1}></UnfoldVertical>
                Expand
              </>}>
                <FoldVertical size={16} class={"mr-3"} strokeWidth={1}></FoldVertical>
                Collapse
              </Show>
              {/*<ContextMenuShortcut>⌘+T</ContextMenuShortcut>*/}
            </ContextMenuItem>
          </Show>
        </ContextMenuContent>
      </ContextMenu>
      <Show
        when={!needFixed() && (props.data.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun?.relatedEvents?.length > 0}>
        {/*<div class="mx-1 border border-t-0 border-gray-200 rounded-sm overflow-hidden">*/}
        <div class="border-l-2 mb-1 ml-1 -mt-1 pl-1 pt-1.5 relative left-[0.5px] top-[-1px]"
             style={{"border-color": getLevelColor(props.data.record.level)}}>
          <For each={(props.data.variant as TracingTreeRecordVariantDtoOneOf)?.spanRun.relatedEvents}>
            {n =>
              <TracingTreeItemBase data={n} onDblClick={() => expand()} class={"text-sm"}
                                   style={{'max-width': rootContainerBounds ? `${rootContainerBounds.width}px` : 'initial', ...(typed_span_name() == null ? {height: `${ITEM_HEIGHT}px`} : {})}}
                                   selected={() => curSelected.isSelected(n.id)} needFixed={needFixed} onClick={() => {
                curSelected.setSelected({
                  record: {record: n, variant: null, end: null},
                  path: [...props.path, props.data]
                });
              }} leading={() => <>
                {/*<TracingLevelIcon class={""} level={props.data.record.level}/>*/}
                <TracingLevelIcon class={"ml-1.5 opacity-50"} level={n.level}/>
                <div
                  class={"leading-none p-1 bg-stone-50 border text-primary text-xsm rounded-sm px-1 py-1"}>
                  {n.fields[RECORD_FIELDS.related_name] ?? "NULL"}
                </div>
                {/*{p()}*/}
              </>}>
                <div
                  class={"text-ellipsis overflow-hidden whitespace-nowrap select-none"}>{n.name}</div>
                <Show when={n.kind == TracingKind.Event && n.repeatedCount != null}>
                  <div title="重复次数"
                       class="shadow-sm border p-1 text-primary flex rounded-sm overflow-hidden text-xsm leading-none select-none">
                    x{n.repeatedCount}
                  </div>
                </Show>
                <TracingTreeItemFields fields={n.fields}></TracingTreeItemFields>
              </TracingTreeItemBase>}
          </For>
        </div>
      </Show>
      <Show when={hasChildren && isExpand()}>
        <div class="ml-4">
          <Suspense fallback={<Loading/>}>
            <Switch fallback={<>
              <TracingTreeItemList scrollToBottom={props.defaultIsExpand} path={[...props.path, {
                record: props.data
              }]}
                                   appRunId={appRunId()}
                                   containerInfo={{fixedElement: itemTarget(), fixed: needFixed}}
                                   layer={props.layer + 1}
                                   isEnd={() => props.isEnd() || props.data.end != null}
                                   spanTId={props.data.record.spanTId ?? null}/>
            </>}>
              <Match when={props.isAppTable && props.data.record.kind == TracingKind.AppStart}>
                <TracingRecordTable
                  appRunId={appRunId()}/>
              </Match>
            </Switch>
          </Suspense>
        </div>
      </Show>
    </div>
  )
}

function TracingLevelIcon(allProps: { level?: TracingLevel | null } & HTMLAttributes<HTMLDivElement>) {
  const [props, rootProps] = splitProps(allProps, ["children", 'level', 'class'])

  return (
    <div {...rootProps} style={{background: getLevelColor(props.level)}}
         class={cn("w-[2px] flex-shrink-0 rounded-sm my-1 self-stretch", props.class)}/>
  )
}

function TracingRunTimer(props: {
  endTimeMs: Accessor<number | null>,
  startTimeMs: number,
  deltaMilliseconds: number
  badEnd: boolean
}) {
  let deltaMilliseconds = props.deltaMilliseconds;
  let getRunElapsed = () => Date.now() - deltaMilliseconds - props.startTimeMs;
  let [runElapsed, setRunElapsed] = createSignal(getRunElapsed());
  let timer;
  createEffect(() => {
    if (props.endTimeMs() != null && timer != null) {
      clearInterval(timer);
    } else {
      timer = setInterval(() => {
        return setRunElapsed(getRunElapsed());
      }, 1000);
    }
    onCleanup(() => clearInterval(timer))
  })
  return (
    <div title="用时"
         class="bg-stone-100 shadow-sm border flex rounded-sm overflow-hidden text-xsm leading-none select-none">
      <div
        class={cn("p-1", props.endTimeMs() != null ? "bg-stone-100 text-primary" : props.badEnd ? "bg-red-400 text-muted" : "bg-blue-600/80 text-muted")}>{props.endTimeMs() != null ? props.badEnd ? 'exception' : 'end' : 'run'}</div>
      <div
        class="bg-background border-l p-1 select-none">{humanizeDuration(props.endTimeMs() ? (props.endTimeMs() - props.startTimeMs) : runElapsed(), durationOptions)}</div>
    </div>
  )
}

function TracingRecordTable(all_props: {
  appRunId?: string
  spanTId?: string,
} & HTMLAttributes<HTMLTableElement>) {
  let [props, rootProps] = splitProps(all_props, ['appRunId', 'spanTId']);

  let {rootContainerElement, filter, search} = useContext(CurTraceTreeInfo);
  let curSelected = useCurSelectedTreeItem();
  let [data] = useRecordsTreeLive({
    isEnd: () => curSelected.selected()?.record.end != null,
    appRunId: () => props.appRunId,
    scene: () => null,
    filter,
    search,
    spanTId: () => null,
    parentSpanTId: () => props.spanTId,
    onLivedAdded() {
      // if (filter.scrollToBottomWhenAdded) {
      //   rootContainerElement().scrollTo({
      //     top: rootContainerElement().scrollHeight
      //   })
      // }
    }
  });
  let elementRef!: HTMLTableSectionElement;

  onMount(() => {
    elementRef?.lastElementChild?.scrollIntoView({
      block: 'end',
      behavior: 'smooth'
    })
  })

  return (
    <Show when={data()}>
      <table {...rootProps}
             class={cn("border-collapse w-full flex flex-col overflow-y-auto border-slate-500", rootProps.class)}>
        <tbody ref={elementRef} class={"w-full"}>
        <For each={data().records} fallback={<AppEmpty/>}>
          {(item) => (
            <TracingRecordTr onClick={() => curSelected.setSelected({record: item, path: []})}
                             selected={() => curSelected.isSelected(item.record.id)}
                             item={item.record}/>
          )}
        </For>
        </tbody>
      </table>
    </Show>
  )
}

function TracingRecordTr(allProps: {
  item: TracingRecordDto,
  selected: Accessor<boolean>,
} & HTMLAttributes<HTMLTableRowElement>) {
  let [props, rootProps] = splitProps(allProps, ['item', 'selected']);
  return (
    <tr {...rootProps} class={cn("leading-8", rootProps.class)}
        classList={{
          "bg-stone-100": allProps.selected(),
          "hover:bg-stone-50": !props.selected(), ...rootProps.classList
        }}>
      <td class="whitespace-nowrap"
          style={{background: getLevelColor(props.item.level), width: "2px"}}></td>
      <td class="w-[1%] pl-2 pr-2 whitespace-nowrap">{props.item.recordTime?.toLocaleString()}</td>
      <td class="w-[1%] pr-2 whitespace-nowrap">{props.item.kind}</td>
      <td class="text-left">
        <div class={"text-nowrap text-ellipsis overflow-hidden max-w-[500px]"}>{props.item.name}</div>
      </td>
      <td class="w-[1%] pr-2 whitespace-nowrap">{props.selected()}</td>
      <td class="w-[1%] pr-2 whitespace-nowrap">{props.item.target}</td>
    </tr>
  )
}
