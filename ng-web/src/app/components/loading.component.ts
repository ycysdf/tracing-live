import { Component, input, computed } from '@angular/core';
import { cn } from '../utils/cn';

@Component({
  selector: 'app-loading',
  template: `
    <div [class]="containerClass()">
      <div class="la-line-scale text-primary">
        <div></div>
        <div></div>
        <div></div>
        <div></div>
        <div></div>
      </div>
      <ng-content />
    </div>
  `,
})
export class LoadingComponent {
  readonly class = input<string>('');
  readonly containerClass = computed(() => cn('flex flex-col items-center gap-3 p-3', this.class()));
}
