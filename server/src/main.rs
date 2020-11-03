mod runtime;

use async_channel::TryRecvError;
use common::{buffer::SimpleBufferPool, message::{MESSAGE_SETTINGS, Message}};
use futures::{pin_mut, FutureExt as FExt, StreamExt, TryStreamExt};
use futures_util::select;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Error, Method, Response, Server, StatusCode,
};
use runtime::NativeRuntime;
use tracing::Level;
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
    
    let data_port = "127.0.0.1:42424".parse().unwrap();
    let public_port = "127.0.0.1:42424".parse().unwrap();
    let session_port: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /*
    let data_port = "localhost:42424".parse().unwrap();
    let public_port = "localhost:42424".parse().unwrap();
    let session_port: SocketAddr = "192.168.100.135:8080".parse().unwrap();
    */
    
    let mut rtc_server = RtcServer::new(data_port, public_port).await.unwrap();
    tracing_subscriber::fmt()
        .with_max_level(Level::WARN)
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
    let mut runtime = common::message::MessageProcessor::new(NativeRuntime::new());

    let message_sender = runtime.message_sender().clone();
    let message_reciever = runtime.message_receiver().clone();
    tokio::spawn(async move {
        loop {
            let received = match message_reciever.recv().await {
                Ok(message) => {
                    tracing::info!("Received {:?}", message);
                    match message {
                        Message::Sync => None,
                        Message::Text(_) => Some(message),
                        Message::Unknown => None,
                    }
                }
                Err(_) => None,
            };
            if let Some(message) = received {
                tracing::info!("Sending {:?} back out", message);
                message_sender.try_send(message).unwrap();
            }
        }
    });

    let byte_receiver = runtime.outgoing_byte_reader();
    let byte_sender = runtime.incoming_byte_sender();
    tokio::spawn( async move {
        runtime.run().await;
    });
    loop {
        let pending_packet = {
            let recieve = rtc_server.recv().fuse();
            pin_mut!(recieve);
            
            let pending_send = byte_receiver.recv().fuse();
            pin_mut!(pending_send);

            let pending_packet = select! {
                received = recieve => {
                    match received {
                        Ok(received) => {
                            tracing::info!("Ingesting incoming message");
                            byte_sender.send(received.message.as_ref().into()).await.unwrap();
                        }
                        Err(err) => {
                            tracing::warn!("Failed to receive message. Error: {}", err);
                        }
                    };
                    None
                },
                pending_packet = pending_send => {
                    match pending_packet {
                        Ok(packet) => {
                            tracing::info!("Sending pending outgoing message");
                            Some(packet)
                        },
                        Err(_) => None,
                    }
                }
            };
            pending_packet
        };
        if let Some(packet) = pending_packet {
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
