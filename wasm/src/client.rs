use common::message::SignedMessage;
use futures::join;
use js_sys::Reflect;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    MessageEvent, RtcDataChannel, RtcDataChannelInit, RtcDataChannelType, RtcIceCandidateInit,
    RtcPeerConnection, RtcPeerConnectionIceEvent, RtcSdpType, RtcSessionDescriptionInit,
    XmlHttpRequest, XmlHttpRequestResponseType,
};

enum DispatcherEvent {
    ChannelOpen,
}

#[wasm_bindgen]
pub struct WebRTCClient {
    processor_rx_outgoing: async_channel::Receiver<SignedMessage<Box<[u8]>>>,
    processor_tx_incoming: async_channel::Sender<Box<[u8]>>,

    rtc_rx_incoming: async_channel::Receiver<Box<[u8]>>,
    rtc_tx_incoming: async_channel::Sender<Box<[u8]>>,

    address: String,
    channel: RtcDataChannel,
    peer: RtcPeerConnection,
}

impl WebRTCClient {
    pub fn new(
        address: String,
        processor_reader: async_channel::Receiver<SignedMessage<Box<[u8]>>>,
        processor_sender: async_channel::Sender<Box<[u8]>>,
    ) -> Self {
        let peer: RtcPeerConnection = RtcPeerConnection::new().unwrap();
        tracing::info!(
            "Created peer connection with state {:?}",
            peer.signaling_state()
        );
        let mut channel_dict = RtcDataChannelInit::new();
        channel_dict.max_retransmits(0).ordered(false);
        let channel: RtcDataChannel =
            peer.create_data_channel_with_data_channel_dict("webudp", &channel_dict);

        let (rtc_tx_incoming, rtc_rx_incoming) = async_channel::unbounded();
        Self {
            address,
            peer,
            channel,
            processor_rx_outgoing: processor_reader,
            processor_tx_incoming: processor_sender,
            rtc_tx_incoming,
            rtc_rx_incoming,
        }
    }
}

#[wasm_bindgen]
impl WebRTCClient {
    pub async fn run(self) {
        self.connect().await;
        let incoming_rx = self.rtc_rx_incoming.clone();
        let processor_incoming_tx = self.processor_tx_incoming.clone();

        let incoming = async move {
            tracing::info!("Incoming message processor started");
            loop {
                match incoming_rx.recv().await {
                    Ok(message) => {
                        processor_incoming_tx.send(message).await.unwrap();
                    }
                    Err(_) => break,
                }
            }
        };
        let outgoing = async move {
            tracing::info!("Outgoing message processor started");
            loop {
                match self.processor_rx_outgoing.recv().await {
                    Ok(message) => {
                        self.channel
                            .send_with_u8_array(message.message.as_ref())
                            .unwrap();
                    }
                    _ => break,
                }
            }
        };
        join!(incoming, outgoing);
    }

    async fn connect(&self) {
        let (tx_internal, rx_internal): (
            async_channel::Sender<DispatcherEvent>,
            async_channel::Receiver<DispatcherEvent>,
        ) = async_channel::unbounded();

        self.channel
            .set_binary_type(RtcDataChannelType::Arraybuffer);

        let rtc_on_message_tx = self.rtc_tx_incoming.clone();
        let on_message_callback = Closure::wrap(Box::new(move |event: MessageEvent| {
            let arr: js_sys::Uint8Array = js_sys::Uint8Array::new(&event.data());
            let rust_vec = arr.to_vec();
            let tx = rtc_on_message_tx.clone();
            spawn_local(async move {
                tx.send(rust_vec.into_boxed_slice()).await.unwrap();
            });
        }) as Box<dyn FnMut(MessageEvent)>);

        self.channel
            .set_onmessage(Some(on_message_callback.as_ref().unchecked_ref()));
        on_message_callback.forget();

        let on_channel_open = Closure::wrap(Box::new(move |_: MessageEvent| {
            tracing::info!("Channel opened");
            let tx = tx_internal.clone();
            spawn_local(async move {
                tx.send(DispatcherEvent::ChannelOpen).await.unwrap();
            });
        }) as Box<dyn FnMut(MessageEvent)>);

        self.channel
            .set_onopen(Some(on_channel_open.as_ref().unchecked_ref()));
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
        self.peer
            .set_onicecandidate(Some(on_ice_candidate_callback.as_ref().unchecked_ref()));
        on_ice_candidate_callback.forget();

        let offer = JsFuture::from(self.peer.create_offer()).await.unwrap();
        let offer_sdp;

        offer_sdp = Reflect::get(&offer, &JsValue::from_str("sdp"))
            .unwrap()
            .as_string()
            .unwrap();

        tracing::info!("Peer offer {:?}", offer_sdp);

        let mut offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        offer_obj.sdp(&offer_sdp);
        let sld_promise = self.peer.set_local_description(&offer_obj);
        JsFuture::from(sld_promise).await.unwrap();
        tracing::info!("Peer state: {:?}", self.peer.signaling_state());

        let request = XmlHttpRequest::new().unwrap();
        let request_clone = request.clone();
        request.set_response_type(XmlHttpRequestResponseType::Json);

        request.open("POST", &self.address).unwrap();
        let peer_clone = self.peer.clone();
        let on_request_response_callback: wasm_bindgen::prelude::Closure<dyn FnMut() -> ()> =
            Closure::new(move || match request_clone.status() {
                Ok(200) => {
                    tracing::info!("Successful session request!");
                    let response = request_clone.response().unwrap();
                    tracing::info!("{:?}", response);
                    let answer_sdp;

                    let answer_obj = Reflect::get(&response, &JsValue::from_str("answer")).unwrap();
                    answer_sdp = Reflect::get(&answer_obj, &JsValue::from_str("sdp"))
                        .unwrap()
                        .as_string()
                        .unwrap();

                    tracing::info!("1");
                    let mut session_description_init =
                        RtcSessionDescriptionInit::new(RtcSdpType::Answer);
                    session_description_init.sdp(&answer_sdp);
                    tracing::info!("2");
                    let candidate_response: String;
                    let sdp_m_line_index: u16;
                    let sdp_mid: String;

                    let candidate_response_js =
                        Reflect::get(&response, &JsValue::from_str("candidate")).unwrap();
                    tracing::info!("CandidateJS: {:?}", candidate_response_js);
                    candidate_response =
                        Reflect::get(&candidate_response_js, &JsValue::from_str("candidate"))
                            .unwrap()
                            .as_string()
                            .unwrap();
                    sdp_m_line_index =
                        Reflect::get(&candidate_response_js, &JsValue::from_str("sdpMLineIndex"))
                            .unwrap()
                            .as_f64()
                            .unwrap() as u16;

                    sdp_mid = Reflect::get(&candidate_response_js, &JsValue::from_str("sdpMid"))
                        .unwrap()
                        .as_string()
                        .unwrap();

                    tracing::info!("Candidate: {:?}", candidate_response);
                    let mut candidate = RtcIceCandidateInit::new(&candidate_response);
                    candidate.sdp_m_line_index(Some(sdp_m_line_index));
                    candidate.sdp_mid(Some(&sdp_mid));

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

        match my_listener.recv().await {
            Ok(_) => (),
            _ => panic!("Failed"),
        };
    }
}
