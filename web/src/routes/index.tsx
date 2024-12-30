import {A, RouteSectionProps, useLocation, useNavigate, useParams} from "@solidjs/router";
import {Select, SelectContent, SelectItem, SelectTrigger, SelectValue} from "~/components/ui/select";
import {getApps} from "~/cache";
import {createResource, createSignal, For, JSX, Show, Suspense} from "solid-js";
import {AppLatestInfoDto} from "~/openapi";
import {Link} from "@kobalte/core/link";
import {
  ArrowLeftFromLine,
  ArrowRightFromLine,
  BellRing,
  GitBranch,
  Kanban,
  LayoutGrid,
  ListTree,
  Logs,
  Settings
} from "lucide-solid";
import {makePersisted} from "@solid-primitives/storage";
import {cn} from "~/lib/utils";
import {t} from "i18next";

interface NavItem {
  name: string,
  url: string,
  icon: JSX.Element,
  activeIcon: JSX.Element,
}


// function AppSelect() {
//   const navigate = useNavigate();
//   let [apps] = createResource(() => getApps());
//   return (
//     <Suspense>
//       <Show when={apps()}>
//         <Select
//           onChange={n => {
//             return navigate(`/${n?.id}`);
//           }}
//           value={apps()?.find(e => e.id === appId)}
//           // value={appId}
//           selectionBehavior='replace'
//           disallowEmptySelection={true}
//           options={apps() ?? []}
//           optionValue={n => n.id}
//           optionTextValue={n => `${n.name} ${n.version}`}
//           itemComponent={(props) => {
//             return <SelectItem item={props.item}>{props.item.textValue}</SelectItem>
//           }}
//         >
//           <SelectTrigger aria-label="Fruit" class="w-[180px]">
//             <SelectValue<AppLatestInfoDto>>{(state) => {
//               let n = state.selectedOption();
//               return `${n.name} ${n.version}`;
//             }}</SelectValue>
//           </SelectTrigger>
//           <SelectContent/>
//         </Select>
//       </Show>
//     </Suspense>
//   )
// }
export function Nav() {
  const location = useLocation();
  let links: NavItem[] = [
    {
      name: t("overview"),
      url: `index`,
      icon: <Kanban size={16} strokeWidth="1"/>,
      activeIcon: <Kanban size={16} strokeWidth="2"/>
    },
    {
      name: t("trace"),
      url: `trace`,
      icon: <ListTree size={16} strokeWidth="1"/>,
      activeIcon: <ListTree size={16} strokeWidth="2"/>
    },
    {
      name: t("node"),
      url: `node`,
      icon: <LayoutGrid size={16} strokeWidth="1"/>,
      activeIcon: <LayoutGrid size={16} strokeWidth="2"/>
    },
    {
      name: t("app"),
      url: `app`,
      icon: <LayoutGrid size={16} strokeWidth="1"/>,
      activeIcon: <LayoutGrid size={16} strokeWidth="2"/>
    },
    {
      name: t("notify"),
      url: `notify`,
      icon: <BellRing size={16} strokeWidth="1"/>,
      activeIcon: <BellRing size={16} strokeWidth="2"/>
    },
    {
      name: t("setting"),
      url: `setting`,
      icon: <Settings size={16} strokeWidth="1"/>,
      activeIcon: <Settings size={16} strokeWidth="2"/>
    },
  ]
  let height = 48;
  let [isCollapse, setIsCollapse] = makePersisted(createSignal(false), {name: 'navIsCollapse'});
  return (
    <div
      class="border-r-gray-200 transition-[width] bg-background shadow-sm flex flex-col justify-between py-2 h-screen"
      style={{"width": isCollapse() ? `${height}px` : "180px"}}>
      <Link href="/"
            class="h-10 cursor-pointer mb-2 leading-none flex justify-center items-center font-bold text-xl text-nowrap">
        {isCollapse() ? "TL" : "Tracing Live"}
      </Link>
      <nav class="flex flex-grow flex-col border-t border-t-border text-muted-foreground">
        <For each={links}>
          {n => <>
            <A href={n.url}
               class={cn("gap-3 transition-[width] hover:bg-stone-50 flex items-center text-nowrap", isCollapse() ? "justify-center" : "px-3")}
               style={{"height": `${height}px`}}
               title={n.name}
               activeClass={"text-primary bg-stone-100 border-r-2 border-r-primary font-bold"}
               inactiveClass={"border-r-2 border-transparent"}>
              {location.pathname == n.url ? n.activeIcon : n.icon}
              <Show when={!isCollapse()}>
                <span>{n.name}</span>
              </Show>
            </A>
          </>}
        </For>
      </nav>
      <div>
        <div class={"flex select-none items-center h-12 px-4 py-2 cursor-pointer hover:bg-stone-50"}
             onClick={() => setIsCollapse(n => !n)}>
          <Show when={isCollapse()}
                fallback={<ArrowLeftFromLine strokeWidth={2} size={16} class={""}></ArrowLeftFromLine>}>
            <ArrowRightFromLine strokeWidth={2} size={16} class={""}></ArrowRightFromLine>
          </Show>
        </div>
      </div>
    </div>
  );
}

export function AppLayout(props: RouteSectionProps<{}>) {
  return (
    <>
      <div class="flex items-stretch">
        <Nav/>
        <div class="flex-grow overflow-hidden flex flex-col gap-3 p-3 bg-gray-50 h-screen">{props.children}</div>
      </div>
    </>
  )
}