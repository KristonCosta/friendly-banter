#![recursion_limit = "512"]
mod game;
mod runtime;
mod cluster;

use std::time::Duration;
use common::message::{RawMessage, SignedMessage, Target};
use futures::{pin_mut, FutureExt as FExt};
use futures_util::select;
use game::{ChannelBundle, Universe};
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Error, Method, Response, Server, StatusCode,
};
use runtime::NativeRuntime;
use std::{collections::{HashMap, HashSet}, net::SocketAddr, process};
use tokio_compat_02::FutureExt;
use tracing_subscriber::fmt::format::FmtSpan;
use webrtc_unreliable::{MessageType, Server as RtcServer};
use crate::cluster::{GameServer, get_game_server};

#[derive(Clone)]
struct Router {
    handler: handlers::Handler,
}

impl Router {
    pub fn new(handler: handlers::Handler) -> Self {
        Self { handler }
    }
    async fn route(
        self,
        remote_address: SocketAddr,
        request: hyper::Request<Body>,
    ) -> Result<Response<Body>, hyper::http::Error> {
        let handler_request = handlers::Request {
            remote_address,
            request,
        };

        match (
            handler_request.request.method(),
            handler_request.request.uri().path(),
        ) {
            (&Method::POST, "/session") => self.handler.post_session(handler_request).await,
            (method, path) => {
                tracing::info!("Unexpected request. Method: {}, Path: {}", method, path);
                let mut response = Response::default();
                *response.status_mut() = StatusCode::NOT_FOUND;
                Ok(response)
            }
        }
    }
}

pub enum InternalMessage {
    ClientSnapshot(HashSet<usize>),
}

pub struct ClientLookup {
    client_lookup: HashMap<SocketAddr, usize>,
    reverse_lookup: HashMap<usize, SocketAddr>,
}

impl ClientLookup {
    pub fn new() -> Self {
        Self {
            client_lookup: HashMap::new(),
            reverse_lookup: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: usize, socket: SocketAddr) {
        self.client_lookup.insert(socket, id);
        self.reverse_lookup.insert(id, socket);
    }

    pub fn lookup(&self, id: usize) -> Option<&SocketAddr> {
        self.reverse_lookup.get(&id)
    }

    pub fn lookup_socket(&self, socket: &SocketAddr) -> Option<&usize> {
        self.client_lookup.get(socket)
    }

    pub fn remove(&mut self, socket: &SocketAddr) {
        if let Some(id) = self.client_lookup.get(socket) {
            self.reverse_lookup.remove(id);
        }
        self.client_lookup.remove(socket);
    }

    pub fn contains(&self, socket: &SocketAddr) -> bool {
        self.client_lookup.contains_key(socket)
    }

    pub fn clients(&self) -> Vec<&SocketAddr> {
        self.client_lookup.keys().collect()
    }

    pub fn ids(&self) -> Vec<&usize> {
        self.reverse_lookup.keys().collect()
    }
}

#[tokio::main]
async fn main() {
    ctrlc::set_handler(move || {
        process::exit(1)
    }).expect("Error setting Ctrl-C handler");

    tracing_subscriber::fmt()
        //   .with_max_level(Level::WARN)
        .with_span_events(FmtSpan::CLOSE)
        .init();
    tracing::info!("Starting up the game server.");

    let gameserver = get_game_server();
    gameserver.start();
    let data_port = "0.0.0.0:42424".parse().unwrap();
    let public_port = gameserver.address().parse().unwrap();
    let session_port: SocketAddr = "[::]:8081".parse().unwrap();

    let mut rtc_server = RtcServer::new(data_port, public_port).await.unwrap();

    let session_endpoint = rtc_server.session_endpoint();

    let server = make_service_fn(move |addr_stream: &AddrStream| {
        let handler = handlers::Handler::new(session_endpoint.clone());
        let router = Router::new(handler);
        let remote_addr = addr_stream.remote_addr();
        async move {
            Ok::<_, Error>(service_fn(move |request| {
                let router = router.clone();
                router.route(remote_addr, request)
            }))
        }
    });

    tokio::spawn(
        async move {
            Server::bind(&session_port).serve(server).await.unwrap();
        }
        .compat(),
    );
    let (signed_packet_sender, signed_packet_receiver) = async_channel::unbounded();
    let mut multiplexer =
        common::multiplexer::ConnectionMultiplexer::new(NativeRuntime::new(), signed_packet_sender);

    let reliable_channel = common::processor::BidirectionalChannel::new();
    let message_channel = common::processor::BidirectionalChannel::new();
    let (internal_sender, internal_receiver) = async_channel::unbounded();
    let bundle = ChannelBundle {
        reliable_sender: reliable_channel.outgoing_sender,
        reliable_receiver: reliable_channel.incoming_reciever,
        message_sender: message_channel.outgoing_sender,
        message_receiver: message_channel.incoming_reciever,
        internal_receiver,
    };
    let mut universe = Universe::new(bundle);

    tokio::spawn(async move {
        universe.run().await;
    });

    let incoming_message = multiplexer.message_receiver();
    let incoming_reliable_message = multiplexer.reliable_message_receiver();

    let mut client_lookup = ClientLookup::new();
    tracing::info!("Game server started.");
    loop {
        let pending_packet = {
            let recieve = rtc_server.recv().fuse();
            pin_mut!(recieve);

            let pending_send = signed_packet_receiver.recv().fuse();
            pin_mut!(pending_send);

            let incoming_message = incoming_message.recv().fuse();
            pin_mut!(incoming_message);

            let incoming_reliable_message = incoming_reliable_message.recv().fuse();
            pin_mut!(incoming_reliable_message);

            let outgoing_reliable_message = reliable_channel.outgoing_receiver.recv().fuse();
            pin_mut!(outgoing_reliable_message);

            let outgoing_message = message_channel.outgoing_receiver.recv().fuse();
            pin_mut!(outgoing_message);
            let pending_packet = select! {
                received = recieve => {

                    if let Ok(received) = received {
                        if !client_lookup.contains(&received.remote_addr) {
                            tracing::info!("Received new client {:?}", &received.remote_addr);
                            let id = multiplexer.register();
                            client_lookup.register(id, received.remote_addr);
                            internal_sender.send(InternalMessage::ClientSnapshot(client_lookup.ids().into_iter().map(|x| *x).collect())).await;
                        }
                        tracing::info!("Received message from client {:?}", &received.remote_addr);
                        multiplexer.send_raw(Target::Client(*client_lookup.lookup_socket(&received.remote_addr).unwrap()), received.message.as_ref().into()).await;
                    }
                    Action::None
                },
                pending_packet = pending_send => {
                    if let Ok(packet) = pending_packet {
                        Action::Send(packet)
                    } else {
                        Action::None
                    }
                },
                message = incoming_message => {
                    if let Ok(message) = message {
                        message_channel.incoming_sender.send(message).await.unwrap();
                    }
                    Action::None
                },
                message = incoming_reliable_message => {
                    if let Ok(message) = message {
                        reliable_channel.incoming_sender.send(message).await.unwrap();
                    }
                    Action::None
                },
                message = outgoing_reliable_message => {
                    if let Ok(message) = message {
                        multiplexer.send_reliable_message(Target::Client(message.id), message.message).await;
                    }
                    Action::None
                }
                message = outgoing_message => {
                    if let Ok(message) = message {
                        multiplexer.send_message(Target::Client(message.id), message.message).await;
                    }
                    Action::None
                }
            };
            pending_packet
        };

        if let Action::Send(packet) = pending_packet {
            let clients: Vec<SocketAddr> =
                rtc_server.connected_clients().map(|x| x.clone()).collect();


            if let Some(client) = client_lookup.lookup(packet.id) {
                tracing::info!("Sending message {:?}", client);
                if rtc_server.is_connected(client) {
                    rtc_server
                        .send(packet.message.as_ref(), MessageType::Binary, client)
                        .await
                        .unwrap();
                }
            }

            let mut disconnected = vec![];

            client_lookup
                .clients()
                .iter()
                .filter(|x| !clients.contains(*x))
                .for_each(|client| {
                    if let Some(target) = client_lookup.lookup_socket(&client) {
                        multiplexer.kill(*target);
                        disconnected.push(*client.clone());
                    }
                });

            disconnected.into_iter().for_each(|client| {
                client_lookup.remove(&client);
            });
        };
    }
}

pub enum Action {
    None,
    Send(SignedMessage<RawMessage>),
    Incoming(),
}

mod handlers {
    use std::net::SocketAddr;

    use hyper::{
        header::{self, HeaderValue},
        Body, Response, StatusCode,
    };
    use webrtc_unreliable::SessionEndpoint;

    #[derive(Clone)]
    pub struct Handler {
        session_endpoint: SessionEndpoint,
    }

    pub struct Request {
        pub(crate) request: hyper::Request<Body>,
        pub(crate) remote_address: SocketAddr,
    }

    impl Handler {
        pub fn new(session_endpoint: SessionEndpoint) -> Self {
            Self { session_endpoint }
        }

        fn response<S: Into<String>>(
            status: StatusCode,
            message: S,
        ) -> Result<Response<Body>, hyper::http::Error> {
            Response::builder()
                .status(status)
                .body(Body::from(message.into()))
        }

        pub async fn post_session(
            mut self,
            request: Request,
        ) -> Result<Response<Body>, hyper::http::Error> {
            tracing::info!(
                "Received RTC session request from {}",
                request.remote_address
            );
            match self
                .session_endpoint
                .http_session_request(request.request.into_body())
                .await
            {
                Ok(mut response) => {
                    tracing::info!(
                        "Successfully handled RTC request from {}",
                        request.remote_address
                    );
                    response.headers_mut().insert(
                        header::ACCESS_CONTROL_ALLOW_ORIGIN,
                        HeaderValue::from_static("*"),
                    );
                    Ok(response.map(Body::from))
                }
                Err(err) => {
                    tracing::error!("Failed to handle RTC session request. Error {}", err);
                    Self::response(StatusCode::BAD_REQUEST, format!("{}", err))
                }
            }
        }
    }
}
