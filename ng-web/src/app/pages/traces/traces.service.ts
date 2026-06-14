import { Injectable, signal, computed, inject, resource } from '@angular/core';
import { firstValueFrom } from 'rxjs';
import {
  NodesService,
  RecordsService,
  type NodesPageDto,
  type TracingTreeRecordDto,
  TracingKind,
  TracingLevel,
  TracingRecordScene,
} from '../../../api';
import { ALL_LEVELS } from '../../utils/constants';

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

/** Typed shape for the tree-records query — isolates the 17-param generated API. */
interface ListTreeRecordsParams {
  cursor?: unknown;
  count?: number | null;
  search?: string | null;
  scene?: unknown;
  appBuildIds?: unknown[] | null;
  appRunIds?: unknown[] | null;
  nodeIds?: unknown[] | null;
  parentId?: string | null;
  parentSpanTIds?: unknown[] | null;
  startTime?: string | null;
  endTime?: string | null;
  kinds?: unknown[] | null;
  spanIds?: unknown[] | null;
  targets?: unknown[] | null;
  name?: unknown[] | null;
  fields?: unknown[] | null;
  levels?: unknown[] | null;
}

@Injectable()
export class TracesService {
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

  // Node page data — reloads automatically when filter changes
  readonly nodesPageResource = resource<NodesPageDto | undefined, TracingTreeFilter>({
    params: () => this.filter(),
    loader: async ({ params: filter }) => {
      return await firstValueFrom(
        this.nodesService.nodesPage(
          undefined,
          filter.selectedAppIds.map((id: string) => [id, null]),
        ),
      );
    },
    defaultValue: undefined,
  });

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
    const params: ListTreeRecordsParams = {
      count: COUNT + 1,
      kinds: options.kinds ?? [],
      levels: filter.selectedLevels,
      appBuildIds: filter.selectedAppIds.map(id => [id, null]),
      nodeIds: filter.selectedNodeIds,
      parentSpanTIds: [options.parentSpanTId ?? 0],
      search: this.search(),
      scene: options.scene,
    };

    if (options.appRunId == null && options.scene === 'Tree') {
      params.appRunIds = [];
      params.kinds = [TracingKind.AppStart, TracingKind.AppStop];
      params.levels = undefined;
    } else if (options.appRunId != null) {
      params.appRunIds = [options.appRunId];
    }

    if (options.spanTId != null) {
      params.fields = [{
        name: '__data.span_t_id',
        op: 'Equal',
        value: options.spanTId,
      }];
    }

    const records = await firstValueFrom(this.listTreeRecordsSafe(params));

    let isEnd = false;
    if (records.length <= COUNT) {
      isEnd = true;
    }
    if (records.length === COUNT + 1) {
      records.shift();
    }

    return { records, moreLoading: false, isEnd };
  }

  private listTreeRecordsSafe(params: ListTreeRecordsParams) {
    return this.recordsService.listTreeRecords(
      params.cursor, params.count, params.search, params.scene,
      params.appBuildIds, params.appRunIds, params.nodeIds,
      params.parentId, params.parentSpanTIds, params.startTime,
      params.endTime, params.kinds, params.spanIds, params.targets,
      params.name, params.fields, params.levels,
    );
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
