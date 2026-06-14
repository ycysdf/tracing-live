import { Injectable } from '@angular/core';
import { Observable, map, share, retry, catchError, EMPTY } from 'rxjs';
import { TracingRecordScene, type TracingTreeRecordDto } from '../../../api';
import { BASE_URL } from '../../utils/constants';
import { fromEventSource } from '../../utils/event-source';

export interface LiveSubscriptionConfig {
  appRunId: string | null;
  spanTId: string | null;
  isEnd: boolean;
}

@Injectable()
export class LiveRecordsService {
  subscribe(config: LiveSubscriptionConfig): Observable<TracingTreeRecordDto> {
    if (config.isEnd) {
      return EMPTY;
    }

    const params = new URLSearchParams();
    params.set('count', '51');
    if (config.appRunId) {
      params.set('app_run_ids', JSON.stringify([config.appRunId]));
    }
    if (config.spanTId) {
      params.set('parent_span_t_ids', JSON.stringify([config.spanTId]));
    }
    params.set('scene', TracingRecordScene.Tree);

    const url = `${BASE_URL}/records_subscribe?${params.toString()}`;

    return fromEventSource(url).pipe(
      map((data) => JSON.parse(data) as TracingTreeRecordDto),
      retry({ count: 3, delay: 1000 }),
      catchError(() => EMPTY),
      share(),
    );
  }
}
