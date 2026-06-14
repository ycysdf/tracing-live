import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter, withHashLocation } from '@angular/router';
import { provideHttpClient } from '@angular/common/http';

import { routes } from './app.routes';
import { provideNgOpenapi } from '../api';

export const appConfig: ApplicationConfig = {
  providers: [
    provideHttpClient(),
    provideNgOpenapi({
      basePath: 'http://localhost',
      enableDateTransform: true,
    }),
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withHashLocation()),
  ],
};
