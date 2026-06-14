import { Observable } from 'rxjs';

/**
 * Creates an RxJS Observable that wraps a browser EventSource.
 * The EventSource is created on subscribe and closed on unsubscribe.
 * Emits the raw `data` string from each SSE message.
 */
export function fromEventSource(
  url: string,
  options?: EventSourceInit,
): Observable<string> {
  return new Observable<string>((subscriber) => {
    const eventSource = new EventSource(url, options);

    eventSource.onmessage = (event) => {
      subscriber.next(event.data as string);
    };

    eventSource.onerror = () => {
      subscriber.error(new Error('EventSource error'));
      eventSource.close();
    };

    return () => {
      eventSource.close();
    };
  });
}
