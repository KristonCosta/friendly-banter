mod game;

use game::universe::Universe;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Error, Method, Response, Server, StatusCode,
};
use std::net::SocketAddr;
use tracing_subscriber::fmt::format::FmtSpan;
use webrtc_unreliable::Server as RtcServer;
use std::str;

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

    tokio::spawn(async move {
        Server::bind(&session_port).serve(server).await.unwrap();
    });

    let universe = Universe::new();
    let mut message_buf = Vec::new();
    loop {
        let received = match rtc_server.recv().await {
            Ok(received) => {
                tracing::info!("Received message from {}", received.remote_addr);
                message_buf.clear();
                message_buf.extend(received.message.as_ref());
                
                match unpack(&message_buf) {
                    Message::Sync => {
                        tracing::info!("Sync request came in")
                    }
                    Message::Unknown => {
                        tracing::info!("Unknown request came in")
                    }
                }

                Some((received.message_type, received.remote_addr))
            }
            Err(err) => {
                tracing::warn!("Failed to receive message. Error: {}", err);
                None
            }
        };

        if let Some((message_type, remote_addr)) = received {
            if let Err(err) = rtc_server
                .send(&message_buf, message_type, &remote_addr)
                .await
            {
                tracing::warn!("Failed to send message to {}. Error: {}", remote_addr, err);
            }
        }
    }

}
enum Message {
    Sync,
    Unknown,
}

fn unpack(message: &Vec<u8>) -> Message {
    match message.len() {
        1 => {
            match message[0] {
                1 => Message::Sync, 
                _ => Message::Unknown
            }
        }
        _ => Message::Unknown
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
                    tracing::info!("Successfully handled RTC request from {}", request.remote_address);
                    response.headers_mut().insert(
                        header::ACCESS_CONTROL_ALLOW_ORIGIN,
                        HeaderValue::from_static("*"),
                    );
                    Ok(response.map(Body::from))
                }
                Err(err) => {
                    tracing::error!("Failed to handle RTC session request. Error {}", err);
                    Self::response(StatusCode::BAD_REQUEST, format!("{}", err))
                },
            }
        }
    }
}
