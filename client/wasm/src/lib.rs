#![feature(async_closure)]
mod processor;
mod utils;

use futures::{future::MapErr, Future, FutureExt};
use js_sys::{Promise, Reflect};
use std::sync::mpsc::{self, Receiver, Sender};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{future_to_promise, spawn_local, JsFuture};
use web_sys::{
    MessageEvent, RtcDataChannel, RtcDataChannelInit, RtcDataChannelType, RtcIceCandidateInit,
    RtcPeerConnection, RtcPeerConnectionIceEvent, RtcSdpType, RtcSessionDescriptionInit,
    XmlHttpRequest, XmlHttpRequestResponseType,
};

#[wasm_bindgen]
pub fn init_panic_hook() {
    utils::set_hooks();
}

pub enum Event {
    String(String),
}

pub enum Message {
    Sync,
}

struct MessageBuffer(Vec<u8>);

impl MessageBuffer {
    pub fn with_capacity(n: usize) -> Self {
        Self(Vec::with_capacity(n))
    }

    fn load(&mut self, message: Message) {
        self.0.clear();
        match message {
            Message::Sync => self.0.push(1),
        }
    }
}

impl AsRef<[u8]> for MessageBuffer {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

enum DispatcherEvent {
    ChannelOpen,
}
#[wasm_bindgen]
#[derive(Clone)]

pub struct Eventer {
    incoming: async_channel::Receiver<Event>,
    outgoing: async_channel::Sender<Message>,
    dispatcher: Option<Dispatcher>,
}

impl Eventer {
    fn new(
        incoming: async_channel::Receiver<Event>,
        outgoing: async_channel::Sender<Message>,
        dispatcher: Dispatcher,
    ) -> Self {
        Self {
            incoming,
            outgoing,
            dispatcher: Some(dispatcher),
        }
    }

    pub fn incoming(&self) -> &async_channel::Receiver<Event> {
        &self.incoming
    }

    pub fn outgoing(&self) -> &async_channel::Sender<Message> {
        &self.outgoing
    }
}

#[wasm_bindgen]
impl Eventer {
    pub fn run(&mut self) -> Promise {
        let mut dispatcher = self.dispatcher.take().unwrap();
        future_to_promise(async move {
            dispatcher.run().await;
            Ok(JsValue::TRUE)
        })
    }
}

#[wasm_bindgen]
#[derive(Clone)]
struct Dispatcher {
    rx_message: async_channel::Receiver<Message>,
    tx_message: async_channel::Sender<Message>,
    rx_event: async_channel::Receiver<Event>,
    tx_event: async_channel::Sender<Event>,
    channel: RtcDataChannel,
    peer: RtcPeerConnection
}

#[wasm_bindgen]
impl Dispatcher {
    pub fn new() -> Self {
        let (tx_message, rx_message): (
            async_channel::Sender<Message>,
            async_channel::Receiver<Message>,
        ) = async_channel::unbounded();
        let (tx_event, rx_event): (async_channel::Sender<Event>, async_channel::Receiver<Event>) =
        async_channel::unbounded();
        let peer: RtcPeerConnection = RtcPeerConnection::new().unwrap();
        tracing::info!(
            "Created peer connection with state {:?}",
            peer.signaling_state()
        );
        let mut channel_dict = RtcDataChannelInit::new();
        channel_dict.max_retransmits(0).ordered(false);
        let channel: RtcDataChannel =
            peer.create_data_channel_with_data_channel_dict("webudp", &channel_dict);
        Self {
            peer, 
            channel,  
            tx_event, 
            rx_event, 
            tx_message, rx_message, 
        }
    }

    pub async fn run(self) {
        tracing::info!("Starting dispatcher");
        let result = async {
            tracing::info!("Dispatcher received channel open. Processing messages");
            let mut message_buffer = MessageBuffer::with_capacity(65536);
            loop {
                match self.rx_message.recv().await {
                    Ok(message) => {
                        message_buffer.load(message);
                        self.channel
                            .send_with_u8_array(message_buffer.as_ref())
                            .unwrap();
                    }
                    Err(_) => break,
                }
            }
        };
        result.await;
    }

    pub async fn start(self) -> Eventer {    


        let (tx_internal, rx_internal): (Sender<DispatcherEvent>, Receiver<DispatcherEvent>) =
            mpsc::channel();

        
        self.channel.set_binary_type(RtcDataChannelType::Arraybuffer);
        let tx_event = self.tx_event.clone();
        let on_message_callback = Closure::wrap(Box::new(
            move |event: MessageEvent| {
                let arr: js_sys::Uint8Array = js_sys::Uint8Array::new(&event.data());
                let rust_vec = arr.to_vec();
                let tx = tx_event.clone();
                spawn_local(async move {
                    tx.send(Event::String(format!("{:?}", rust_vec)))
                        .await
                        .unwrap();
                });
            }, 
        ) as Box<dyn FnMut(MessageEvent)>);

        self.channel.set_onmessage(Some(on_message_callback.as_ref().unchecked_ref()));
        on_message_callback.forget();

        let inner_channel = self.channel.clone();
        let on_channel_open = Closure::wrap(Box::new(move |_: MessageEvent| {
            tracing::info!("Channel opened");

            tx_internal.send(DispatcherEvent::ChannelOpen);
        }) as Box<dyn FnMut(MessageEvent)>);

        self.channel.set_onopen(Some(on_channel_open.as_ref().unchecked_ref()));
        on_channel_open.forget();

        let on_ice_candidate_callback = Closure::wrap(Box::new(
            move |ev: RtcPeerConnectionIceEvent| match ev.candidate() {
                Some(candidate) => {
                    tracing::info!("Peer on ice candidate: {:#?}", candidate.candidate());
                }
                None => {
                    tracing::info!("All local candidates received");
                }
            },
        )
            as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
        self.peer.set_onicecandidate(Some(on_ice_candidate_callback.as_ref().unchecked_ref()));
        on_ice_candidate_callback.forget();

        let offer = JsFuture::from(self.peer.create_offer()).await.unwrap();
        let offer_sdp;
        unsafe {
            offer_sdp = Reflect::get(&offer, &JsValue::from_str("sdp"))
                .unwrap()
                .as_string()
                .unwrap();
        }
        tracing::info!("Peer offer {:?}", offer_sdp);

        let mut offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        offer_obj.sdp(&offer_sdp);
        let sld_promise = self.peer.set_local_description(&offer_obj);
        JsFuture::from(sld_promise).await.unwrap();
        tracing::info!("Peer state: {:?}", self.peer.signaling_state());

        let request = XmlHttpRequest::new().unwrap();
        let request_clone = request.clone();
        request.set_response_type(XmlHttpRequestResponseType::Json);

        let address = "http://localhost:8080/session";

        request.open("POST", address).unwrap();
        let peer_clone = self.peer.clone();
        let on_request_response_callback: wasm_bindgen::prelude::Closure<dyn FnMut() -> ()> =
            Closure::new(move || match request_clone.status() {
                Ok(200) => {
                    tracing::info!("Successful session request!");
                    let response = request_clone.response().unwrap();
                    tracing::info!("{:?}", response);
                    let answer_sdp;
                    unsafe {
                        let answer_obj =
                            Reflect::get(&response, &JsValue::from_str("answer")).unwrap();
                        answer_sdp = Reflect::get(&answer_obj, &JsValue::from_str("sdp"))
                            .unwrap()
                            .as_string()
                            .unwrap();
                    }
                    tracing::info!("1");
                    let mut session_description_init =
                        RtcSessionDescriptionInit::new(RtcSdpType::Answer);
                    session_description_init.sdp(&answer_sdp);
                    tracing::info!("2");
                    let candidate_response: String;
                    let sdp_m_line_index: u16;
                    let sdp_mid: String;
                    unsafe {
                        let candidate_response_js =
                            Reflect::get(&response, &JsValue::from_str("candidate")).unwrap();
                        tracing::info!("CandidateJS: {:?}", candidate_response_js);
                        candidate_response =
                            Reflect::get(&candidate_response_js, &JsValue::from_str("candidate"))
                                .unwrap()
                                .as_string()
                                .unwrap();
                        sdp_m_line_index = Reflect::get(
                            &candidate_response_js,
                            &JsValue::from_str("sdpMLineIndex"),
                        )
                        .unwrap()
                        .as_f64()
                        .unwrap() as u16;

                        sdp_mid =
                            Reflect::get(&candidate_response_js, &JsValue::from_str("sdpMid"))
                                .unwrap()
                                .as_string()
                                .unwrap();
                    }

                    tracing::info!("Candidate: {:?}", candidate_response);
                    let mut candidate = RtcIceCandidateInit::new(&candidate_response);
                    candidate.sdp_m_line_index(Some(sdp_m_line_index));
                    candidate.sdp_mid(Some(&sdp_mid));
                    let another_peer = peer_clone.clone();
                    let ice_closure = Closure::new(move |_| {
                        tracing::info!("Set ice closure");
                    });
                    let another_peer = peer_clone.clone();
                    let description_closure = Closure::new(move |_| {
                        tracing::info!("Set the remote description");
                        another_peer
                            .add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&candidate))
                            .then(&ice_closure);
                    });

                    peer_clone
                        .set_remote_description(&session_description_init)
                        .then(&description_closure);
                    description_closure.forget();
                }
                _ => {
                    tracing::error!("Failed to request session");
                }
            });
        request.set_onload(Some(on_request_response_callback.as_ref().unchecked_ref()));
        on_request_response_callback.forget();

        request
            .send_with_opt_str(Some(&self.peer.local_description().unwrap().sdp()))
            .unwrap();
        let my_listener = rx_internal;
        
        async {
            loop {
                match my_listener.try_recv() {
                    Err(_) => break,
                    Ok(_) => break,
                }
            }
        }
        .await;

        Eventer::new(self.rx_event.clone(), self.tx_message.clone(), self)
    }
}
