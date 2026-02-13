use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn udp_bridge(
    port: u16,
    proxy_target: String,
    graceful: kulfi_utils::Graceful,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    startup_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
) {
    use eyre::WrapErr;

    let socket = match tokio::net::UdpSocket::bind(format!("127.0.0.1:{port}"))
        .await
        .wrap_err_with(|| {
            format!("Can not listen on UDP port {port}, is it busy or you do not have permission?")
        }) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            let error_msg = format!("Failed to bind UDP to port {port}: {e}");
            eprintln!("{error_msg}");
            let _ = startup_tx.send(Err(error_msg));
            return;
        }
    };

    let local_addr = socket.local_addr().unwrap();
    println!("UDP bridge listening on {local_addr}");
    let _ = startup_tx.send(Ok(()));

    let peer_connections = kulfi_utils::PeerStreamSenders::default();
    let sessions: Arc<Mutex<HashMap<SocketAddr, tokio::sync::mpsc::Sender<Vec<u8>>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let mut buf = vec![0u8; 65535];
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                tracing::info!("Stopping UDP bridge.");
                break;
            }
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((n, client_addr)) => {
                        let data = buf[..n].to_vec();
                        let mut sessions_guard = sessions.lock().await;

                        if let Some(sender) = sessions_guard.get(&client_addr) {
                            if sender.send(data.clone()).await.is_err() {
                                sessions_guard.remove(&client_addr);
                                drop(sessions_guard);
                                start_session(
                                    socket.clone(),
                                    client_addr,
                                    data,
                                    proxy_target.clone(),
                                    peer_connections.clone(),
                                    sessions.clone(),
                                    graceful.clone(),
                                ).await;
                            }
                        } else {
                            drop(sessions_guard);
                            start_session(
                                socket.clone(),
                                client_addr,
                                data,
                                proxy_target.clone(),
                                peer_connections.clone(),
                                sessions.clone(),
                                graceful.clone(),
                            ).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!("failed to recv UDP: {e:?}");
                    }
                }
            }
        }
    }
}

async fn start_session(
    socket: Arc<tokio::net::UdpSocket>,
    client_addr: SocketAddr,
    initial_data: Vec<u8>,
    remote_node_id52: String,
    peer_connections: kulfi_utils::PeerStreamSenders,
    sessions: Arc<Mutex<HashMap<SocketAddr, tokio::sync::mpsc::Sender<Vec<u8>>>>>,
    graceful: kulfi_utils::Graceful,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    {
        let mut sessions_guard = sessions.lock().await;
        sessions_guard.insert(client_addr, tx);
    }

    let graceful_for_session = graceful.clone();
    graceful.spawn(async move {
        println!("forwarding UDP datagrams to {remote_node_id52}");

        let self_endpoint = kulfi_utils::global_iroh_endpoint().await;
        let header = kulfi_utils::ProtocolHeader::from(kulfi_utils::Protocol::Udp);

        let result = async {
            let (mut send, mut recv) = kulfi_utils::get_stream(
                self_endpoint,
                header,
                remote_node_id52.to_string(),
                peer_connections,
                graceful_for_session.clone(),
            )
            .await?;

            // Send the initial datagram
            kulfi_utils::write_framed_datagram(&mut send, &initial_data).await?;

            let socket_for_recv = socket.clone();

            // iroh -> local UDP (responses from remote)
            let recv_task = tokio::spawn(async move {
                loop {
                    match kulfi_utils::read_framed_datagram(&mut recv).await {
                        Ok(data) => {
                            if let Err(e) = socket_for_recv.send_to(&data, client_addr).await {
                                tracing::error!("failed to send UDP response: {e:?}");
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::trace!("iroh recv stream ended: {e:?}");
                            break;
                        }
                    }
                }
            });

            // local UDP -> iroh (subsequent datagrams from client via channel)
            while let Some(data) = rx.recv().await {
                kulfi_utils::write_framed_datagram(&mut send, &data).await?;
            }

            send.finish()?;
            let _ = recv_task.await;

            Ok::<(), eyre::Report>(())
        }
        .await;

        if let Err(e) = result {
            tracing::error!("UDP session error: {e:?}");
        }

        // Cleanup session
        let mut sessions_guard = sessions.lock().await;
        sessions_guard.remove(&client_addr);
    });
}
