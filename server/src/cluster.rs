
pub trait GameServer {
    fn start(&self);
    fn address(&self) -> String;
}

#[cfg(feature = "clustered")]
mod agones_server {
    use std::time::Duration;
    use tokio_compat_02::FutureExt;
    use crate::cluster::GameServer;

    pub struct AgonesCluster {
        sdk: agones::Sdk
    }

    impl AgonesCluster {
        pub fn new() -> Self {
            Self {
                sdk: agones::Sdk::new().unwrap()
            }
        }
    }

    impl GameServer for AgonesCluster {
        fn start(&self) {
            tracing::info!("Starting up the Agones SDK.");
            let mut inner_sdk = self.sdk.clone();
            tokio::spawn(
                async move {
                    loop {
                        match inner_sdk.health() {
                            (s, Ok(_)) => {
                                tracing::info!("Health ping sent.");
                                inner_sdk = s;
                            }
                            (s, Err(e)) => {
                                tracing::info!("Health ping failed to send.");
                                inner_sdk = s;
                            }
                        }
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
                    .compat(),
            );
            self.sdk.ready().unwrap();
        }

        fn address(&self) -> String {
            let gameserver = self.sdk
                .get_gameserver()
                .unwrap();
            let address = gameserver.status.as_ref().unwrap().address.clone();
            let port = gameserver.status.as_ref().unwrap().ports.get(1).as_ref().unwrap().port;
            let full_address = format!("{}:{}", address, port);
            tracing::info!("Creating public port: {}", full_address);
            full_address
        }
    }
}

struct Standalone {}

impl GameServer for Standalone {
    fn start(&self) {
    }

    fn address(&self) -> String {
        "127.0.0.1:42424".to_string()
    }
}

#[cfg(feature = "clustered")]
pub fn get_game_server() -> impl GameServer {
    agones_server::AgonesCluster::new()
}

#[cfg(not(feature = "clustered"))]
pub fn get_game_server() -> impl GameServer {
    Standalone {}
}