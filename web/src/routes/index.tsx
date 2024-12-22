import {A, RouteSectionProps, useLocation, useNavigate, useParams} from "@solidjs/router";
import {Select, SelectContent, SelectItem, SelectTrigger, SelectValue} from "~/components/ui/select";
import {getApps} from "~/cache";
import {createResource, For, JSX, Show, Suspense} from "solid-js";
import {AppLatestInfoDto} from "~/openapi";
import {Link} from "@kobalte/core/link";
import {BellRing, GitBranch, Kanban, LayoutGrid, ListTree, Logs, Settings} from "lucide-solid";

interface NavItem {
  name: string,
  url: string,
  icon: JSX.Element,
  activeIcon: JSX.Element,
}

export function Nav() {
  const location = useLocation();
  let links: NavItem[] = [
    {
      name: "概况",
      url: `index`,
      icon: <Kanban size={16} strokeWidth="1"/>,
      activeIcon: <Kanban size={16} strokeWidth="2"/>
    },
    {
      name: "追踪",
      url: `trace`,
      icon: <ListTree size={16} strokeWidth="1"/>,
      activeIcon: <ListTree size={16} strokeWidth="2"/>
    },
    {
      name: "节点",
      url: `nodes`,
      icon: <LayoutGrid size={16} strokeWidth="1"/>,
      activeIcon: <LayoutGrid size={16} strokeWidth="2"/>
    },
    {
      name: "应用",
      url: `app`,
      icon: <LayoutGrid size={16} strokeWidth="1"/>,
      activeIcon: <LayoutGrid size={16} strokeWidth="2"/>
    },
    {
      name: "通知",
      url: `notify`,
      icon: <BellRing size={16} strokeWidth="1"/>,
      activeIcon: <BellRing size={16} strokeWidth="2"/>
    },
    {
      name: "设置",
      url: `setting`,
      icon: <Settings size={16} strokeWidth="1"/>,
      activeIcon: <Settings size={16} strokeWidth="2"/>
    },
  ]
  return (
    <div class="border-r-gray-200 bg-background shadow-sm flex flex-col justify-between py-4 h-screen"
         style={{"min-width": "180px"}}>
      <Link href="/"
            class="h-10 cursor-pointer border-b-border pb-4 border-b flex justify-center items-center font-bold text-xl">
        Tracing Live
      </Link>
      <nav class="flex flex-grow flex-col text-muted-foreground">
        <For each={links}>
          {n => <>
            <A href={n.url} class={"px-3 h-12 gap-3 hover:bg-stone-50 flex items-center"}
               activeClass={"text-primary bg-stone-100 border-r-2 border-r-primary font-bold"}
               inactiveClass={"border-r-2 border-transparent"}>
              {location.pathname == n.url ? n.activeIcon : n.icon}
              <span>{n.name}</span>
            </A>
          </>}
        </For>
      </nav>
      <div>
        {/*<AppSelect></AppSelect>*/}
      </div>
    </div>
  );
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