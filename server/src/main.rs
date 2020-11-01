mod runtime;

use async_channel::TryRecvError;
use common::Message;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Error, Method, Response, Server, StatusCode,
};

use futures::{StreamExt, TryStreamExt};
use std::net::SocketAddr;
use std::str;
use tokio_compat_02::FutureExt;
use tracing_subscriber::fmt::format::FmtSpan;
use turbulence::{BufferPacket, MessageChannelsBuilder, Packet, PacketMultiplexer, PacketPool};
use webrtc_unreliable::{MessageType, Server as RtcServer};

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
#[tokio::main]
async fn main() {
    /*
    let data_port = "127.0.0.1:42424".parse().unwrap();
    let public_port = "127.0.0.1:42424".parse().unwrap();
    let session_port: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    */
    let data_port = "192.168.100.135:42424".parse().unwrap();
    let public_port = "192.168.100.135:42424".parse().unwrap();
    let session_port: SocketAddr = "192.168.100.135:8080".parse().unwrap();
    let mut rtc_server = RtcServer::new(data_port, public_port).await.unwrap();
    tracing_subscriber::fmt()
        .with_span_events(FmtSpan::CLOSE)
        .init();
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

    let pool = turbulence::BufferPacketPool::new(common::SimpleBufferPool(32));
    let runtime = runtime::Runtime::new();
    let mut multiplexer = PacketMultiplexer::new();
    let mut builder = MessageChannelsBuilder::new(runtime, pool);
    builder
        .register::<Message>(common::MESSAGE_SETTINGS)
        .unwrap();

    let mut channels = builder.build(&mut multiplexer);

    let (tx_outgoing, rx_outgoing): (
        async_channel::Sender<BufferPacket<Box<[u8]>>>,
        async_channel::Receiver<BufferPacket<Box<[u8]>>>,
    ) = async_channel::unbounded();
    let (mut incoming, mut outgoing) = multiplexer.start();
    tokio::spawn(async move {
        loop {
            let received = match channels.try_async_recv::<Message>().await {
                Ok(message) => {
                    tracing::info!("Received {:?}", message);
                    Some(message)
                }
                Err(_) => None,
            };
            if let Some(message) = received {
                tracing::info!("Sending {:?} back out", message);
                channels.try_send(message).unwrap();
            }
        }
    });
    tokio::spawn(async move {
        loop {
            match outgoing.next().await {
                Some(buf) => {
                    tracing::info!("Queuing pending outgoing message");
                    tx_outgoing.send(buf).await.unwrap()
                }
                _ => break,
            }
        }
    });

    loop {
        match rtc_server.recv().await {
            Ok(received) => {
                tracing::info!("Ingesting incoming message");
                let mut packet = pool.acquire();
                packet.extend(received.message.as_ref());
                incoming.try_send(packet).unwrap();
            }
            Err(err) => {
                tracing::warn!("Failed to receive message. Error: {}", err);
            }
        };

        let pending_outgoing = match rx_outgoing.try_recv() {
            Ok(packet) => {
                tracing::info!("Sending pending outgoing message");
                Some(packet)
            }
            Err(TryRecvError::Closed) => break,
            Err(_) => None,
        };

        if let Some(packet) = pending_outgoing {
            let clients: Vec<SocketAddr> =
                rtc_server.connected_clients().map(|x| x.clone()).collect();
            for client in clients {
                rtc_server
                    .send(packet.as_ref(), MessageType::Binary, &client)
                    .await
                    .unwrap();
            }
        };
    }
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
