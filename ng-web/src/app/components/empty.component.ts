import { Component, input, inject, computed } from '@angular/core';
import { TranslationService } from '../i18n/translation.service';
import { cn } from '../utils/cn';

@Component({
  selector: 'app-empty',
  template: `
    <div [class]="containerClass()">
      {{ i18n.t('noData', 'common') }}
    </div>
  `,
})
export class EmptyComponent {
  readonly class = input<string>('');
  readonly i18n = inject(TranslationService);
  readonly containerClass = computed(() => cn('items-center justify-center p-2', this.class()));
}
