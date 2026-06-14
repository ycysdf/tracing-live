import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet],
  template: `<router-outlet />`,
  styles: [
    `
      :host {
        /*@apply items-center justify-center p-2;*/
      }
    `,
  ],
})
export class App {}
