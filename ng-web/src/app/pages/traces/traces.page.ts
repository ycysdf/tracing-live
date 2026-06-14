import { Component, signal, computed, inject, effect, OnDestroy } from '@angular/core';
import { FormsModule } from '@angular/forms';
import {
  NodesService,
  TracingKind,
  TracingLevel,
  TracingRecordScene,
  type NodesPageDto,
  type TracingTreeRecordDto,
  type AppNodeRunDto,
  type TracingRecordDto,
} from '../../../api';
import { TranslatePipe } from '@ngx-translate/core';
import { cn } from '../../utils/cn';
import {
  ALL_LEVELS,
  BASE_URL,
  EXPANDABLE_KINDS,
  getLevelColor,
  RECORD_FIELDS,
  NULL_STR,
} from '../../utils/constants';
import { LoadingComponent } from '../../components/loading.component';
import { LoadingPanelComponent } from '../../components/loading-panel.component';
import { EmptyComponent } from '../../components/empty.component';
import { TracesService, type ShowMode, SHOW_MODES, type TracePathItem, type SelectedTreeItem, type RecordsTreeData } from './traces.service';
import { NodeItemComponent } from './node-item.component';
import { TracePathComponent } from './trace-path.component';
import { DetailPanelComponent } from './detail-panel.component';
import { TreeItemComponent } from './tree-item.component';

function formatDuration(ms: number): string {
  if (ms < 1000) return ms + 'ms';
  const s = Math.floor(ms / 1000);
  if (s < 60) return s + 's';
  const m = Math.floor(s / 60);
  if (m < 60) return m + 'm ' + (s % 60) + 's';
  const h = Math.floor(m / 60);
  return h + 'h ' + (m % 60) + 'm';
}

@Component({
  selector: 'app-traces-page',
  templateUrl: './traces.page.html',
  providers: [TracesService],
  imports: [
    FormsModule,
    TranslatePipe,
    LoadingComponent,
    LoadingPanelComponent,
    EmptyComponent,
    NodeItemComponent,
    TracePathComponent,
    DetailPanelComponent,
    TreeItemComponent,
  ],
})
export class TracesPage implements OnDestroy {
  readonly service = inject(TracesService);

  readonly filter = this.service.filter;
  readonly nodesPage = this.service.nodesPage;
  readonly nodesPageLoading = this.service.nodesPageLoading;
  readonly tracePath = this.service.tracePath;
  readonly selectedItem = this.service.selectedItem;
  readonly search = this.service.search;
  readonly nodeSearch = this.service.nodeSearch;
  readonly isEnd = this.service.isEnd;
  readonly spanTId = this.service.spanTId;
  readonly appRunId = this.service.appRunId;

  readonly treeData = signal<RecordsTreeData | null>(null);
  readonly treeLoading = signal(false);
  public now = new Date();

  readonly ALL_LEVELS = ALL_LEVELS;
  readonly SHOW_MODES = SHOW_MODES;
  readonly EXPANDABLE_KINDS = EXPANDABLE_KINDS;

  private eventSource: EventSource | null = null;

  constructor() {
    this.service.loadNodesPage();

    effect(() => {
      // React to filter changes to reload nodes
      this.filter();
      this.service.loadNodesPage();
    });

    // Load initial tree data
    this.loadTreeData();
  }

  ngOnDestroy(): void {
    this.closeEventSource();
  }

  // Computed filtered nodes
  readonly filteredNodes = computed(() => {
    const nodes = this.nodesPage()?.nodes ?? [];
    const search = (this.nodeSearch() ?? '').trim().toLowerCase();
    if (!search) return nodes;
    return nodes.filter(n =>
      n.node_id?.toLowerCase().includes(search) ||
      n.app_run_id?.toLowerCase().includes(search) ||
      n.app_build_ids?.some(b => b?.some(v => String(v ?? '').toLowerCase().includes(search))) ||
      Object.keys(n.data ?? {}).some(key =>
        String(n.data?.[key] ?? '').toLowerCase().includes(search)
      )
    );
  });

  getLevelColor(level: TracingLevel | null | undefined): string {
    return getLevelColor(level);
  }

  formatDuration(ms: number): string {
    return formatDuration(ms);
  }

  formatDate(date: Date | string | undefined): string {
    if (!date) return '';
    return new Date(date).toLocaleString();
  }

  setSearch(value: string): void {
    this.search.set(value);
  }

  onNodeSearchChange(value: string): void {
    this.service.nodeSearch.set(value);
  }

  toggleApp(appId: string): void {
    this.service.toggleApp(appId);
  }

  toggleNode(nodeId: string): void {
    this.service.toggleNode(nodeId);
  }

  toggleLevel(level: TracingLevel): void {
    this.service.toggleLevel(level);
  }

  isLevelSelected(level: TracingLevel): boolean {
    return this.service.isLevelSelected(level);
  }

  isAppSelected(appId: string): boolean {
    return this.service.isAppSelected(appId);
  }

  isNodeSelected(nodeId: string): boolean {
    return this.service.isNodeSelected(nodeId);
  }

  setShowMode(mode: ShowMode): void {
    this.service.setShowMode(mode);
  }

  onTracePathClick(path: TracePathItem[]): void {
    this.service.navigateToPath(path);
    this.loadTreeData();
  }

  onGoClick(item: SelectedTreeItem): void {
    this.service.goToItem(item);
    this.loadTreeData();
  }

  onSelectTreeItem(item: TracingTreeRecordDto): void {
    this.service.selectedItem.set({
      record: item,
      path: this.tracePath(),
    });
  }

  // Tree data loading
  async loadTreeData(): Promise<void> {
    this.treeLoading.set(true);
    try {
      const path = this.tracePath();
      const curSpanTId = path.length > 0 ? (path[path.length - 1].record.record.span_t_id ?? null) : null;
      const curAppRunId = path.length > 0 ? (path[0].record.record.app_run_id ?? null) : null;
      const curIsEnd = path.length > 0 ? path[path.length - 1].record.end != null : false;

      const data = await this.service.loadTreeRecords({
        appRunId: curAppRunId,
        spanTId: curSpanTId,
        parentSpanTId: curSpanTId,
        isEnd: curIsEnd,
        scene: TracingRecordScene.Tree,
      });
      this.treeData.set(data);

      this.setupEventSource(curAppRunId, curSpanTId);
    } finally {
      this.treeLoading.set(false);
    }
  }

  private setupEventSource(appRunId: string | null, spanTId: string | null): void {
    this.closeEventSource();
    const currentIsEnd = this.tracePath().length > 0 ? this.tracePath()[this.tracePath().length - 1].record.end != null : false;
    if (currentIsEnd) return;

    const filter = this.filter();
    const params = new URLSearchParams();
    params.set('count', '51');
    if (appRunId) params.set('app_run_ids', JSON.stringify([appRunId]));
    if (spanTId) params.set('parent_span_t_ids', JSON.stringify([spanTId]));
    params.set('scene', TracingRecordScene.Tree);

    this.eventSource = new EventSource(`${BASE_URL}/records_subscribe?${params.toString()}`);
    this.eventSource.onmessage = (event) => {
      const record = JSON.parse(event.data) as TracingTreeRecordDto;
      this.handleLiveRecord(record);
    };
    this.eventSource.onerror = () => {
      this.eventSource?.close();
    };
  }

  private handleLiveRecord(record: TracingTreeRecordDto): void {
    this.treeData.update(d => {
      if (!d) return d;
      const records = [...d.records];

      if (record.record.kind === TracingKind.SpanClose) {
        const idx = records.findIndex(
          r => r.record.span_t_id === record.record.span_t_id && r.record.kind === TracingKind.SpanCreate
        );
        if (idx >= 0) {
          records[idx] = { ...records[idx], end: record.end };
        }
      } else if (record.record.kind === TracingKind.AppStop) {
        const idx = records.findIndex(
          r => r.record.app_run_id === record.record.app_run_id && r.record.kind === TracingKind.AppStart
        );
        if (idx >= 0) {
          records[idx] = { ...records[idx], end: record.end, variant: record.variant };
        }
      } else if (record.record.kind === TracingKind.RepEvent) {
        if (records.length > 0) {
          const last = { ...records[records.length - 1] };
          last.record = { ...last.record, repeated_count: (last.record.repeated_count ?? 0) + 1 };
          records[records.length - 1] = last;
        }
      } else if (record.record.kind === TracingKind.RelatedEvent) {
        const idx = records.findIndex(
          r => r.record.span_t_id === record.record.span_t_id && r.record.kind === TracingKind.SpanCreate
        );
        if (idx >= 0) {
          const updated = { ...records[idx] };
          const variant = updated.variant as any;
          if (variant?.spanRun) {
            variant.spanRun = {
              ...variant.spanRun,
              relatedEvents: [...(variant.spanRun.relatedEvents ?? []), record.record],
            };
          }
          records[idx] = updated;
        }
      } else if (record.record.kind === TracingKind.SpanRecord) {
        const idx = records.findIndex(
          r => r.record.span_t_id === record.record.span_t_id && r.record.kind === TracingKind.SpanCreate
        );
        if (idx >= 0) {
          const updated = { ...records[idx] };
          updated.record = { ...updated.record, fields: { ...updated.record.fields, ...record.record.fields } };
          records[idx] = updated;
        }
      } else if (!d.isEnd) {
        records.push(record);
        if (records.length > 100) {
          records.shift();
        }
      }

      return { ...d, records };
    });
  }

  private closeEventSource(): void {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }
}
