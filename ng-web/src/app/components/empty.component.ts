import { Component, input, computed } from '@angular/core';
import { TranslatePipe } from '@ngx-translate/core';
import { cn } from '../utils/cn';

@Component({
  selector: 'app-empty',
  imports: [TranslatePipe],
  template: `
    <div [class]="containerClass()">
      {{ 'common.noData' | translate }}
    </div>
  `,
  styles: [
    `
      :host {
        /*@apply items-center justify-center p-2;*/
      }
    `,
  ],
})
export class EmptyComponent {
  readonly class = input<string>('');
  readonly containerClass = computed(() => cn('items-center justify-center p-2', this.class()));
}
