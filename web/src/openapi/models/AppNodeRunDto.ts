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
/**
 * 
 * @export
 * @interface AppNodeRunDto
 */
export interface AppNodeRunDto {
    /**
     * 
     * @type {Array<Array<any>>}
     * @memberof AppNodeRunDto
     */
    appBuildIds: Array<Array<any>>;
    /**
     * 
     * @type {string}
     * @memberof AppNodeRunDto
     */
    appRunId: string;
    /**
     * 
     * @type {Date}
     * @memberof AppNodeRunDto
     */
    creationTime: Date;
    /**
     * 
     * @type {object}
     * @memberof AppNodeRunDto
     */
    data: object;
    /**
     * 
     * @type {boolean}
     * @memberof AppNodeRunDto
     */
    exceptionEnd: boolean;
    /**
     * 
     * @type {string}
     * @memberof AppNodeRunDto
     */
    nodeId: string;
    /**
     * 
     * @type {number}
     * @memberof AppNodeRunDto
     */
    recordId: number;
    /**
     * 
     * @type {Date}
     * @memberof AppNodeRunDto
     */
    startTime: Date;
    /**
     * 
     * @type {number}
     * @memberof AppNodeRunDto
     */
    stopRecordId?: number | null;
    /**
     * 
     * @type {Date}
     * @memberof AppNodeRunDto
     */
    stopTime?: Date | null;
}

/**
 * Check if a given object implements the AppNodeRunDto interface.
 */
export function instanceOfAppNodeRunDto(value: object): value is AppNodeRunDto {
    if (!('appBuildIds' in value) || value['appBuildIds'] === undefined) return false;
    if (!('appRunId' in value) || value['appRunId'] === undefined) return false;
    if (!('creationTime' in value) || value['creationTime'] === undefined) return false;
    if (!('data' in value) || value['data'] === undefined) return false;
    if (!('exceptionEnd' in value) || value['exceptionEnd'] === undefined) return false;
    if (!('nodeId' in value) || value['nodeId'] === undefined) return false;
    if (!('recordId' in value) || value['recordId'] === undefined) return false;
    if (!('startTime' in value) || value['startTime'] === undefined) return false;
    return true;
}

export function AppNodeRunDtoFromJSON(json: any): AppNodeRunDto {
    return AppNodeRunDtoFromJSONTyped(json, false);
}

export function AppNodeRunDtoFromJSONTyped(json: any, ignoreDiscriminator: boolean): AppNodeRunDto {
    if (json == null) {
        return json;
    }
    return {
        
        'appBuildIds': json['app_build_ids'],
        'appRunId': json['app_run_id'],
        'creationTime': (new Date(json['creation_time'])),
        'data': json['data'],
        'exceptionEnd': json['exception_end'],
        'nodeId': json['node_id'],
        'recordId': json['record_id'],
        'startTime': (new Date(json['start_time'])),
        'stopRecordId': json['stop_record_id'] == null ? undefined : json['stop_record_id'],
        'stopTime': json['stop_time'] == null ? undefined : (new Date(json['stop_time'])),
    };
}

  export function AppNodeRunDtoToJSON(json: any): AppNodeRunDto {
      return AppNodeRunDtoToJSONTyped(json, false);
  }

  export function AppNodeRunDtoToJSONTyped(value?: AppNodeRunDto | null, ignoreDiscriminator: boolean = false): any {
    if (value == null) {
        return value;
    }

    return {
        
        'app_build_ids': value['appBuildIds'],
        'app_run_id': value['appRunId'],
        'creation_time': ((value['creationTime']).toISOString()),
        'data': value['data'],
        'exception_end': value['exceptionEnd'],
        'node_id': value['nodeId'],
        'record_id': value['recordId'],
        'start_time': ((value['startTime']).toISOString()),
        'stop_record_id': value['stopRecordId'],
        'stop_time': value['stopTime'] == null ? undefined : ((value['stopTime'] as any).toISOString()),
    };
}

