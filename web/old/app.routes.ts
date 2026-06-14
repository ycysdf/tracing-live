import { Routes } from '@angular/router';
import {AppTraceComponent} from '../../ng-web/src/app/app-trace/app-trace.component';

export const routes: Routes = [
  {path: '', component: AppTraceComponent},
  {
    path: 'trace',
    component: AppTraceComponent,
    // loadComponent: () => import('./app-trace/app-trace.component').then(n=> n.AppTraceComponent),
  }
];
