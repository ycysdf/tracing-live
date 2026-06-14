import {
  ChangeDetectorRef,
  Component,
  Host, HostBinding,
  inject,
  Injector,
  Input,
  Signal,
  signal,
  WritableSignal
} from '@angular/core';
import {RouterLink, RouterLinkActive, RouterOutlet} from '@angular/router';
import {SelectionModel} from '@angular/cdk/collections';
import {JsonPipe, NgClass} from '@angular/common';
// import * as n from 'wasm_play';
import {toSignal} from '@angular/core/rxjs-interop';
import * as rx from 'rxjs';
import {FormBuilder, NgForm} from '@angular/forms';

@Component({
  selector: 'tracing-tree-item',
  template: `
  `
})
export class TracingTreeItemComponent {

}


@Component({
  selector: 'app-nav',
  imports: [
    RouterLink,
    RouterLinkActive,
    NgClass
  ],
  host: {
    class: "max-w-[180px] border-r-gray-200 transition-[width] bg-background shadow-sm flex flex-col justify-between py-2 h-screen",
  },
  template: `
    <a [routerLink]="['/']"
       class="h-10 cursor-pointer mb-2 leading-none flex justify-center items-center font-bold text-xl text-nowrap">
      {{ isCollapse() ? "TL" : "Tracing Live" }}
    </a>
    <nav class="flex flex-grow flex-col border-t border-t-border text-muted-foreground">
      @for (item of links; track item.name) {
        <a [routerLink]="item.url" [title]="item.name"
           routerLinkActive="text-primary bg-stone-100 border-r-2 border-r-primary font-bold"
           class="border-r-2 py-3 border-transparent gap-3 transition-[width] hover:bg-stone-50 flex items-center text-nowrap"
           [ngClass]="[isCollapse()?'justify-center':'px-3']">
          {{ item.name }}
        </a>
      }
    </nav>
    <div>
      <div (click)="onToggle()" class="p-3">
        Toggle
      </div>
    </div>
  `
})
export class AppNavComponent {
// {{"width": isCollapse() ? `${height}px` : "180px"}}
  @Input()
  isCollapse = signal(false);

  // @Host("style.width")
  // width
  links = [

    {
      name: "overview",
      url: `index`,
      // icon: <Kanban size={16} strokeWidth="1"/>,
      // activeIcon: <Kanban size={16} strokeWidth="2"/>
    },
    {
      name: "trace",
      url: `trace`,
      // icon: <ListTree size={16} strokeWidth="1"/>,
      // activeIcon: <ListTree size={16} strokeWidth="2"/>
    },
    {
      name: "node",
      url: `node`,
      // icon: <Monitor size={16} strokeWidth="1"/>,
      // activeIcon: <Monitor size={16} strokeWidth="2"/>
    },
    {
      name: "app",
      url: `app`,
      // icon: <LayoutGrid size={16} strokeWidth="1"/>,
      // activeIcon: <LayoutGrid size={16} strokeWidth="2"/>
    },
    {
      name: "notify",
      url: `notify`,
      // icon: <BellRing size={16} strokeWidth="1"/>,
      // activeIcon: <BellRing size={16} strokeWidth="2"/>
    },
    {
      name: "setting",
      url: `setting`,
      // icon: <Settings size={16} strokeWidth="1"/>,
      // activeIcon: <Settings size={16} strokeWidth="2"/>
    },
  ];

  onToggle() {
    this.isCollapse.update((n: boolean) => !n)
  }
}

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, JsonPipe, AppNavComponent],
  templateUrl: './app.component.html',
  styleUrl: './app.component.css'
})
export class AppComponent {
  title = 'tracing-live';
  jobs = [{}, {}, {}, {}]
  selectedJob: SelectionModel<any> = new SelectionModel(false);
  data?: Signal<any>;

  constructor(injector: Injector, changeDetectorRef: ChangeDetectorRef) {
    // this.test(injector, changeDetectorRef)
  }

  /*
    async test(injector: Injector, changeDetectorRef: ChangeDetectorRef) {
      n.debug();
      let channel = await n.rpc_test_service("http://localhost:30002/rpc_test")


      await channel.msg_streaming((async function* () {
        yield "1";
        yield "2";
        yield "3";
      })());
      this.data = toSignal(rx.from(await channel.reply_streaming()), {
        injector: injector
      });
      changeDetectorRef.detectChanges();
      // setTimeout(() => {
      //   console.log('AppComponent loaded');
      //   let r = n.test_wasm();
      //   console.log(r);
      //   (async () => {
      //     for await (const value of r) {
      //       console.log("item:");
      //       console.log(value);
      //     }
      //   })();
      // }, 3000);
    }*/
}


