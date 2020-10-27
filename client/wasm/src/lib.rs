#![feature(async_closure)]
mod utils;

use futures::{poll, Future, TryFutureExt};
use js_sys::{Reflect, JSON};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{JsFuture, future_to_promise};
use web_sys::{
    MessageEvent, RtcDataChannel, RtcDataChannelEvent, RtcDataChannelInit, RtcDataChannelType,
    RtcIceCandidate, RtcIceCandidateInit, RtcPeerConnection, RtcPeerConnectionIceEvent, RtcSdpType,
    RtcSessionDescription, RtcSessionDescriptionInit, XmlHttpRequest, XmlHttpRequestResponseType,
};

#[wasm_bindgen]
pub fn init_panic_hook() {
    utils::set_hooks();
}

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    let peer: RtcPeerConnection = RtcPeerConnection::new()?;
    tracing::info!(
        "Created peer connection with state {:?}",
        peer.signaling_state()
    );
    let mut channel_dict = RtcDataChannelInit::new();
    channel_dict.max_retransmits(0).ordered(false);
    let channel: RtcDataChannel =
        peer.create_data_channel_with_data_channel_dict("webudp", &channel_dict);
    channel.set_binary_type(RtcDataChannelType::Arraybuffer);

    let on_message_callback =
        Closure::wrap(
            Box::new(move |event: MessageEvent| match event.data().as_string() {
                Some(message) => {
                    tracing::info!("Received message {:?}", message);
                }
                None => tracing::error!("Message: {:?}", event),
            }) as Box<dyn FnMut(MessageEvent)>,
        );

    channel.set_onmessage(Some(on_message_callback.as_ref().unchecked_ref()));
    on_message_callback.forget();

    let inner_channel = channel.clone();
    let on_channel_open =
        Closure::wrap(
            Box::new(move |_: MessageEvent| {
                    tracing::info!("Channel opened");
                    inner_channel.send_with_str("Test").unwrap();
            }) as Box<dyn FnMut(MessageEvent)>,
        );

    channel.set_onopen(Some(on_channel_open.as_ref().unchecked_ref()));
    on_channel_open.forget();

    let on_ice_candidate_callback =
        Closure::wrap(
            Box::new(move |ev: RtcPeerConnectionIceEvent| match ev.candidate() {
                Some(candidate) => {
                    tracing::info!("Peer on ice candidate: {:#?}", candidate.candidate());
                }
                None => {
                    tracing::info!("All local candidates received");
                }
            }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>,
        );
    peer.set_onicecandidate(Some(on_ice_candidate_callback.as_ref().unchecked_ref()));
    on_ice_candidate_callback.forget();

    let offer = JsFuture::from(peer.create_offer()).await?;
    let offer_sdp;
    unsafe {
        offer_sdp = Reflect::get(&offer, &JsValue::from_str("sdp"))?
            .as_string()
            .unwrap();
    }
    tracing::info!("Peer offer {:?}", offer_sdp);

    let mut offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
    offer_obj.sdp(&offer_sdp);
    let sld_promise = peer.set_local_description(&offer_obj);
    JsFuture::from(sld_promise).await?;
    tracing::info!("Peer state: {:?}", peer.signaling_state());

    let request = XmlHttpRequest::new()?;
    let request_clone = request.clone();
    request.set_response_type(XmlHttpRequestResponseType::Json);

    let address = "http://localhost:8080/session";

    request.open("POST", address)?;
    let peer_clone = peer.clone();
    let on_request_response_callback: wasm_bindgen::prelude::Closure<dyn FnMut() -> ()> =
        Closure::new(move || match request_clone.status() {
            Ok(200) => {
                tracing::info!("Successful session request!");
                let response = request_clone.response().unwrap();
                tracing::info!("{:?}", response);
                let answer_sdp;
                unsafe {
                    let answer_obj = Reflect::get(&response, &JsValue::from_str("answer")).unwrap();
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
                    candidate_response = Reflect::get(&candidate_response_js, &JsValue::from_str("candidate"))
                        .unwrap()
                        .as_string()
                        .unwrap();
                    sdp_m_line_index = Reflect::get(&candidate_response_js, &JsValue::from_str("sdpMLineIndex"))
                        .unwrap()
                        .as_f64()
                        .unwrap() as u16;
                        
                    sdp_mid = Reflect::get(&candidate_response_js, &JsValue::from_str("sdpMid"))
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
                    another_peer.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(
                        &candidate,
                    )).then(&ice_closure);
                });
                
                peer_clone.set_remote_description(&session_description_init).then(&description_closure);
                description_closure.forget();

                /*
                                        .and_then(|_| async {
                            tracing::info!("Set the remote description");
                            Ok(JsFuture::from(
                                
                            ))
                        })
                        .and_then(|_| async {
                            tracing::info!("Successfully set the ice candidate!");
                            Ok(())
                        }).or_else(|_| { Ok(())})
                */
                tracing::info!("4");
            }
            _ => {
                tracing::error!("Failed to request session");
            }
        });
    request.set_onload(Some(on_request_response_callback.as_ref().unchecked_ref()));
    on_request_response_callback.forget();

    request.send_with_opt_str(Some(&peer.local_description().unwrap().sdp()))?;

    Ok(())
}

#[wasm_bindgen]
pub fn add_two_ints(a: u32, b: u32) -> u32 {
    a + b + 4
}

#[wasm_bindgen]
pub fn fib(n: u32) -> u32 {
    if n == 0 || n == 1 {
        return n;
    }

    fib(n - 1) + fib(n - 2)
}
