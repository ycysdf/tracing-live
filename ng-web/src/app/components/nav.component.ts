import { Component, signal, inject } from '@angular/core';
import { Router, RouterLink, RouterLinkActive } from '@angular/router';
import { TranslatePipe } from '@ngx-translate/core';

interface NavItem {
  translationKey: string;
  url: string;
  iconSvg: string;
}

@Component({
  selector: 'app-nav',
  imports: [RouterLink, RouterLinkActive, TranslatePipe],
  template: `
    <div
      class="border-r-gray-200 transition-[width] bg-background shadow-sm flex flex-col justify-between py-2 h-screen"
      [style.width.px]="isCollapse() ? 48 : 180"
    >
      <!-- Brand -->
      <a
        routerLink="/"
        class="h-10 cursor-pointer mb-2 leading-none flex justify-center items-center font-bold text-xl text-nowrap no-underline text-foreground"
      >
        {{ isCollapse() ? 'TL' : 'Tracing Live' }}
      </a>

      <!-- Navigation -->
      <nav class="flex flex-grow flex-col border-t border-t-border text-muted-foreground">
        @for (item of links; track item.url) {
          <a
            [routerLink]="item.url"
            [title]="item.translationKey | translate"
            class="gap-3 transition-[width] hover:bg-stone-50 flex items-center text-nowrap no-underline text-muted-foreground"
            [class.justify-center]="isCollapse()"
            [class.px-3]="!isCollapse()"
            style="height: 48px"
            routerLinkActive="text-primary bg-stone-100 border-r-2 border-r-primary font-bold"
            [routerLinkActiveOptions]="{ exact: false }"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              [attr.stroke-width]="isActive(item.url) ? '2' : '1'"
              stroke-linecap="round"
              stroke-linejoin="round"
              [innerHTML]="item.iconSvg"
            ></svg>
            @if (!isCollapse()) {
              <span>{{ item.translationKey | translate }}</span>
            }
          </a>
        }
      </nav>

      <!-- Toggle -->
      <div>
        <div
          class="flex select-none items-center h-12 px-4 py-2 cursor-pointer hover:bg-stone-50"
          (click)="toggleCollapse()"
          (keydown.enter)="toggleCollapse()"
          tabindex="0"
          role="button"
          [attr.aria-label]="isCollapse() ? 'Expand sidebar' : 'Collapse sidebar'"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            @if (isCollapse()) {
              <path d="M13 17l5-5-5-5" />
              <path d="M6 17V7" />
            } @else {
              <path d="M11 17l-5-5 5-5" />
              <path d="M18 17V7" />
            }
          </svg>
        </div>
      </div>
    </div>
  `,
})
export class NavComponent {
  readonly router = inject(Router);

  readonly isCollapse = signal(this.getStoredCollapse());

  readonly links: NavItem[] = [
    {
      translationKey: 'translation.overview',
      url: '/index',
      iconSvg: `<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M9 21V9"/>`,
    },
    {
      translationKey: 'translation.trace',
      url: '/trace',
      iconSvg: `<path d="M21 12a7 7 0 0 1-7 7"/><path d="M3 12a7 7 0 0 0 7 7"/><path d="M12 3v18"/><path d="M12 3a7 7 0 0 0-7 7"/><path d="M21 12a7 7 0 0 0-4-5.8"/>`,
    },
    {
      translationKey: 'translation.node',
      url: '/node',
      iconSvg: `<rect width="20" height="14" x="2" y="3" rx="2"/><path d="M8 21h8"/><path d="M12 17v4"/>`,
    },
    {
      translationKey: 'translation.app',
      url: '/app',
      iconSvg: `<rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/>`,
    },
    {
      translationKey: 'translation.notify',
      url: '/notify',
      iconSvg: `<path d="M10.268 21a2 2 0 0 0 3.464 0"/><path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 3.51A6 6 0 0 0 12 3"/>`,
    },
    {
      translationKey: 'translation.setting',
      url: '/setting',
      iconSvg: `<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>`,
    },
  ];

  isActive(url: string): boolean {
    return this.router.isActive(url, {
      paths: 'exact',
      matrixParams: 'ignored',
      queryParams: 'ignored',
      fragment: 'ignored',
    });
  }

  toggleCollapse(): void {
    this.isCollapse.update(v => {
      const next = !v;
      localStorage.setItem('navIsCollapse', String(next));
      return next;
    });
  }

  private getStoredCollapse(): boolean {
    const stored = localStorage.getItem('navIsCollapse');
    return stored === 'true';
  }
}
