import { Component, input, output, computed } from '@angular/core';
import type { TracingRecordDto } from '../../../api';
import { EXPANDABLE_KINDS } from '../../utils/constants';
import type { TracePathItem } from './traces.service';

@Component({
  selector: 'app-trace-path',
  template: `
    <div class="inline-flex flex-grow-0 px-2 items-stretch">
      @if (hasRoot()) {
        <div
          class="text-sm cursor-pointer py-2 px-2 hover:bg-stone-50 text-nowrap overflow-hidden text-ellipsis"
          (click)="pathClick.emit([])"
          (keydown.enter)="pathClick.emit([])"
          tabindex="0"
          role="button"
          aria-label="Root"
        >Root</div>
        <div class="text-sm flex items-center -mx-1 py-2 px-2">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="m9 18 6-6-6-6"/>
          </svg>
        </div>
      }
      @for (item of path(); track item.record.record.id; let idx = $index) {
        <div
          class="text-sm cursor-pointer py-2 px-2 hover:bg-stone-50 text-nowrap overflow-hidden text-ellipsis"
          (click)="pathClick.emit(path().slice(0, idx + 1))"
          (keydown.enter)="pathClick.emit(path().slice(0, idx + 1))"
          tabindex="0"
          role="button"
        >{{ item.record.record.name }}</div>
        <div class="text-sm flex items-center -mx-1 py-2 px-2">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="m9 18 6-6-6-6"/>
          </svg>
        </div>
      }
      @if (cur(); as curItem) {
        <div class="text-sm flex items-center py-1 px-1 text-nowrap overflow-hidden text-ellipsis">
          {{ curItem.name }}
        </div>
        @if (curIsExpandable()) {
          <div class="text-sm flex items-center -mx-1 py-2 px-2">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="m9 18 6-6-6-6"/>
            </svg>
          </div>
        }
      }
    </div>
  `,
})
export class TracePathComponent {
  readonly path = input<TracePathItem[]>([]);
  readonly cur = input<TracingRecordDto | null>(null);
  readonly hasRoot = input(false);
  readonly curIsExpandable = input(false);
  readonly pathClick = output<TracePathItem[]>();
}
