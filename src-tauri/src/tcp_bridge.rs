pub async fn tcp_bridge(
    port: u16,
    proxy_target: String,
    graceful: kulfi_utils::Graceful,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    startup_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
) {
    use eyre::WrapErr;

    let listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .wrap_err_with(|| {
            format!("Can not listen on port {port}, is it busy or you do not have permission?")
        }) {
        Ok(listener) => listener,
        Err(e) => {
            let error_msg = format!("Failed to bind TCP to port {port}: {e}");
            eprintln!("{error_msg}");
            let _ = startup_tx.send(Err(error_msg));
            return;
        }
    };

    println!("TCP bridge listening on 127.0.0.1:{port}");
    let _ = startup_tx.send(Ok(()));

    let peer_connections = kulfi_utils::PeerStreamSenders::default();

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                tracing::info!("Stopping TCP bridge.");
                break;
            }
            val = listener.accept() => {
                match val {
                    Ok((stream, _addr)) => {
                        tracing::info!("got TCP connection");
                        let self_endpoint = kulfi_utils::global_iroh_endpoint().await;
                        let graceful_for_conn = graceful.clone();
                        let peer_connections = peer_connections.clone();
                        let proxy_target = proxy_target.clone();
                        graceful.spawn(async move {
                            println!("forwarding tcp connection to {proxy_target}");
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
        }
    }
}
