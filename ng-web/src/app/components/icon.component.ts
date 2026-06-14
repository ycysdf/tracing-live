import { Component, input, computed, HostBinding } from '@angular/core';

export type IconName = 'chevron-right' | 'check' | 'dot' | 'arrow-right' | 'go-here' | 'collapse-left' | 'collapse-right';

const ICONS: Record<IconName, string> = {
  'chevron-right': `<path d="m9 18 6-6-6-6"/>`,
  'check': `<path d="M5 12l5 5l10 -10"/>`,
  'dot': `<circle cx="12" cy="12" r="4"/>`,
  'arrow-right': `<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><polyline points="10 17 15 12 10 7"/><line x1="15" x2="3" y1="12" y2="12"/>`,
  'go-here': `<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><polyline points="10 17 15 12 10 7"/><line x1="15" x2="3" y1="12" y2="12"/>`,
  'collapse-left': `<path d="M13 17l5-5-5-5"/><path d="M6 17V7"/>`,
  'collapse-right': `<path d="M11 17l-5-5 5-5"/><path d="M18 17V7"/>`,
};

@Component({
  selector: 'svg[appIcon]',
  template: '',
  host: {
    '[attr.xmlns]': '"http://www.w3.org/2000/svg"',
    '[attr.viewBox]': '"0 0 24 24"',
    '[attr.fill]': '"none"',
    '[attr.stroke]': '"currentColor"',
    '[attr.stroke-linecap]': '"round"',
    '[attr.stroke-linejoin]': '"round"',
    '[innerHTML]': 'iconSvg()',
  },
})
export class IconComponent {
  readonly name = input.required<IconName>({ alias: 'appIcon' });
  readonly iconSvg = computed(() => ICONS[this.name()] ?? '');
}
