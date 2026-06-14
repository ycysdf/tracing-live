import { Component, input, output, signal, computed } from '@angular/core';
import { TracingKind } from '../../../api';
import { EXPANDABLE_KINDS, NULL_STR } from '../../utils/constants';
import { formatDate, formatValue } from '../../utils/format';
import type { TracePathItem, SelectedTreeItem } from './traces.service';
import { TracePathComponent } from './trace-path.component';

@Component({
  selector: 'app-detail-panel',
  imports: [TracePathComponent],
  template: `
    <div class="flex flex-col gap-3 w-[550px] max-w-[900px]">
      <!-- Header -->
      <div class="flex items-stretch gap-2 justify-between">
        <div class="panel overflow-hidden flex-grow">
          <app-trace-path
            [path]="data()?.path ?? []"
            [cur]="data()?.record?.record ?? null"
            [hasRoot]="true"
            [curIsExpandable]="isCurExpandable()"
            (pathClick)="tracePathClick.emit($event)"
          />
        </div>
        <div
          class="px-4 py-2 panel flex-shrink-0 items-center text-sm justify-center flex text-center hover:bg-stone-50 cursor-pointer"
          (click)="goClick.emit()"
          (keydown.enter)="goClick.emit()"
          tabindex="0"
          role="button"
          aria-label="Go to item"
        >Go</div>
      </div>

      <!-- Detail content -->
      <div class="panel flex-grow p-2 flex flex-col self-start flex-shrink-0 w-full overflow-hidden" style="max-height: calc(100vh - 200px)">
        @if (tabs().length > 1) {
          <div class="flex gap-1 mb-1 px-2 -mx-2 bg-gray-50" role="tablist">
            @for (tab of tabs(); track tab) {
              <div
                class="border-t-2 cursor-pointer p-2 px-3"
                [class.hover:bg-background]="curTab() !== tab"
                [class.bg-background]="curTab() === tab"
                [class.border-t-primary]="curTab() === tab"
                (click)="curTab.set(tab)"
                (keydown.enter)="curTab.set(tab)"
                tabindex="0"
                role="tab"
                [attr.aria-selected]="curTab() === tab"
              >{{ tab }}</div>
            }
          </div>
        }

        <div class="flex-grow overflow-hidden -mx-2 flex flex-col overflow-y-auto">
          @switch (curTab()) {
            @case ('Info') {
              <table class="flex flex-col">
                <tbody>
                  <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                    <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">Id</td>
                    <td class="text-xsm">{{ record()?.record?.id }}</td>
                  </tr>
                  <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                    <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">RecordIndex</td>
                    <td class="text-xsm">{{ record()?.record?.record_index }}</td>
                  </tr>
                  <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                    <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">Content</td>
                    <td class="text-xsm">{{ record()?.record?.name }}</td>
                  </tr>
                  <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                    <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">RecordTime</td>
                    <td class="text-xsm">{{ formatDate(record()?.record?.record_time) }}</td>
                  </tr>
                  <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                    <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">NodeId</td>
                    <td class="text-xsm">{{ record()?.record?.node_id }}</td>
                  </tr>
                  <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                    <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">ParentSpanTId</td>
                    <td class="text-xsm">{{ record()?.record?.parent_span_t_id ?? NULL_STR }}</td>
                  </tr>
                  <!-- Fields -->
                  @if (fieldKeys().length > 0) {
                    <tr class="leading-8 border-b border-b-gray-100 hover:bg-stone-50 cursor-pointer" (click)="fieldsExpanded.update(v => !v)">
                      <td class="w-[1%] select-none text-xsm font-bold pl-1 pr-1 min-w-[100px] text-left whitespace-nowrap">
                        <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                             class="inline-block mr-1 transition-transform" [class.rotate-90]="fieldsExpanded()">
                          <path d="m9 18 6-6-6-6"/>
                        </svg>
                        Fields ({{ fieldKeys().length }})
                      </td>
                      <td class="text-sm"></td>
                    </tr>
                    @if (fieldsExpanded()) {
                      @for (key of fieldKeys(); track key) {
                        <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                          <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">{{ key }}</td>
                          <td class="text-xsm max-h-[400px] overflow-y-auto break-all whitespace-pre-wrap pr-2">{{ formatValue(record()?.record?.fields?.[key]) }}</td>
                        </tr>
                      }
                    }
                  }
                  <!-- Other -->
                  <tr class="leading-8 border-b border-b-gray-100 hover:bg-stone-50 cursor-pointer" (click)="otherExpanded.update(v => !v)">
                    <td class="w-[1%] select-none text-xsm font-bold pl-1 pr-1 min-w-[100px] text-left whitespace-nowrap">
                      <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                           class="inline-block mr-1 transition-transform" [class.rotate-90]="otherExpanded()">
                        <path d="m9 18 6-6-6-6"/>
                      </svg>
                      Other
                    </td>
                    <td class="text-sm"></td>
                  </tr>
                  @if (otherExpanded()) {
                    <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                      <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">ParentId</td>
                      <td class="text-xsm">{{ record()?.record?.parent_id ?? NULL_STR }}</td>
                    </tr>
                    <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                      <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">Kind</td>
                      <td class="text-xsm">{{ record()?.record?.kind }}</td>
                    </tr>
                    <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                      <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">Level</td>
                      <td class="text-xsm">{{ record()?.record?.level }}</td>
                    </tr>
                    <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                      <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">SpanTId</td>
                      <td class="text-xsm">{{ record()?.record?.span_t_id ?? NULL_STR }}</td>
                    </tr>
                    <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                      <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">CreationTime</td>
                      <td class="text-xsm">{{ formatDate(record()?.record?.creation_time) }}</td>
                    </tr>
                  }
                  <!-- App info -->
                  <tr class="leading-8 border-b border-b-gray-100 hover:bg-stone-50 cursor-pointer" (click)="appInfoExpanded.update(v => !v)">
                    <td class="w-[1%] select-none text-xsm font-bold pl-1 pr-1 min-w-[100px] text-left whitespace-nowrap">
                      <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                           class="inline-block mr-1 transition-transform" [class.rotate-90]="appInfoExpanded()">
                        <path d="m9 18 6-6-6-6"/>
                      </svg>
                      AppInfo
                    </td>
                    <td class="text-sm"></td>
                  </tr>
                  @if (appInfoExpanded()) {
                    <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                      <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">AppId</td>
                      <td class="text-xsm">{{ record()?.record?.app_id }}</td>
                    </tr>
                    <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                      <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">AppVersion</td>
                      <td class="text-xsm">{{ record()?.record?.app_version }}</td>
                    </tr>
                    <tr class="align-top leading-8 border-b border-b-gray-100 hover:bg-stone-50">
                      <td class="w-[1%] select-none text-xsm font-bold pl-6 pr-2 min-w-[160px] text-left whitespace-nowrap">AppRunId</td>
                      <td class="text-xsm">{{ record()?.record?.app_run_id }}</td>
                    </tr>
                  }
                </tbody>
              </table>
            }
            @case ('Enter List') {
              <div class="p-2 text-sm text-muted-foreground">Enter List (lazy loaded)</div>
            }
            @case ('Field Record') {
              <div class="p-2 text-sm text-muted-foreground">Field Record (lazy loaded)</div>
            }
          }
        </div>
      </div>
    </div>
  `,
})
export class DetailPanelComponent {
  readonly data = input<SelectedTreeItem | null>(null);
  readonly tracePathClick = output<TracePathItem[]>();
  readonly goClick = output<void>();

  readonly NULL_STR = NULL_STR;
  readonly TracingKind = TracingKind;

  readonly curTab = signal('Info');
  readonly fieldsExpanded = signal(true);
  readonly otherExpanded = signal(false);
  readonly appInfoExpanded = signal(false);

  readonly record = computed(() => this.data()?.record);
  readonly tabs = computed(() => {
    const kind = this.data()?.record?.record?.kind;
    const tabs = ['Info'];
    if (kind === TracingKind.SpanCreate) {
      tabs.push('Enter List', 'Field Record');
    }
    return tabs;
  });

  readonly isCurExpandable = computed(() => {
    const kind = this.data()?.record?.record?.kind;
    return kind != null && EXPANDABLE_KINDS.includes(kind);
  });

  readonly fieldKeys = computed(() => {
    const fields = this.data()?.record?.record?.fields ?? {};
    return Object.keys(fields);
  });

  formatDate(date: Date | string | undefined): string {
    return formatDate(date, NULL_STR);
  }

  formatValue(value: unknown): string {
    return formatValue(value, NULL_STR);
  }
}
