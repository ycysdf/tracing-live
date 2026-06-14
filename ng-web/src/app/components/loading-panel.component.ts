import { Component, input } from '@angular/core';

@Component({
  selector: 'app-loading-panel',
  template: `
    <app-loading class="panel py-12" [class]="class()">
      <ng-content>
        <div class="text-xsm">Loading..</div>
      </ng-content>
    </app-loading>
  `,
  imports: [LoadingComponent],
})
export class LoadingPanelComponent {
  readonly class = input<string>('');
}

import { LoadingComponent } from './loading.component';
