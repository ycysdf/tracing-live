/* tslint:disable */
/* eslint-disable */
/**
 * tracing-lv-server
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document: 0.1.0
 *
 *
 * NOTE: This class is auto generated by OpenAPI Generator (https://openapi-generator.tech).
 * https://openapi-generator.tech
 * Do not edit the class manually.
 */

/**
 * @type ListRecordsParentTIdParameter
 *
 * @export
 */
export type ListRecordsParentTIdParameter = number;

export function ListRecordsParentTIdParameterFromJSON(json: any): ListRecordsParentTIdParameter {
  return ListRecordsParentTIdParameterFromJSONTyped(json, false);
}

export function ListRecordsParentTIdParameterFromJSONTyped(json: any, ignoreDiscriminator: boolean): ListRecordsParentTIdParameter {
  if (json == null) {
    return json;
  }
  if (instanceOfnumber(json)) {
    return numberFromJSONTyped(json, true);
  }

  return {} as any;
}

export function ListRecordsParentTIdParameterToJSON(value?: ListRecordsParentTIdParameter | null): any {
  if (value == null) {
    return value;
  }

  if (instanceOfnumber(value)) {
    return numberToJSON(value as number);
  }

  return {};
}

