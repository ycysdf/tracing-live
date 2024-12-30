import {JSX, splitProps} from "solid-js";
import HTMLAttributes = JSX.HTMLAttributes;
import {t} from "i18next";
import {cn} from "~/lib/utils";

export function AppEmpty(allProps: HTMLAttributes<HTMLDivElement>) {
  let [props, rootProps] = splitProps(allProps, ["children", "class"]);
  return (
    <div class={cn("items-center justify-center p-2", props.class)} {...rootProps}>{t('common:noData')}</div>
  )
}