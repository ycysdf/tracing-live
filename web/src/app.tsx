import {HashRouter, Route, useNavigate} from "@solidjs/router";
import {Suspense} from "solid-js";
import NotFound from "./routes/not-found";
import {Traces} from "./routes/traces";
import {AppLayout} from "./routes";
import {AppOverview} from "./routes/overview";
import {LoadingPanel} from "~/components/Loading";

export function App() {
  return (
    <HashRouter root={n => <Suspense fallback={<LoadingPanel/>}>{n.children}</Suspense>}>
      <Route component={AppLayout}>
        <Route path="/" component={() => {
          useNavigate()("index")
          return <></>
        }}/>
        <Route path="/index" component={AppOverview}/>
        <Route path="/trace" component={Traces}/>
        <Route path="/node" component={AppOverview}/>
        <Route path="/app" component={AppOverview}/>
        <Route path="/notify" component={AppOverview}/>
        <Route path="/*rest" component={NotFound}></Route>
      </Route>
      <Route path="/*rest" component={NotFound}></Route>
    </HashRouter>
  );
}
