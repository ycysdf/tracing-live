import {createStore} from "solid-js/store";
import {For} from "solid-js";

// Name
// IP
// 启动时长 Version

function ConditionItem(props: { item: ConditionItem }) {
  return (
    <div>
      <div class="panel flex flex-col gap-2 min-[100px]">
        <input type="text"/>
        <input type="text"/>
        <input type="text"/>
      </div>
      <div>

      </div>
    </div>
  )
}

export enum ConditionType {
  All,
  Any
}

export interface Condition {
  negate: boolean,
  condition_type: ConditionType,
  conditions: ConditionItem[]
}

export type Expr = Unary | Binary;

export interface Unary {
  op: string,
  expr: Expr,
}

export interface Binary {
  left_expr: Expr,
  op: string,
  right_expr: Expr,
}

export type ConditionItem = Condition | Expr;

function ConditionBuilder() {
  let [conditions, setConditions] = createStore<Condition[]>([]);
  return (
    <>
      <For each={conditions}>
        {n => <ConditionItem item={n}/>}
      </For>
    </>
  )
}