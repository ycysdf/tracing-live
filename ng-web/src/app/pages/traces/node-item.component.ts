import { Component, input, output, computed } from '@angular/core';
import type { AppNodeRunDto } from '../../../api';

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
  selector: 'app-node-item',
  template: `
    <div
      class="p-1.5 pb-1 gap-1.5 border-gray-100 border border-t-2 flex flex-col w-[220px] cursor-pointer select-none"
      [class.bg-stone-100]="isSelected()"
      [class.border-t-primary]="isSelected()"
      [class.hover:bg-stone-50]="!isSelected()"
      (click)="itemClick.emit()"
      (keydown.enter)="itemClick.emit()"
      tabindex="0"
      role="button"
      [attr.aria-pressed]="isSelected()"
    >
      <div class="flex justify-between items-baseline overflow-hidden gap-1">
        <div class="text-lg font-bold flex-shrink-0 text-ellipsis overflow-hidden whitespace-nowrap">
          {{ displayName() }}
        </div>
        <div class="text-xsm text-ellipsis overflow-hidden whitespace-nowrap">
          {{ secondName() }}
        </div>
      </div>
      <div class="flex text-xsm items-center justify-between gap-2">
        <div class="text-sm">{{ brief() }}</div>
        <div class="text-xsm text-ellipsis overflow-hidden whitespace-nowrap">
          {{ duration() }}
        </div>
      </div>
    </div>
  `,
})
export class NodeItemComponent {
  readonly data = input.required<AppNodeRunDto>();
  readonly now = input.required<Date>();
  readonly isSelected = input(false);
  readonly itemClick = output<void>();

  private readonly dataObj = computed(() => this.data().data ?? {});

  readonly displayName = computed(() =>
    this.dataObj()['name'] ?? this.dataObj()['node_name'] ?? this.data().node_id
  );

  readonly secondName = computed(() =>
    this.dataObj()['second_name'] ?? this.dataObj()['os_name'] ?? ''
  );

  readonly brief = computed(() =>
    this.dataObj()['brief'] ?? this.dataObj()['ip'] ?? ''
  );

  readonly duration = computed(() => {
    const d = this.data();
    const nowValue = this.now();
    const nowMs = nowValue instanceof Date ? nowValue.getTime() : new Date(nowValue as unknown as string).getTime();
    return formatDuration(nowMs - new Date(d.start_time).getTime());
  });
}
