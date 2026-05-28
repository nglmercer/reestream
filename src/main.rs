use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use tracing_subscriber::filter::LevelFilter;

use reestream::client::handle_publisher;
use reestream::config::Config;

#[derive(clap::Parser)]
struct Args {
    /// Define config.toml path
    #[clap(long, short, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_line_number(true)
        .with_max_level(LevelFilter::DEBUG)
        .init();

    let args = Args::parse();
    let config = Config::from_file(args.config)?;
    let Config {
        rtmp_addr,
        rtmp_port,
        stream_key,
        platform,
        ..
    } = &config;

    println!("Configuration loaded:");
    println!("  Listener: {rtmp_addr}:{rtmp_port}");
    println!("  Stream key: {stream_key}");
    println!(
        "  Configured platforms: {}",
        platform.clone().unwrap_or_default().len()
    );

    let addr: SocketAddr = format!("{rtmp_addr}:{rtmp_port}").parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!("RTMP relay listening on {}", addr);

    let platforms = Arc::new(RwLock::new(platform.clone().unwrap_or_default()));

    loop {
        tokio::select! {
            biased;

            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C signal, shutting down...");
                break;
            }

            accept = listener.accept() => {
                match accept {
                    Ok((socket, peer_addr)) => {
                        if let Err(e) = socket.set_nodelay(true) {
                            warn!("Failed to set_nodelay on incoming socket: {}", e);
                        }

                        info!("New incoming connection from {}", peer_addr);
                        let platforms = platforms.clone();
                        let stream_key = stream_key.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_publisher(socket, platforms, stream_key).await {
                                error!("Error in connection from {}: {:#}", peer_addr, e);
                            } else {
                                info!("Connection from {} ended correctly", peer_addr);
                            }
                        });
                    }
                    Err(e) => {
                        warn!("Error accepting connection: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reestream::AsyncReadWrite;

    fn parse_socket_addr(addr: &str, port: u16) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        let addr: SocketAddr = format!("{addr}:{port}").parse()?;
        Ok(addr)
    }

    #[test]
    fn test_parse_socket_addr_valid() {
        let addr = parse_socket_addr("0.0.0.0", 1935).unwrap();
        assert_eq!(addr, "0.0.0.0:1935".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_parse_socket_addr_localhost() {
        let addr = parse_socket_addr("127.0.0.1", 8080).unwrap();
        assert_eq!(addr, "127.0.0.1:8080".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_parse_socket_addr_invalid() {
        let result = parse_socket_addr("not-an-address", 1935);
        assert!(result.is_err());
    }

    #[test]
    fn test_args_default_config() {
        let args = Args::try_parse_from(["reestream"]).unwrap();
        assert_eq!(args.config, PathBuf::from("config.toml"));
    }

    #[test]
    fn test_args_custom_config_short() {
        let args = Args::try_parse_from(["reestream", "-c", "/tmp/myconfig.toml"]).unwrap();
        assert_eq!(args.config, PathBuf::from("/tmp/myconfig.toml"));
    }

    #[test]
    fn test_args_custom_config_long() {
        let args =
            Args::try_parse_from(["reestream", "--config", "/etc/reestream/config.toml"]).unwrap();
        assert_eq!(args.config, PathBuf::from("/etc/reestream/config.toml"));
    }

    #[test]
    fn test_async_read_write_trait_bounds() {
        fn _assert_impl<T: AsyncReadWrite>() {}
        _assert_impl::<tokio::net::TcpStream>();
    }
}
