use crate::NoSubscriberService;
use crate::persistence::{EncodeBytesSubscriber, RECORD_BLOCK_SIZE, RWS, RecordsPersistenceToFile};
use bytes::Bytes;
use chrono::Utc;
use futures_util::StreamExt;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::yield_now;
use tonic::transport::Channel;
use tracing::{error, warn};
use tracing_lv_core::TracingLiveMsgSubscriber;
use tracing_lv_core::proto::app_run_replay::Variant;
use tracing_lv_core::proto::tracing_service_client::TracingServiceClient;
use tracing_lv_core::proto::{AppStart, RecordParam, record_param};

pub struct TLReconnectAndPersistenceSetting {
    pub records_writer: Arc<Mutex<dyn RWS + Send>>,
    pub reconnect_interval: Vec<Duration>,
    pub compression_level: i32,
}

impl TLReconnectAndPersistenceSetting {
    pub fn from_file(records_file: std::fs::File) -> Result<Self, std::io::Error> {
        Ok(Self {
            records_writer: Arc::new(Mutex::new(records_file)),
            reconnect_interval: vec![
                Duration::from_secs(0),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(8),
                Duration::from_secs(32),
                Duration::from_secs(64),
                Duration::from_secs(256),
            ],
            compression_level: 8,
        })
    }
}
pub async fn reconnect_and_persistence(
    setting: TLReconnectAndPersistenceSetting,
    msg_sender: flume::Sender<RecordParam>,
    msg_receiver: flume::Receiver<RecordParam>,
    mut app_start: AppStart,
    mut client: TracingServiceClient<NoSubscriberService<Channel>>,
) -> (
    Box<dyn TracingLiveMsgSubscriber>,
    impl Future<Output = ()> + Send + 'static,
) {
    let (sender, receiver) = flume::unbounded::<Option<(Bytes, u64)>>();
    enum RecordMsg {
        NewRecord(u64),
        Close {
            reply_sender: tokio::sync::oneshot::Sender<()>,
        },
        ReConnect {
            receiver: tokio::sync::oneshot::Receiver<u64>,
        },
    }
    let (record_index_sender, record_index_receiver) = flume::unbounded::<RecordMsg>();

    let run_id = u128::from_be_bytes(app_start.run_id.as_slice().try_into().unwrap());
    let _read_file_and_send_task = {
        use bytes::Buf;
        let mut buf_recv = Vec::with_capacity(RECORD_BLOCK_SIZE);
        let mut file = RecordsPersistenceToFile::new(setting.compression_level).unwrap();
        {
            let mut records_io = setting.records_writer.lock().unwrap();
            file.init(run_id, &mut *records_io).unwrap();
        }

        let _task_handle: tokio::task::JoinHandle<()> = tokio::spawn({
            let records_io = setting.records_writer.clone();
            let record_index_sender = record_index_sender.clone();
            async move {
                let mut is_close = false;
                while let Ok(n) = receiver.recv_async().await {
                    let Some(n) = n else {
                        break;
                    };
                    buf_recv.clear();
                    buf_recv.push(n);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    while let Ok(n) = receiver.try_recv() {
                        let Some(n) = n else {
                            is_close = true;
                            break;
                        };
                        buf_recv.push(n);
                        if buf_recv.len() == RECORD_BLOCK_SIZE {
                            break;
                        }
                    }
                    let mut records_io = records_io.lock().unwrap();
                    file.write_frames(
                        &mut *records_io,
                        buf_recv.iter().map(|n| (n.0.chunk(), n.1)),
                    )
                    .unwrap();
                    if record_index_sender
                        .send(RecordMsg::NewRecord(buf_recv[0].1))
                        .is_err()
                    {
                        break;
                    }
                    if is_close {
                        break;
                    }
                }
            }
        });

        tokio::spawn({
            let msg_sender = msg_sender.clone();
            let sender = sender.clone();
            async move {
                let mut received_msg = record_index_receiver.recv_async().await;
                // let mut cur_record_index = 0;
                let mut file = RecordsPersistenceToFile::new(setting.compression_level).unwrap();
                let mut send_msgs = |cur_record_index: u64| {
                    let mut records_io = setting.records_writer.lock().unwrap();

                    let _metadata = { file.init_exist(run_id, &mut *records_io).unwrap() };
                    file.iter_from(
                        &mut *records_io,
                        &_metadata,
                        &_metadata.current_instance_footer,
                        cur_record_index,
                        |msg| {
                            // println!("msg: {msg:?}");
                            // *cur_record_index = msg.record_index();
                            let _ = msg_sender.send(msg.into());
                        },
                    )
                    .unwrap();
                };
                while let Ok(msg) = received_msg {
                    match msg {
                        RecordMsg::NewRecord(_record_index) => {
                            // cur_record_index = _record_index;
                            if msg_sender.is_disconnected() {
                                // println!("msg_sender.is_disconnected()");
                                break;
                            }
                            send_msgs(_record_index);
                            // println!("NewRecord cur_record_index: {cur_record_index:?}");
                        }
                        RecordMsg::ReConnect { receiver } => match receiver.await {
                            Ok(record_index) => {
                                // cur_record_index = record_index + 1;
                                send_msgs(record_index + 1);
                            }
                            Err(_err) => {
                                received_msg = record_index_receiver.recv_async().await;
                                continue;
                            }
                        },
                        RecordMsg::Close { reply_sender } => {
                            sender.send(None).unwrap();
                            _task_handle.await.unwrap();
                            while let Ok(msg) = record_index_receiver.try_recv() {
                                match msg {
                                    RecordMsg::NewRecord(record_index) => {
                                        send_msgs(record_index);
                                    }
                                    RecordMsg::Close { .. } => {
                                        warn!(
                                            "Close should not be sent to record_index_receiver  when already close"
                                        );
                                    }
                                    RecordMsg::ReConnect { .. } => {
                                        warn!(
                                            "ReConnect should not be sent to record_index_receiver when already close"
                                        );
                                    }
                                }
                            }
                            reply_sender.send(()).unwrap();
                            break;
                        }
                    }
                    received_msg = record_index_receiver.recv_async().await;
                }
            }
        })
    };

    (Box::new(EncodeBytesSubscriber { sender }) as _, {
        let msg_sender = msg_sender.clone();
        async move {
            let mut reconnect_times = 0;
            let mut reconnect_sender: Option<tokio::sync::oneshot::Sender<u64>> = None;
            'r: loop {
                let r =
                   client
                      .app_run(futures_util::stream::unfold(
                          (
                              msg_receiver.clone(),
                              record_index_sender.clone(),
                              None,
                              false,
                          ),
                          move |(
                                    msg_receiver,
                                    record_index_sender,
                                    mut app_stop,
                                    is_end,
                                )| async move {
                              if is_end {
                                  return None;
                              }
                              let (mut param, app_stop, is_end) = if app_stop.is_some() {
                                  yield_now().await;
                                  let param = msg_receiver
                                     .try_recv()
                                     .ok()
                                     .or_else(|| app_stop.take())
                                     .unwrap();
                                  let is_end = app_stop.is_none();
                                  (param, app_stop, is_end)
                              } else {
                                  let param = msg_receiver.recv_async().await.ok()?;
                                  if matches!(param.variant.as_ref().unwrap(),record_param::Variant::AppStop(_)) {
                                      let mut app_stop = Some(param);
                                      yield_now().await;
                                      let (reply_sender, reply_receiver) =
                                         tokio::sync::oneshot::channel();
                                      record_index_sender
                                         .send(RecordMsg::Close { reply_sender })
                                         .unwrap();
                                      let _ = reply_receiver.await;
                                      // tokio::time::sleep(Duration::from_secs(10)).await;
                                      let param = msg_receiver
                                         .try_recv()
                                         .ok()
                                         .or_else(|| app_stop.take())
                                         .unwrap();
                                      let is_end = app_stop.is_none();
                                      (param, app_stop, is_end)
                                  } else {
                                      (param, None, false)
                                  }
                              };
                              param.send_time = Utc::now().timestamp_nanos_opt().unwrap();
                              Some((
                                  param,
                                  (msg_receiver, record_index_sender, app_stop, is_end),
                              ))
                          },
                      ))
                      .await;
                match r {
                    Ok(mut record_param) => {
                        reconnect_times = 0;
                        while let Some(reply) = record_param.get_mut().next().await {
                            match reply {
                                Ok(reply) => match reply.variant.unwrap() {
                                    Variant::ReconnectReply(reply) => {
                                        let sender = reconnect_sender.take().unwrap();
                                        if sender.send(reply.last_record_index).is_err() {
                                            break;
                                        }
                                    }
                                },
                                Err(err) => {
                                    eprintln!("error: {err:?}");
                                    eprintln!("reconnect");
                                    app_start.reconnect = true;
                                    let (sender, receiver) = tokio::sync::oneshot::channel();
                                    if record_index_sender
                                        .send(RecordMsg::ReConnect { receiver })
                                        .is_err()
                                    {
                                        break;
                                    }
                                    reconnect_sender = Some(sender);
                                    while msg_receiver.try_recv().is_ok() {}
                                    msg_sender
                                        .send(RecordParam {
                                            send_time: Utc::now().timestamp_nanos_opt().unwrap(),
                                            record_index: 0,
                                            variant: Some(record_param::Variant::AppStart(
                                                app_start.clone(),
                                            )),
                                        })
                                        .unwrap();
                                    continue 'r;
                                }
                            }
                        }
                        // println!("server app_run stream end");
                        return;
                    }
                    Err(err) => {
                        // setting.reconnect_and_persistence
                        reconnect_times += 1;
                        error!("app run error end. {err:?}");
                        eprintln!("app run error end. {err:?}");
                        let timeout = setting
                            .reconnect_interval
                            .get(reconnect_times)
                            .cloned()
                            .unwrap_or(
                                setting
                                    .reconnect_interval
                                    .last()
                                    .cloned()
                                    .unwrap_or(Duration::from_secs(3)),
                            );
                        tokio::time::sleep(timeout).await;
                    }
                }
            }
        }
    })
}
