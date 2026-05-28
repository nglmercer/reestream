#![allow(dead_code)]

use bytes::Bytes;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{
    ClientSession, ClientSessionConfig, ClientSessionResult, PublishRequestType, ServerSession,
    ServerSessionConfig, ServerSessionResult,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub struct MockRtmpServer {
    pub addr: std::net::SocketAddr,
    listener: TcpListener,
}

impl MockRtmpServer {
    pub async fn bind() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        Self { addr, listener }
    }

    pub async fn accept(&self) -> MockServerSession {
        let (stream, _) = self.listener.accept().await.unwrap();
        MockServerSession { stream }
    }

    pub async fn accept_with_timeout(&self, duration: Duration) -> Option<MockServerSession> {
        match tokio::time::timeout(duration, self.listener.accept()).await {
            Ok(Ok((stream, _))) => Some(MockServerSession { stream }),
            _ => None,
        }
    }
}

pub struct MockServerSession {
    stream: TcpStream,
}

impl MockServerSession {
    pub async fn perform_handshake(
        &mut self,
    ) -> Result<(ServerSession, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
        let mut hs = Handshake::new(PeerType::Server);
        let mut buf = [0u8; 4096];

        loop {
            let n = self.stream.read(&mut buf).await?;
            if n == 0 {
                return Err("EOF during mock handshake".into());
            }

            match hs.process_bytes(&buf[..n])? {
                HandshakeProcessResult::InProgress { response_bytes } => {
                    if !response_bytes.is_empty() {
                        self.stream.write_all(&response_bytes).await?;
                    }
                }
                HandshakeProcessResult::Completed {
                    response_bytes,
                    remaining_bytes,
                } => {
                    if !response_bytes.is_empty() {
                        self.stream.write_all(&response_bytes).await?;
                    }

                    let mut config = ServerSessionConfig::new();
                    config.chunk_size = 128;
                    config.window_ack_size = 262_144;
                    let (session, initial_results) = ServerSession::new(config)?;
                    for res in initial_results {
                        if let ServerSessionResult::OutboundResponse(packet) = res {
                            self.stream.write_all(&packet.bytes).await?;
                        }
                    }
                    return Ok((session, remaining_bytes));
                }
            }
        }
    }

    pub async fn read_packet(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let mut buf = [0u8; 8192];
        let n = self.stream.read(&mut buf).await?;
        Ok(buf[..n].to_vec())
    }

    pub async fn write_all(&mut self, data: &[u8]) -> Result<(), std::io::Error> {
        self.stream.write_all(data).await
    }
}

pub struct MockRtmpClient {
    stream: TcpStream,
    session: Option<ClientSession>,
}

impl MockRtmpClient {
    pub async fn connect(addr: std::net::SocketAddr) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            session: None,
        })
    }

    pub async fn perform_handshake(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut hs = Handshake::new(PeerType::Client);
        let c0_c1 = hs.generate_outbound_p0_and_p1()?;
        self.stream.write_all(&c0_c1).await?;

        let mut buf = [0u8; 4096];
        loop {
            let n = self.stream.read(&mut buf).await?;
            if n == 0 {
                return Err("EOF during client handshake".into());
            }

            match hs.process_bytes(&buf[..n])? {
                HandshakeProcessResult::InProgress { response_bytes } => {
                    if !response_bytes.is_empty() {
                        self.stream.write_all(&response_bytes).await?;
                    }
                }
                HandshakeProcessResult::Completed { response_bytes, .. } => {
                    if !response_bytes.is_empty() {
                        self.stream.write_all(&response_bytes).await?;
                    }
                    break;
                }
            }
        }

        let mut config = ClientSessionConfig::new();
        config.tc_url = Some("rtmp://127.0.0.1/app".to_string());
        let (session, initial_results) = ClientSession::new(config)?;

        for res in initial_results {
            if let ClientSessionResult::OutboundResponse(packet) = res {
                self.stream.write_all(&packet.bytes).await?;
            }
        }

        self.session = Some(session);
        Ok(())
    }

    pub async fn request_connection(
        &mut self,
        app: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(session) = &mut self.session {
            let result = session.request_connection(app.to_string())?;
            if let ClientSessionResult::OutboundResponse(packet) = result {
                self.stream.write_all(&packet.bytes).await?;
            }
        }
        Ok(())
    }

    pub async fn request_publish(
        &mut self,
        stream_key: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(session) = &mut self.session {
            let result = session.request_publishing(stream_key.to_string(), PublishRequestType::Live)?;
            if let ClientSessionResult::OutboundResponse(packet) = result {
                self.stream.write_all(&packet.bytes).await?;
            }
        }
        Ok(())
    }

    pub async fn send_video_data(&mut self, data: Bytes) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(session) = &mut self.session {
            let result = session.publish_video_data(data, rml_rtmp::time::RtmpTimestamp::new(0), true)?;
            if let ClientSessionResult::OutboundResponse(packet) = result {
                self.stream.write_all(&packet.bytes).await?;
            }
        }
        Ok(())
    }

    pub async fn send_audio_data(&mut self, data: Bytes) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(session) = &mut self.session {
            let result = session.publish_audio_data(data, rml_rtmp::time::RtmpTimestamp::new(0), true)?;
            if let ClientSessionResult::OutboundResponse(packet) = result {
                self.stream.write_all(&packet.bytes).await?;
            }
        }
        Ok(())
    }

    pub async fn read_response(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let mut buf = [0u8; 8192];
        let n = self.stream.read(&mut buf).await?;
        Ok(buf[..n].to_vec())
    }

    pub async fn disconnect(self) {
        drop(self.stream);
    }
}
