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

import { mapValues } from '../runtime';
import type { AppNodeRunDto } from './AppNodeRunDto';
import {
    AppNodeRunDtoFromJSON,
    AppNodeRunDtoFromJSONTyped,
    AppNodeRunDtoToJSON,
    AppNodeRunDtoToJSONTyped,
} from './AppNodeRunDto';

/**
 * 
 * @export
 * @interface NodePageEventOneOf1
 */
export interface NodePageEventOneOf1 {
    /**
     * 
     * @type {AppNodeRunDto}
     * @memberof NodePageEventOneOf1
     */
    nodeAppStart: AppNodeRunDto;
}

/**
 * Check if a given object implements the NodePageEventOneOf1 interface.
 */
export function instanceOfNodePageEventOneOf1(value: object): value is NodePageEventOneOf1 {
    if (!('nodeAppStart' in value) || value['nodeAppStart'] === undefined) return false;
    return true;
}

export function NodePageEventOneOf1FromJSON(json: any): NodePageEventOneOf1 {
    return NodePageEventOneOf1FromJSONTyped(json, false);
}

export function NodePageEventOneOf1FromJSONTyped(json: any, ignoreDiscriminator: boolean): NodePageEventOneOf1 {
    if (json == null) {
        return json;
    }
    return {
        
        'nodeAppStart': AppNodeRunDtoFromJSON(json['NodeAppStart']),
    };
}

  export function NodePageEventOneOf1ToJSON(json: any): NodePageEventOneOf1 {
      return NodePageEventOneOf1ToJSONTyped(json, false);
  }

  export function NodePageEventOneOf1ToJSONTyped(value?: NodePageEventOneOf1 | null, ignoreDiscriminator: boolean = false): any {
    if (value == null) {
        return value;
    }

    return {
        
        'NodeAppStart': AppNodeRunDtoToJSON(value['nodeAppStart']),
    };
}

