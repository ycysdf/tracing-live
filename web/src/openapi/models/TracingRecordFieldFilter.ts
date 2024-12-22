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
import type { TLBinOp } from './TLBinOp';
import {
    TLBinOpFromJSON,
    TLBinOpFromJSONTyped,
    TLBinOpToJSON,
    TLBinOpToJSONTyped,
} from './TLBinOp';

/**
 * 
 * @export
 * @interface TracingRecordFieldFilter
 */
export interface TracingRecordFieldFilter {
    /**
     * 
     * @type {string}
     * @memberof TracingRecordFieldFilter
     */
    name: string;
    /**
     * 
     * @type {TLBinOp}
     * @memberof TracingRecordFieldFilter
     */
    op: TLBinOp;
    /**
     * 
     * @type {string}
     * @memberof TracingRecordFieldFilter
     */
    value?: string | null;
}



/**
 * Check if a given object implements the TracingRecordFieldFilter interface.
 */
export function instanceOfTracingRecordFieldFilter(value: object): value is TracingRecordFieldFilter {
    if (!('name' in value) || value['name'] === undefined) return false;
    if (!('op' in value) || value['op'] === undefined) return false;
    return true;
}

export function TracingRecordFieldFilterFromJSON(json: any): TracingRecordFieldFilter {
    return TracingRecordFieldFilterFromJSONTyped(json, false);
}

export function TracingRecordFieldFilterFromJSONTyped(json: any, ignoreDiscriminator: boolean): TracingRecordFieldFilter {
    if (json == null) {
        return json;
    }
    return {
        
        'name': json['name'],
        'op': TLBinOpFromJSON(json['op']),
        'value': json['value'] == null ? undefined : json['value'],
    };
}

  export function TracingRecordFieldFilterToJSON(json: any): TracingRecordFieldFilter {
      return TracingRecordFieldFilterToJSONTyped(json, false);
  }

  export function TracingRecordFieldFilterToJSONTyped(value?: TracingRecordFieldFilter | null, ignoreDiscriminator: boolean = false): any {
    if (value == null) {
        return value;
    }

    return {
        
        'name': value['name'],
        'op': TLBinOpToJSON(value['op']),
        'value': value['value'],
    };
}

