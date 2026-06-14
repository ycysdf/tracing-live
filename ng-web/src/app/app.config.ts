import {
  ApplicationConfig,
  provideBrowserGlobalErrorListeners,
  provideAppInitializer,
  inject,
} from '@angular/core';
import { provideRouter, withHashLocation } from '@angular/router';
import { provideHttpClient } from '@angular/common/http';
import { provideTranslateService, TranslateService } from '@ngx-translate/core';
import { TranslateHttpLoader, TRANSLATE_HTTP_LOADER_CONFIG } from '@ngx-translate/http-loader';

import { routes } from './app.routes';
import { provideNgOpenapi } from '../api';

export const appConfig: ApplicationConfig = {
  providers: [
    provideHttpClient(),
    provideTranslateService({
      lang: 'en',
      loader: TranslateHttpLoader,
    }),
    {
      provide: TRANSLATE_HTTP_LOADER_CONFIG,
      useValue: {
        resources: [{ prefix: '/i18n/', suffix: '.json' }],
      },
    },
    provideAppInitializer(() => {
      const translate = inject(TranslateService);
      const browserLang = navigator.language?.split('-')[0];
      const lang = browserLang === 'zh' ? 'zh' : 'en';

      return new Promise((resolve) => {
        translate.use(lang).subscribe({
          next: () => {
            document.documentElement.lang = lang;
            resolve({});
          },
          error: () => {
            // Fallback to English on load failure
            translate.use('en').subscribe({
              next: () => resolve({}),
              error: () => resolve({}),
            });
          },
        });
      });
    }),
    provideNgOpenapi({
      basePath: 'http://localhost',
      enableDateTransform: true,
    }),
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withHashLocation()),
  ],
};
