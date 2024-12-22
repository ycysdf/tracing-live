// import {getApps} from "~/cache";
// import {createAsync, useNavigate} from "@solidjs/router";
// import {For, Suspense} from "solid-js";
// import {Link} from "@kobalte/core/link";
// import {Loading} from "~/components/Loading";
//
// export const route = {
//   preload: async () => {
//     await getApps();
//   },
// };
//
// export default function AppSelect() {
//   let apps = createAsync(() => getApps());
//   if (apps.latest?.length == 1) {
//     useNavigate()(`/${apps.latest?.at(0)?.id}`)
//   }
//   return (
//     <div class="h-screen w-screen flex justify-center items-start bg-gray-50">
//       <Suspense fallback={<Loading/>}>
//         <div class="mt-[20%] shadow-sm pb-4 flex flex-col border flex-grow-0 flex-shrink bg-white">
//           <div class="border-b py-4 text-center px-4 mb-4 font-bold">选择一个你要查看的应用</div>
//           <For each={apps()} fallback={<div class={"p-4"}>No Found Apps</div>}>
//             {n => <Link href={`${n.id}`}
//                         class="px-4 cursor-pointer py-2 flex-col gap-2 border-y border-y-gray-100 hover:bg-primary/10">
//               <div class="text-base font-bold">{n.name} {n.version}</div>
//               <div class="text-sm">{n.id}</div>
//             </Link>}
//           </For>
//         </div>
//       </Suspense>
//     </div>
//   )
// }