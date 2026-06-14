import { Injectable, signal, computed, inject } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { firstValueFrom, Observable } from 'rxjs';
import {
  NodesService,
  RecordsService,
  type NodesPageDto,
  type TracingTreeRecordDto,
  type TracingRecordDto,
  type CursorInfo,
  TracingKind,
  TracingLevel,
  TracingRecordScene,
} from '../../../api';
import { BASE_URL, ALL_LEVELS } from '../../utils/constants';

export type ShowMode = 'Tree' | 'Flatten';
export const SHOW_MODES: ShowMode[] = ['Tree', 'Flatten'];

export interface TracingTreeFilter {
  selectedLevels: TracingLevel[];
  selectedAppIds: string[];
  selectedAppVersions: string[];
  selectedNodeIds: string[];
  showMode: ShowMode;
  scrollToBottomWhenAdded: boolean;
}

export interface TracePathItem {
  record: TracingTreeRecordDto;
}

export interface SelectedTreeItem {
  record: TracingTreeRecordDto;
  path: TracePathItem[];
}

export interface RecordsTreeData {
  records: TracingTreeRecordDto[];
  moreLoading: boolean;
  isEnd: boolean;
}

const COUNT = 50;

@Injectable()
export class TracesService {
  private readonly http = inject(HttpClient);
  private readonly nodesService = inject(NodesService);
  private readonly recordsService = inject(RecordsService);

  // Filter state
  readonly filter = signal<TracingTreeFilter>({
    selectedLevels: [...ALL_LEVELS],
    selectedAppIds: [],
    selectedNodeIds: [],
    selectedAppVersions: [],
    showMode: 'Tree',
    scrollToBottomWhenAdded: false,
  });

  // Node page data
  readonly nodesPage = signal<NodesPageDto | null>(null);
  readonly nodesPageLoading = signal(false);

  // Search
  readonly search = signal<string | undefined>(undefined);
  readonly nodeSearch = signal('');

  // Selected trace path (for drilling down)
  readonly tracePath = signal<TracePathItem[]>([]);

  // Selected tree item
  readonly selectedItem = signal<SelectedTreeItem | null>(null);

  // Parent span TId from trace path
  readonly spanTId = computed(() => {
    const path = this.tracePath();
    return path.length > 0 ? path[path.length - 1].record.record.span_t_id ?? null : null;
  });

  readonly isEnd = computed(() => {
    const path = this.tracePath();
    return path.length > 0 ? path[path.length - 1].record.end != null : false;
  });

  readonly appRunId = computed(() => {
    const path = this.tracePath();
    return path.length > 0 ? path[0].record.record.app_run_id ?? null : null;
  });

  // Tree records data store
  readonly treeData = signal<RecordsTreeData | null>(null);
  readonly notMoreOlderData = signal(false);

  // Multi-selection helpers
  isLevelSelected(level: TracingLevel): boolean {
    return this.filter().selectedLevels.includes(level);
  }

  toggleLevel(level: TracingLevel): void {
    this.filter.update(f => {
      const idx = f.selectedLevels.indexOf(level);
      const levels = [...f.selectedLevels];
      if (idx >= 0) {
        levels.splice(idx, 1);
      } else {
        levels.push(level);
      }
      return { ...f, selectedLevels: levels };
    });
  }

  isAppSelected(appId: string): boolean {
    return this.filter().selectedAppIds.includes(appId);
  }

  toggleApp(appId: string): void {
    this.filter.update(f => {
      const idx = f.selectedAppIds.indexOf(appId);
      const ids = [...f.selectedAppIds];
      if (idx >= 0) {
        ids.splice(idx, 1);
      } else {
        ids.push(appId);
      }
      return { ...f, selectedAppIds: ids, selectedNodeIds: [] };
    });
  }

  isNodeSelected(nodeId: string): boolean {
    return this.filter().selectedNodeIds.includes(nodeId);
  }

  toggleNode(nodeId: string): void {
    this.filter.update(f => {
      const idx = f.selectedNodeIds.indexOf(nodeId);
      const ids = [...f.selectedNodeIds];
      if (idx >= 0) {
        ids.splice(idx, 1);
      } else {
        ids.push(nodeId);
      }
      return { ...f, selectedNodeIds: ids };
    });
  }

  setShowMode(mode: ShowMode): void {
    this.filter.update(f => ({ ...f, showMode: mode }));
  }

  // Fetch nodes page
  async loadNodesPage(): Promise<void> {
    this.nodesPageLoading.set(true);
    try {
      const filter = this.filter();
      const result = await firstValueFrom(
        this.nodesService.nodesPage(undefined, filter.selectedAppIds.map(id => [id, null]))
      );
      this.nodesPage.set(result);
    } finally {
      this.nodesPageLoading.set(false);
    }
  }

  // Fetch tree records
  async loadTreeRecords(options: {
    appRunId?: string | null;
    spanTId?: string | null;
    parentSpanTId?: string | null;
    isEnd: boolean;
    scene: TracingRecordScene | null;
    kinds?: TracingKind[];
  }): Promise<RecordsTreeData | null> {
    const filter = this.filter();
    const param: any = {
      count: COUNT + 1,
      kinds: options.kinds ?? [],
      levels: filter.selectedLevels,
      app_build_ids: filter.selectedAppIds.map(id => [id, null]),
      node_ids: filter.selectedNodeIds,
      parent_span_t_ids: [options.parentSpanTId ?? 0],
      search: this.search(),
      scene: options.scene,
    };

    if (options.appRunId == null && options.scene === 'Tree') {
      param.app_run_ids = [];
      param.kinds = [TracingKind.AppStart, TracingKind.AppStop];
      param.parent_id = undefined;
      param.levels = undefined;
    } else {
      if (options.appRunId != null) {
        param.app_run_ids = [options.appRunId];
      }
    }

    if (options.spanTId != null) {
      param.parent_id = undefined;
      param.fields = [{
        name: '__data.span_t_id',
        op: 'Equal',
        value: options.spanTId,
      }];
    }

    try {
      const records = await firstValueFrom(this.recordsService.listTreeRecords(
        param.cursor, param.count, param.search, param.scene,
        param.app_build_ids, param.app_run_ids, param.node_ids,
        param.parent_id, param.parent_span_t_ids, param.start_time,
        param.end_time, param.kinds, param.span_ids, param.targets,
        param.name, param.fields, param.levels
      ));

      let isEnd = false;
      if (records.length <= COUNT) {
        isEnd = true;
      }
      if (records.length === COUNT + 1) {
        records.shift();
      }

      return { records, moreLoading: false, isEnd };
    } catch {
      return null;
    }
  }

  // SSE subscription for live updates
  createEventSource(path: string, params: string): EventSource {
    return new EventSource(`${BASE_URL}${path}?${params}`);
  }

  // Navigate trace path
  navigateToPath(path: TracePathItem[]): void {
    this.tracePath.set(path);
  }

  goToItem(item: SelectedTreeItem): void {
    const { record, path } = item;
    if (this.isExpandableKind(record.record.kind)) {
      this.tracePath.set([...path, { record }]);
    } else {
      this.tracePath.set(path);
    }
  }

  isExpandableKind(kind: TracingKind): boolean {
    return kind === TracingKind.SpanCreate || kind === TracingKind.AppStart;
  }
}
