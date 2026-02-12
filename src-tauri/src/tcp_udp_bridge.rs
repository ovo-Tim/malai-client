pub async fn tcp_udp_bridge(
    port: u16,
    proxy_target: String,
    graceful: kulfi_utils::Graceful,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    use eyre::WrapErr;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // Bind TCP and UDP on the same port (different protocols, so no conflict)
    let tcp_listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .wrap_err_with(|| format!("can not listen TCP on port {port}"))
    {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind TCP to port {port}: {e:?}");
            return;
        }
    };

    let udp_socket = match tokio::net::UdpSocket::bind(format!("127.0.0.1:{port}"))
        .await
        .wrap_err_with(|| format!("can not listen UDP on port {port}"))
    {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("Failed to bind UDP to port {port}: {e:?}");
            return;
        }
    };

    println!("TCP+UDP bridge listening on 127.0.0.1:{port}");

    let tcp_peer_connections = kulfi_utils::PeerStreamSenders::default();
    let udp_peer_connections = kulfi_utils::PeerStreamSenders::default();
    let udp_sessions: Arc<Mutex<HashMap<SocketAddr, tokio::sync::mpsc::Sender<Vec<u8>>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let mut udp_buf = vec![0u8; 65535];

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                tracing::info!("Stopping TCP+UDP bridge.");
                break;
            }
            // TCP accept
            val = tcp_listener.accept() => {
                match val {
                    Ok((stream, _addr)) => {
                        tracing::info!("got TCP connection");
                        let self_endpoint = kulfi_utils::global_iroh_endpoint().await;
                        let graceful_for_conn = graceful.clone();
                        let peer_connections = tcp_peer_connections.clone();
                        let proxy_target = proxy_target.clone();
                        graceful.spawn(async move {
                            if let Err(e) = kulfi_utils::tcp_to_peer(
                                kulfi_utils::Protocol::Tcp.into(),
                                self_endpoint,
                                stream,
                                &proxy_target,
                                peer_connections,
                                graceful_for_conn,
                            )
                            .await
                            {
                                tracing::error!("failed to proxy tcp: {e:?}");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("failed to accept TCP: {e:?}");
                    }
                }
            }
            // UDP recv
            result = udp_socket.recv_from(&mut udp_buf) => {
                match result {
                    Ok((n, client_addr)) => {
                        let data = udp_buf[..n].to_vec();
                        let mut sessions_guard = udp_sessions.lock().await;

                        if let Some(sender) = sessions_guard.get(&client_addr) {
                            if sender.send(data.clone()).await.is_err() {
                                sessions_guard.remove(&client_addr);
                                drop(sessions_guard);
                                start_udp_session(
                                    udp_socket.clone(),
                                    client_addr,
                                    data,
                                    proxy_target.clone(),
                                    udp_peer_connections.clone(),
                                    udp_sessions.clone(),
                                    graceful.clone(),
                                ).await;
                            }
                        } else {
                            drop(sessions_guard);
                            start_udp_session(
                                udp_socket.clone(),
                                client_addr,
                                data,
                                proxy_target.clone(),
                                udp_peer_connections.clone(),
                                udp_sessions.clone(),
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

async fn start_udp_session(
    socket: std::sync::Arc<tokio::net::UdpSocket>,
    client_addr: std::net::SocketAddr,
    initial_data: Vec<u8>,
    remote_node_id52: String,
    peer_connections: kulfi_utils::PeerStreamSenders,
    sessions: std::sync::Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<
                std::net::SocketAddr,
                tokio::sync::mpsc::Sender<Vec<u8>>,
            >,
        >,
    >,
    graceful: kulfi_utils::Graceful,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    {
        let mut sessions_guard = sessions.lock().await;
        sessions_guard.insert(client_addr, tx);
    }

    let graceful_for_session = graceful.clone();
    graceful.spawn(async move {
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

            kulfi_utils::write_framed_datagram(&mut send, &initial_data).await?;

            let socket_for_recv = socket.clone();
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

        let mut sessions_guard = sessions.lock().await;
        sessions_guard.remove(&client_addr);
    });
}
