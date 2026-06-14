import { Component, computed, inject, effect, resource, Injector } from '@angular/core';
import { FormsModule } from '@angular/forms';
import {
  TracingKind,
  TracingLevel,
  TracingRecordScene,
  type TracingTreeRecordDto,
  type TracingRecordDto,
} from '../../../api';
import { TranslatePipe } from '@ngx-translate/core';
import { formatDuration, formatDate } from '../../utils/format';
import {
  ALL_LEVELS,
  EXPANDABLE_KINDS,
  getLevelColor,
} from '../../utils/constants';
import { LoadingComponent } from '../../components/loading.component';
import { LoadingPanelComponent } from '../../components/loading-panel.component';
import { EmptyComponent } from '../../components/empty.component';
import {
  TracesService,
  type ShowMode,
  SHOW_MODES,
  type TracePathItem,
  type SelectedTreeItem,
  type RecordsTreeData,
} from './traces.service';
import { LiveRecordsService } from './live-records.service';
import { NodeItemComponent } from './node-item.component';
import { TracePathComponent } from './trace-path.component';
import { DetailPanelComponent } from './detail-panel.component';
import { TreeItemComponent } from './tree-item.component';

@Component({
  selector: 'app-traces-page',
  templateUrl: './traces.page.html',
  providers: [TracesService, LiveRecordsService],
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
export class TracesPage {
  readonly service = inject(TracesService);
  readonly liveRecords = inject(LiveRecordsService);

  readonly filter = this.service.filter;
  readonly nodesPage = this.service.nodesPageResource.value;
  readonly nodesPageLoading = this.service.nodesPageResource.isLoading;
  readonly nodesPageError = this.service.nodesPageResource.error;
  readonly tracePath = this.service.tracePath;
  readonly selectedItem = this.service.selectedItem;
  readonly search = this.service.search;
  readonly nodeSearch = this.service.nodeSearch;
  readonly isEnd = this.service.isEnd;
  readonly spanTId = this.service.spanTId;
  readonly appRunId = this.service.appRunId;

  // Tree data — reloads automatically when tracePath or filter changes
  readonly treeDataResource = resource({
    params: () => ({
      path: this.tracePath(),
      filter: this.filter(),
    }),
    loader: async ({ params }) => {
      const { path } = params;
      const curSpanTId = path.length > 0 ? (path[path.length - 1].record.record.span_t_id ?? null) : null;
      const curAppRunId = path.length > 0 ? (path[0].record.record.app_run_id ?? null) : null;
      const curIsEnd = path.length > 0 ? path[path.length - 1].record.end != null : false;

      return await this.service.loadTreeRecords({
        appRunId: curAppRunId,
        spanTId: curSpanTId,
        parentSpanTId: curSpanTId,
        isEnd: curIsEnd,
        scene: TracingRecordScene.Tree,
      });
    },
    defaultValue: null as RecordsTreeData | null,
    injector: inject(Injector),
  });

  public now = new Date();

  readonly ALL_LEVELS = ALL_LEVELS;
  readonly SHOW_MODES = SHOW_MODES;
  readonly EXPANDABLE_KINDS = EXPANDABLE_KINDS;

  constructor() {
    // SSE live updates — auto-reattach when path changes
    effect((onCleanup) => {
      const data = this.treeDataResource.value();
      if (!data || data.isEnd) return;

      const path = this.tracePath();
      const appRunId = path.length > 0 ? (path[0].record.record.app_run_id ?? null) : null;
      const spanTId = path.length > 0 ? (path[path.length - 1].record.record.span_t_id ?? null) : null;

      const sub = this.liveRecords
        .subscribe({ appRunId, spanTId, isEnd: false })
        .subscribe((record) => this.handleLiveRecord(record));

      onCleanup(() => sub.unsubscribe());
    });
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
    return formatDate(date);
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
  }

  onGoClick(item: SelectedTreeItem): void {
    this.service.goToItem(item);
  }

  onSelectTreeItem(item: TracingTreeRecordDto): void {
    this.service.selectedItem.set({
      record: item,
      path: this.tracePath(),
    });
  }

  private handleLiveRecord(record: TracingTreeRecordDto): void {
    this.treeDataResource.value.update(d => {
      if (!d) return d;
      const records = [...d.records];

      if (record.record.kind === TracingKind.SpanClose) {
        const idx = records.findIndex(
          r => r.record.span_t_id === record.record.span_t_id && r.record.kind === TracingKind.SpanCreate,
        );
        if (idx >= 0) {
          records[idx] = { ...records[idx], end: record.end };
        }
      } else if (record.record.kind === TracingKind.AppStop) {
        const idx = records.findIndex(
          r => r.record.app_run_id === record.record.app_run_id && r.record.kind === TracingKind.AppStart,
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
          r => r.record.span_t_id === record.record.span_t_id && r.record.kind === TracingKind.SpanCreate,
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
          r => r.record.span_t_id === record.record.span_t_id && r.record.kind === TracingKind.SpanCreate,
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
}
