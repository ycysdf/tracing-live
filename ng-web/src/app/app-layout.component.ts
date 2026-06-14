import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { NavComponent } from './components/nav.component';

@Component({
  selector: 'app-layout',
  imports: [RouterOutlet, NavComponent],
  template: `
    <div class="flex items-stretch">
      <app-nav />
      <div class="flex-grow overflow-hidden flex flex-col gap-3 p-3 bg-gray-50 h-screen">
        <router-outlet />
      </div>
    </div>
  `,
})
export class AppLayout {}
