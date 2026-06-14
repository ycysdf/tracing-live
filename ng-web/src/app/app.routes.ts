import { Routes } from '@angular/router';
import { AppLayout } from './app-layout.component';

export const routes: Routes = [
  {
    path: '',
    component: AppLayout,
    children: [
      { path: '', redirectTo: 'trace', pathMatch: 'full' },
      {
        path: 'index',
        loadComponent: () => import('./pages/overview/overview.page').then(m => m.OverviewPage),
      },
      {
        path: 'trace',
        loadComponent: () => import('./pages/traces/traces.page').then(m => m.TracesPage),
      },
      {
        path: 'node',
        loadComponent: () => import('./pages/overview/overview.page').then(m => m.OverviewPage),
      },
      {
        path: 'app',
        loadComponent: () => import('./pages/overview/overview.page').then(m => m.OverviewPage),
      },
      {
        path: 'notify',
        loadComponent: () => import('./pages/overview/overview.page').then(m => m.OverviewPage),
      },
      {
        path: 'setting',
        loadComponent: () => import('./pages/overview/overview.page').then(m => m.OverviewPage),
      },
    ],
  },
  {
    path: '**',
    loadComponent: () => import('./pages/not-found/not-found.page').then(m => m.NotFoundPage),
  },
];
