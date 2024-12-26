import {Component, JSX, splitProps} from "solid-js";
import styles from './loading.module.css'
import {cn} from "~/lib/utils";

export const Loading: Component<JSX.HTMLAttributes<HTMLDivElement>> = allProps => {
  return (
    <div {...allProps} class={cn("flex flex-col items-center gap-3 p-3", allProps.class)}>
      <div class={`${styles['la-line-scale']} ${styles['la-1x']} text-primary`}>
        <div/>
        <div/>
        <div/>
        <div/>
        <div/>
      </div>
      {allProps.children}
    </div>
  )
}

export const LoadingPanel: Component<JSX.HTMLAttributes<HTMLDivElement>> = allProps => {
  return (
    <Loading
      {...allProps} class={cn(`panel py-12`, allProps.class)}>
      {allProps.children ??
        <div class="text-xsm">Loading..</div>}
    </Loading>
  )
}