import { Injectable, signal, computed, inject, DestroyRef } from '@angular/core';
import { DOCUMENT } from '@angular/common';
import { TranslateService } from '@ngx-translate/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';

export type SupportedLang = 'en' | 'zh';

@Injectable({ providedIn: 'root' })
export class TranslationService {
  private readonly ts = inject(TranslateService);
  private readonly document = inject(DOCUMENT);
  private readonly destroyRef = inject(DestroyRef);

  private readonly _lang = signal<SupportedLang>('en');

  readonly lang = this._lang.asReadonly();

  readonly isZh = computed(() => this._lang() === 'zh');

  constructor() {
    // Sync initial state from TranslateService
    const currentLang = this.ts.getCurrentLang();
    if (currentLang === 'zh' || currentLang === 'en') {
      this._lang.set(currentLang);
    }

    // React to future language changes
    this.ts.onLangChange
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe((event) => {
        const lang = event.lang as SupportedLang;
        this._lang.set(lang);
        this.document.documentElement.lang = lang;
      });
  }

  setLang(lang: SupportedLang): void {
    this.ts.use(lang);
  }

  t(key: string, ns: 'translation' | 'common' = 'translation'): string {
    const fullKey = `${ns}.${key}`;
    return this.ts.instant(fullKey);
  }

  /** Usage: {{ 'common:search' | t }} */
  translate(key: string): string {
    const parts = key.split(':');
    if (parts.length === 2) {
      return this.t(parts[1], parts[0] as 'common');
    }
    return this.t(parts[0]);
  }
}
