// src/client/push.rs
use crate::DynStream;
use crate::client::perform_client_handshake;
use bytes::Bytes;
use rml_rtmp::sessions::{
    ClientSession, ClientSessionConfig, ClientSessionEvent, ClientSessionResult,
    PublishRequestType, StreamMetadata,
};
use rml_rtmp::time::RtmpTimestamp;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::task::JoinHandle;
use tokio_native_tls::{TlsConnector, native_tls};
use tracing::{error, info, trace};
use url::Url;

pub const MAX_BUFFER_SIZE: usize = 256;

pub struct ClientStateWrapper {
    pub session: ClientSession,
    pub prepublish_video_buffer: VecDeque<(Bytes, RtmpTimestamp)>,
    pub prepublish_audio_buffer: VecDeque<(Bytes, RtmpTimestamp)>,
    pub prepublish_metadata: Option<StreamMetadata>,
    pub video_sequence_header: Option<Bytes>,
    pub audio_sequence_header: Option<Bytes>,
}

impl ClientStateWrapper {
    pub fn buffer_video(&mut self, data: Bytes, timestamp: RtmpTimestamp) {
        if self.prepublish_video_buffer.len() >= MAX_BUFFER_SIZE {
            self.prepublish_video_buffer.pop_front();
        }
        self.prepublish_video_buffer.push_back((data, timestamp));
    }
    #[allow(dead_code)]
    pub fn buffer_audio(&mut self, data: Bytes, timestamp: RtmpTimestamp) {
        if self.prepublish_audio_buffer.len() >= MAX_BUFFER_SIZE {
            self.prepublish_audio_buffer.pop_front();
        }
        self.prepublish_audio_buffer.push_back((data, timestamp));
    }

    pub fn update_video_header(&mut self, data: Bytes) {
        self.video_sequence_header = Some(data);
    }

    pub fn update_audio_header(&mut self, data: Bytes) {
        self.audio_sequence_header = Some(data);
    }
}

pub struct PushClient {
    pub tx_feed: mpsc::Sender<Bytes>,
    pub client_state: Arc<RwLock<ClientStateWrapper>>,
    pub publish_ready_rx: watch::Receiver<bool>,
    pub url: Url,
    pub stream_key: String,
    _tasks: Vec<JoinHandle<()>>,
}

impl Drop for PushClient {
    fn drop(&mut self) {
        for task in &self._tasks {
            task.abort();
        }
        info!("PushClient dropped, background tasks aborted.");
    }
}

impl PushClient {
    fn send_packet(tx: &mpsc::Sender<Bytes>, result: ClientSessionResult) {
        if let ClientSessionResult::OutboundResponse(packet) = result {
            let _ = tx.try_send(Bytes::from(packet.bytes));
        }
    }

    pub async fn connect_and_publish(
        url: &Url,
        stream_key: String,
        cached_video_header: Option<Bytes>,
        cached_audio_header: Option<Bytes>,
        cached_metadata: Option<StreamMetadata>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let host = url.host_str().ok_or("Invalid host")?.to_string();
        let port = url.port_or_known_default().unwrap_or(1935);
        let addr = format!("{host}:{port}");

        let tcp_stream = TcpStream::connect(&addr).await?;
        let _ = tcp_stream.set_nodelay(true);

        let mut stream: DynStream = if url.scheme() == "rtmps" {
            let native = native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .build()?;
            let connector = TlsConnector::from(native);
            Box::new(connector.connect(&host, tcp_stream).await?)
        } else {
            Box::new(tcp_stream)
        };

        perform_client_handshake(&mut stream).await?;

        let mut client_cfg = ClientSessionConfig::new();
        let app_segment = url
            .path()
            .trim_start_matches('/')
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();

        client_cfg.tc_url = Some(format!("rtmp://{host}:{port}/{app_segment}"));

        let (mut session, initial_results) = ClientSession::new(client_cfg)?;
        let (tx, mut rx) = mpsc::channel::<Bytes>(256);
        let (kill_tx, mut kill_rx) = mpsc::channel::<()>(1);

        let (mut rd, mut wr) = tokio::io::split(stream);

        // Writer Task
        let writer_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = kill_rx.recv() => {
                        break;
                    }
                    msg = rx.recv() => {
                        match msg {
                            Some(bytes) => {
                                if wr.write_all(&bytes).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        for res in initial_results {
            Self::send_packet(&tx, res);
        }

        let res = session.request_connection(app_segment)?;
        Self::send_packet(&tx, res);

        let client_state = Arc::new(RwLock::new(ClientStateWrapper {
            session,
            prepublish_video_buffer: VecDeque::new(),
            prepublish_audio_buffer: VecDeque::new(),
            prepublish_metadata: cached_metadata,
            video_sequence_header: cached_video_header,
            audio_sequence_header: cached_audio_header,
        }));

        let (ready_tx, ready_rx) = watch::channel(false);
        let state_clone = client_state.clone();
        let tx_clone = tx.clone();
        let stream_key_clone = stream_key.clone();

        // Reader Task
        let reader_handle = tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                let n = match rd.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };

                let mut state = state_clone.write().await;
                let input_res =
                    catch_unwind(AssertUnwindSafe(|| state.session.handle_input(&buf[..n])));

                let results = match input_res {
                    Ok(Ok(res)) => res,
                    _ => break,
                };

                for res in results {
                    match res {
                        ClientSessionResult::OutboundResponse(packet) => {
                            let _ = tx_clone.try_send(Bytes::from(packet.bytes));
                        }
                        ClientSessionResult::RaisedEvent(ev) => match ev {
                            ClientSessionEvent::ConnectionRequestAccepted => {
                                if let Ok(res) = state.session.request_publishing(
                                    stream_key_clone.clone(),
                                    PublishRequestType::Live,
                                ) {
                                    Self::send_packet(&tx_clone, res);
                                }
                            }
                            // FIXED: Removed redundant { .. }
                            ClientSessionEvent::PublishRequestAccepted => {
                                info!("Publish succeeded for remote RTMP");
                                let _ = ready_tx.send(true);
                                Self::drain_buffers(&mut state, &tx_clone);
                            }
                            ClientSessionEvent::UnhandleableOnStatusCode { code } => {
                                info!("RTMP Status received: {}", code);
                                if code.contains("BadName")
                                    || code.contains("error")
                                    || code.contains("Failed")
                                {
                                    error!("Stopping stream due to RTMP status: {}", code);
                                    let _ = kill_tx.send(()).await;
                                    return;
                                }
                            }
                            ClientSessionEvent::ConnectionRequestRejected { description } => {
                                error!("RTMP Connection Rejected: {}", description);
                                let _ = kill_tx.send(()).await;
                                return;
                            }
                            _ => trace!("Client Event: {:?}", ev),
                        },
                        ClientSessionResult::UnhandleableMessageReceived(_) => {}
                    }
                }
            }
            let _ = ready_tx.send(false);
            let _ = kill_tx.send(()).await;
        });

        Ok(Self {
            tx_feed: tx,
            client_state,
            publish_ready_rx: ready_rx,
            url: url.clone(),
            stream_key,
            _tasks: vec![writer_handle, reader_handle],
        })
    }

    pub fn drain_buffers(state: &mut ClientStateWrapper, tx: &mpsc::Sender<Bytes>) {
        // FIXED: Collapsed nested if let
        if let Some(meta) = &state.prepublish_metadata
            && let Ok(res) = state.session.publish_metadata(meta)
        {
            Self::send_packet(tx, res);
        }

        // FIXED: Collapsed nested if let
        if let Some(header) = &state.video_sequence_header
            && let Ok(res) =
                state
                    .session
                    .publish_video_data(header.clone(), RtmpTimestamp::new(0), true)
        {
            Self::send_packet(tx, res);
        }

        // FIXED: Collapsed nested if let
        if let Some(header) = &state.audio_sequence_header
            && let Ok(res) =
                state
                    .session
                    .publish_audio_data(header.clone(), RtmpTimestamp::new(0), true)
        {
            Self::send_packet(tx, res);
        }

        while let Some((data, ts)) = state.prepublish_video_buffer.pop_front() {
            if let Ok(res) = state.session.publish_video_data(data, ts, true) {
                Self::send_packet(tx, res);
            }
        }
        while let Some((data, ts)) = state.prepublish_audio_buffer.pop_front() {
            if let Ok(res) = state.session.publish_audio_data(data, ts, true) {
                Self::send_packet(tx, res);
            }
        }
    }

    pub async fn shutdown(&self) {
        let mut state = self.client_state.write().await;

        info!(
            "Sending graceful shutdown (FCUnpublish/deleteStream) to {}",
            self.url
        );

        match state.session.stop_publishing() {
            Ok(results) => {
                for res in results {
                    Self::send_packet(&self.tx_feed, res);
                }
            }
            Err(e) => error!("Error generating stop_publishing packets: {}", e),
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
