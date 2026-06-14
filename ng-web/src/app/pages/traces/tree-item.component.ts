import { Component, input, signal, computed, inject } from '@angular/core';
import {
  TracingKind,
  TracingLevel,
  type TracingTreeRecordDto,
  type TracingRecordDto,
  type TracingTreeRecordVariantDto,
} from '../../../api';
import {
  EXPANDABLE_KINDS,
  getLevelColor,
  RECORD_FIELDS,
} from '../../utils/constants';
import { getFlags } from '../../utils/helpers';
import { cn } from '../../utils/cn';
import { TracesService, type TracePathItem, type SelectedTreeItem } from './traces.service';

function formatDuration(ms: number): string {
  if (ms < 1000) return ms + 'ms';
  const s = Math.floor(ms / 1000);
  if (s < 60) return s + 's';
  const m = Math.floor(s / 60);
  return m + 'm ' + (s % 60) + 's';
}

const ITEM_HEIGHT = 32;

@Component({
  selector: 'app-tree-item',
  imports: [],
  template: `
    <div class="border border-gray-100" [class.ring-1 ring-offset-4 z-20 ring-blue-600]="isSelected()">
      <!-- Item header -->
      <div
        class="flex bg-background z-10 items-center gap-2 px-1 select-none cursor-pointer border border-transparent"
        [class.bg-stone-100]="isSelected()"
        [class.hover:bg-stone-50]="!isSelected()"
        [style.height.px]="ITEM_HEIGHT"
        (click)="onSelect()"
        (dblclick)="onExpand()"
        (contextmenu)="onContextMenu($event)"
      >
        <!-- Level indicator -->
        <div
          class="w-[2px] flex-shrink-0 rounded-sm my-1 self-stretch"
          [style.background]="getLevelColor(data().record.level)"
        ></div>

        <!-- Expand chevron -->
        @if (isExpandable() && !isTypedSpan()) {
          <div class="p-1 -mx-1 rounded-sm cursor-pointer hover:bg-stone-200" (click)="toggleExpand($event)">
            <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                 class="transition-transform" [class.rotate-90]="expanded()">
              <path d="m9 18 6-6-6-6"/>
            </svg>
          </div>
        }

        <!-- Dot for non-expandable items -->
        @if (!isExpandable()) {
          <div class="p-1 flex-shrink-0 -mx-1" [style.color]="getLevelColor(data().record.level)">
            <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="currentColor" stroke="none">
              <circle cx="12" cy="12" r="4"/>
            </svg>
          </div>
        }

        <!-- Name -->
        <div class="text-ellipsis flex-shrink-0 overflow-hidden whitespace-nowrap select-none">
          {{ data().record.name }}
        </div>

        <!-- Repeated count -->
        @if (data().record.kind === TracingKind.Event && data().record.repeated_count != null) {
          <div class="shadow-sm border p-1 text-primary flex rounded-sm overflow-hidden text-xsm leading-none select-none"
               title="重复次数">
            x{{ data().record.repeated_count }}
          </div>
        }

        <!-- Fields badges -->
        @for (entry of displayFields(); track entry[0]) {
          <div class="bg-stone-100 shadow-sm flex-shrink-0 border flex rounded-sm overflow-hidden text-xsm leading-none text-nowrap select-none">
            <div class="p-1">{{ entry[0] }}</div>
            @if (entry[1]) {
              <div class="bg-background border-l p-1">{{ entry[1] }}</div>
            }
          </div>
        }

        <div class="flex-grow self-stretch"></div>

        <!-- Timestamp -->
        <div class="whitespace-nowrap flex-shrink-0 select-none">
          {{ formatDate(data().record.record_time) }}
        </div>

        <!-- Duration timer for SpanCreate/AppStart -->
        @if (isRunning()) {
          <div class="bg-stone-100 shadow-sm border flex rounded-sm overflow-hidden text-xsm leading-none select-none"
               title="用时">
            <div class="p-1 bg-blue-600/80 text-muted">run</div>
            <div class="bg-background border-l p-1 text-nowrap select-none">{{ runElapsed() }}</div>
          </div>
        }
      </div>

      <!-- Context Menu (simplified - shown as a dropdown) -->
      @if (contextMenuOpen()) {
        <div class="fixed z-50 min-w-32 overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
             [style.left.px]="contextMenuX()" [style.top.px]="contextMenuY()"
             (click)="contextMenuOpen.set(false)">
          <div class="relative flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none hover:bg-accent"
               (click)="onGoHere()">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mr-3">
              <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><polyline points="10 17 15 12 10 7"/><line x1="15" x2="3" y1="12" y2="12"/>
            </svg>
            Go Here
          </div>
          @if (isExpandable()) {
            <div class="relative flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none hover:bg-accent"
                 (click)="toggleExpand($event)">
              @if (expanded()) {
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none"
                     stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mr-3">
                  <path d="m15 18-6-6 6-6"/><path d="m9 6 6 6-6 6"/>
                </svg>
                Collapse
              } @else {
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none"
                     stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mr-3">
                  <path d="m15 6-6 6 6 6"/><path d="m9 18 6-6-6-6"/>
                </svg>
                Expand
              }
            </div>
          }
        </div>
      }

      <!-- Related events -->
      @if (relatedEvents().length > 0) {
        <div class="border-l-2 mb-1 ml-1 -mt-1 pl-1 pt-1.5 relative left-[0.5px] top-[-1px]"
             [style.border-color]="getLevelColor(data().record.level)">
          @for (event of relatedEvents(); track event.id) {
            <div class="flex items-center gap-2 px-1 leading-8 select-none text-sm hover:bg-stone-50 cursor-pointer"
                 (click)="onSelectRelated(event)">
              <div class="w-[2px] flex-shrink-0 rounded-sm my-1 self-stretch ml-1.5 opacity-50"
                   [style.background]="getLevelColor(event.level)"></div>
              <div class="leading-none p-1 bg-stone-50 border text-primary text-xsm rounded-sm px-1 py-1">
                {{ event.fields[RECORD_FIELDS.related_name] ?? 'NULL' }}
              </div>
              <div class="text-ellipsis overflow-hidden whitespace-nowrap select-none">{{ event.name }}</div>
              @if (event.kind === TracingKind.Event && event.repeated_count != null) {
                <div class="shadow-sm border p-1 text-primary flex rounded-sm overflow-hidden text-xsm leading-none select-none">
                  x{{ event.repeated_count }}
                </div>
              }
            </div>
          }
        </div>
      }

      <!-- Children (when expanded) -->
      @if (hasChildren() && expanded()) {
        <div class="ml-4">
          <!-- Todo: recursion for deeper levels -->
        </div>
      }
    </div>
  `,
  host: {
    '[style.display]': '"block"',
  },
})
export class TreeItemComponent {
  readonly service = inject(TracesService);
  readonly ITEM_HEIGHT = ITEM_HEIGHT;
  readonly RECORD_FIELDS = RECORD_FIELDS;
  readonly TracingKind = TracingKind;

  readonly data = input.required<TracingTreeRecordDto>();
  readonly isEnd = input(false);
  readonly layer = input(0);
  readonly path = input<TracePathItem[]>([]);

  readonly expanded = signal(false);
  readonly contextMenuOpen = signal(false);
  readonly contextMenuX = signal(0);
  readonly contextMenuY = signal(0);

  readonly isExpandable = computed(() =>
    EXPANDABLE_KINDS.includes(this.data().record.kind)
  );

  readonly hasChildren = computed(() =>
    this.data().record.kind === TracingKind.AppStart ||
    (this.data().record.span_t_id != null && this.data().record.kind === TracingKind.SpanCreate)
  );

  readonly isTypedSpan = computed(() => {
    const name = this.data().record.name;
    return this.data().record.kind === TracingKind.SpanCreate &&
      name.startsWith('[t:') && name.endsWith(']');
  });

  readonly isRunning = computed(() =>
    [TracingKind.SpanCreate, TracingKind.AppStart as string].includes(this.data().record.kind)
  );

  readonly isSelected = computed(() => {
    const item = this.service.selectedItem();
    return item?.record?.record?.id === this.data().record.id;
  });

  readonly relatedEvents = computed(() => {
    const variant = this.data().variant as any;
    return variant?.spanRun?.relatedEvents ?? [];
  });

  readonly displayFields = computed(() => {
    const fields = {
      ...(this.data().record.fields ?? {}),
      ...(((this.data().variant as any)?.spanRun?.fields) ?? {}),
    };
    return Object.entries(fields)
      .filter(([key, val]: [string, unknown]) => typeof val !== 'object' && !key.startsWith('__data'))
      .map(([key, val]: [string, unknown]) => {
        let value = String(val ?? '').trim();
        if (value.length > 80) value = '...';
        return [key, value] as [string, string];
      });
  });

  readonly runElapsed = computed(() => {
    // Simplified - just show the time since start if still running
    if (!this.isEnd()) {
      const start = new Date(this.data().record.record_time).getTime();
      return formatDuration(Date.now() - start);
    }
    return '';
  });

  getLevelColor(level?: TracingLevel | null): string {
    return getLevelColor(level);
  }

  formatDate(date: Date | string): string {
    if (!date) return '';
    return new Date(date).toLocaleString();
  }

  onSelect(): void {
    this.service.selectedItem.set({
      record: this.data(),
      path: this.path(),
    });
  }

  toggleExpand(event: MouseEvent): void {
    event.stopPropagation();
    this.expanded.update(v => !v);
  }

  onExpand(): void {
    if (!this.isTypedSpan()) {
      this.expanded.update(v => !v);
    }
  }

  onContextMenu(event: MouseEvent): void {
    event.preventDefault();
    this.contextMenuX.set(event.clientX);
    this.contextMenuY.set(event.clientY);
    this.contextMenuOpen.set(true);
    // Close on next click outside
    setTimeout(() => {
      const close = () => {
        this.contextMenuOpen.set(false);
        document.removeEventListener('click', close);
      };
      document.addEventListener('click', close);
    });
  }

  onGoHere(): void {
    const data = this.data();
    const path = this.path();
    if (EXPANDABLE_KINDS.includes(data.record.kind)) {
      this.service.tracePath.set([...path, { record: data }]);
    } else {
      this.service.tracePath.set(path);
    }
  }

  onSelectRelated(event: TracingRecordDto): void {
    this.service.selectedItem.set({
      record: { record: event, variant: null, end: null },
      path: [...this.path(), { record: this.data() }],
    });
  }
}
