import {
  ApplicationConfig,
  provideBrowserGlobalErrorListeners,
  APP_INITIALIZER,
  inject,
} from '@angular/core';
import { provideRouter, withHashLocation } from '@angular/router';
import { provideHttpClient } from '@angular/common/http';
import {
  provideTranslateService,
  TranslateService,
} from '@ngx-translate/core';
import { provideTranslateHttpLoader } from '@ngx-translate/http-loader';

import { routes } from './app.routes';
import { provideNgOpenapi } from '../api';

export const appConfig: ApplicationConfig = {
  providers: [
    provideHttpClient(),
    provideTranslateHttpLoader({ prefix: '/i18n/', suffix: '.json' }),
    provideTranslateService({ lang: 'en' }),
    {
      provide: APP_INITIALIZER,
      useFactory: () => {
        const translate = inject(TranslateService);

        return () => {
          const browserLang = navigator.language?.split('-')[0];
          const lang = browserLang === 'zh' ? 'zh' : 'en';

          return new Promise<void>((resolve) => {
            translate.use(lang).subscribe({
              next: () => {
                document.documentElement.lang = lang;
                resolve();
              },
              error: () => {
                // Fallback to English on load failure
                translate.use('en').subscribe({
                  next: () => resolve(),
                  error: () => resolve(),
                });
              },
            });
          });
        };
      },
      multi: true,
    },
    provideNgOpenapi({
      basePath: 'http://localhost',
      enableDateTransform: true,
    }),
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withHashLocation()),
  ],
};
