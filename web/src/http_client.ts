import {Configuration, DefaultApi} from "~/openapi";
import qs from 'qs';
import {BASE_URL} from "~/consts";

export const HttpClient = new DefaultApi(new Configuration({
  basePath: BASE_URL,
  queryParamsStringify(param) {
    return qs.stringify(param, {
      arrayFormat: 'indices',
    })
  }
}));