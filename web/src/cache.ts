import {cache} from "@solidjs/router";
import {HttpClient} from "~/http_client";
import {ListNodesRequest, ListRecordsRequest, NodesPageRequest} from "~/openapi";

export const getRecords = cache(async (params: ListRecordsRequest) => {
  return await HttpClient.listRecords(params)
}, "records");


export const getApps = cache(async () => {
  return await HttpClient.listApps()
}, "apps");

export const getNodes = cache(async (params: ListNodesRequest) => {
  return await HttpClient.listNodes(params)
}, "apps");
export const getNodesPage = cache(async (params: NodesPageRequest) => {
  return await HttpClient.nodesPage(params)
}, "apps");