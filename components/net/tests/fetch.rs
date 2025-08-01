/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![cfg(not(target_os = "windows"))]

use std::fs;
use std::iter::FromIterator;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use base::id::{TEST_PIPELINE_ID, TEST_WEBVIEW_ID};
use content_security_policy as csp;
use crossbeam_channel::{Sender, unbounded};
use devtools_traits::{HttpRequest as DevtoolsHttpRequest, HttpResponse as DevtoolsHttpResponse};
use headers::{
    AccessControlAllowCredentials, AccessControlAllowHeaders, AccessControlAllowMethods,
    AccessControlAllowOrigin, AccessControlMaxAge, CacheControl, ContentLength, ContentType,
    Expires, HeaderMapExt, LastModified, Pragma, StrictTransportSecurity, UserAgent,
};
use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use http::{Method, StatusCode};
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Request as HyperRequest, Response as HyperResponse};
use mime::{self, Mime};
use net::fetch::cors_cache::CorsCache;
use net::fetch::methods::{self, FetchContext};
use net::filemanager_thread::FileManager;
use net::hsts::HstsEntry;
use net::protocols::ProtocolRegistry;
use net::request_interceptor::RequestInterceptor;
use net::resource_thread::CoreResourceThreadPool;
use net_traits::filemanager_thread::FileTokenCheck;
use net_traits::http_status::HttpStatus;
use net_traits::request::{
    Destination, RedirectMode, Referrer, Request, RequestBuilder, RequestMode,
};
use net_traits::response::{CacheState, Response, ResponseBody, ResponseType};
use net_traits::{
    FetchTaskTarget, IncludeSubdomains, NetworkError, ReferrerPolicy, ResourceFetchTiming,
    ResourceTimingType,
};
use servo_arc::Arc as ServoArc;
use servo_url::ServoUrl;
use uuid::Uuid;

use crate::http_loader::{devtools_response_with_body, expect_devtools_http_request};
use crate::{
    DEFAULT_USER_AGENT, create_embedder_proxy, create_embedder_proxy_and_receiver,
    create_http_state, fetch, fetch_with_context, fetch_with_cors_cache, make_body, make_server,
    make_ssl_server, new_fetch_context,
};

// TODO write a struct that impls Handler for storing test values

#[test]
fn test_fetch_response_is_not_network_error() {
    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let fetch_response = fetch(request, None);
    let _ = server.close();

    if fetch_response.is_network_error() {
        panic!("fetch response shouldn't be a network error");
    }
}

#[test]
fn test_fetch_on_bad_port_is_network_error() {
    let url = ServoUrl::parse("http://www.example.org:6667").unwrap();
    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let fetch_response = fetch(request, None);
    assert!(fetch_response.is_network_error());
    let fetch_error = fetch_response.get_network_error().unwrap();
    assert_eq!(
        fetch_error,
        &NetworkError::Internal("Request attempted on bad port".into())
    )
}

#[test]
fn test_fetch_response_body_matches_const_message() {
    static MESSAGE: &'static [u8] = b"Hello World!";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let fetch_response = fetch(request, None);
    let _ = server.close();

    assert!(!fetch_response.is_network_error());
    assert_eq!(fetch_response.response_type, ResponseType::Basic);

    match *fetch_response.body.lock().unwrap() {
        ResponseBody::Done(ref body) => {
            assert_eq!(&**body, MESSAGE);
        },
        _ => panic!(),
    };
}

#[test]
fn test_fetch_aboutblank() {
    let url = ServoUrl::parse("about:blank").unwrap();
    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();

    let fetch_response = fetch(request, None);
    // We should see an opaque-filtered response.
    assert_eq!(fetch_response.response_type, ResponseType::Opaque);
    assert!(!fetch_response.is_network_error());
    assert_eq!(fetch_response.headers.len(), 0);
    let resp_body = fetch_response.body.lock().unwrap();
    assert_eq!(*resp_body, ResponseBody::Empty);

    // The underlying response behind the filter should
    // have a 0-byte body.
    let actual_response = fetch_response.actual_response();
    assert!(!actual_response.is_network_error());
    let resp_body = actual_response.body.lock().unwrap();
    assert_eq!(*resp_body, ResponseBody::Done(vec![]));
}

#[test]
fn test_fetch_blob() {
    use net_traits::blob_url_store::BlobBuf;

    struct FetchResponseCollector {
        sender: Sender<Response>,
        buffer: Vec<u8>,
        expected: Vec<u8>,
    }

    impl FetchTaskTarget for FetchResponseCollector {
        fn process_request_body(&mut self, _: &Request) {}
        fn process_request_eof(&mut self, _: &Request) {}
        fn process_response(&mut self, _: &Request, _: &Response) {}
        fn process_response_chunk(&mut self, _: &Request, chunk: Vec<u8>) {
            self.buffer.extend_from_slice(chunk.as_slice());
        }
        /// Fired when the response is fully fetched
        fn process_response_eof(&mut self, _: &Request, response: &Response) {
            assert_eq!(self.buffer, self.expected);
            let _ = self.sender.send(response.clone());
        }
        fn process_csp_violations(&mut self, _: &Request, _: Vec<csp::Violation>) {}
    }

    let context = new_fetch_context(None, None, None);

    let bytes = b"content";
    let blob_buf = BlobBuf {
        filename: Some("test.txt".into()),
        type_string: "text/plain".into(),
        size: bytes.len() as u64,
        bytes: bytes.to_vec(),
    };

    let origin = ServoUrl::parse("http://www.example.org/").unwrap();

    let id = Uuid::new_v4();
    context.filemanager.lock().unwrap().promote_memory(
        id.clone(),
        blob_buf,
        true,
        "http://www.example.org".into(),
    );
    let url = ServoUrl::parse(&format!("blob:{}{}", origin.as_str(), id.simple())).unwrap();

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(origin.origin())
        .build();

    let (sender, receiver) = unbounded();

    let mut target = FetchResponseCollector {
        sender,
        buffer: vec![],
        expected: bytes.to_vec(),
    };

    crate::HANDLE.block_on(methods::fetch(request, &mut target, &context));

    let fetch_response = receiver.recv().unwrap();
    assert!(!fetch_response.is_network_error());

    assert_eq!(fetch_response.headers.len(), 2);

    let content_type: Mime = fetch_response
        .headers
        .typed_get::<ContentType>()
        .unwrap()
        .into();
    assert_eq!(content_type, mime::TEXT_PLAIN);

    let content_length: ContentLength = fetch_response.headers.typed_get().unwrap();
    assert_eq!(content_length.0, bytes.len() as u64);

    assert_eq!(
        *fetch_response.body.lock().unwrap(),
        ResponseBody::Receiving(vec![])
    );
}

#[test]
fn test_file() {
    let path = Path::new("../../components/net/tests/test.css")
        .canonicalize()
        .unwrap();
    let url = ServoUrl::from_file_path(path.clone()).unwrap();

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();

    let pool = CoreResourceThreadPool::new(1, "CoreResourceTestPool".to_string());
    let pool_handle = Arc::new(pool);
    let mut context = new_fetch_context(None, None, Some(Arc::downgrade(&pool_handle)));
    let fetch_response = fetch_with_context(request, &mut context);

    // We should see an opaque-filtered response.
    assert_eq!(fetch_response.response_type, ResponseType::Opaque);

    assert!(!fetch_response.is_network_error());
    assert_eq!(fetch_response.headers.len(), 0);
    let resp_body = fetch_response.body.lock().unwrap();
    assert_eq!(*resp_body, ResponseBody::Empty);

    // The underlying response behind the filter should
    // have the file's MIME type and contents.
    let actual_response = fetch_response.actual_response();
    assert!(!actual_response.is_network_error());
    assert_eq!(actual_response.headers.len(), 1);
    let content_type: Mime = actual_response
        .headers
        .typed_get::<ContentType>()
        .unwrap()
        .into();
    assert_eq!(content_type, mime::TEXT_CSS);

    let resp_body = actual_response.body.lock().unwrap();
    let file = fs::read(path).unwrap();

    match *resp_body {
        ResponseBody::Done(ref val) => {
            assert_eq!(val, &file);
        },
        _ => panic!(),
    }
}

#[test]
fn test_fetch_ftp() {
    let url = ServoUrl::parse("ftp://not-supported").unwrap();
    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let fetch_response = fetch(request, None);
    assert!(fetch_response.is_network_error());
}

#[test]
fn test_fetch_bogus_scheme() {
    let url = ServoUrl::parse("bogus://whatever").unwrap();
    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let fetch_response = fetch(request, None);
    assert!(fetch_response.is_network_error());
}

#[test]
fn test_cors_preflight_fetch() {
    static ACK: &'static [u8] = b"ACK";
    let state = Arc::new(AtomicUsize::new(0));
    let handler =
        move |request: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            if request.method() == Method::OPTIONS &&
                state.clone().fetch_add(1, Ordering::SeqCst) == 0
            {
                assert!(
                    request
                        .headers()
                        .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
                );
                assert!(
                    !request
                        .headers()
                        .contains_key(header::ACCESS_CONTROL_REQUEST_HEADERS)
                );
                assert!(
                    !request
                        .headers()
                        .get(header::REFERER)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .contains("a.html")
                );
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowOrigin::ANY);
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowCredentials);
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowMethods::from_iter(vec![Method::GET]));
            } else {
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowOrigin::ANY);
                *response.body_mut() = make_body(ACK.to_vec());
            }
        };
    let (server, url) = make_server(handler);

    let target_url = url.clone().join("a.html").unwrap();
    let mut request = RequestBuilder::new(
        Some(TEST_WEBVIEW_ID),
        url,
        Referrer::ReferrerUrl(target_url),
    )
    .build();
    request.referrer_policy = ReferrerPolicy::Origin;
    request.use_cors_preflight = true;
    request.mode = RequestMode::CorsMode;
    let fetch_response = fetch(request, None);
    let _ = server.close();

    assert!(!fetch_response.is_network_error());
    match *fetch_response.body.lock().unwrap() {
        ResponseBody::Done(ref body) => assert_eq!(&**body, ACK),
        _ => panic!(),
    };
}

#[test]
fn test_cors_preflight_cache_fetch() {
    static ACK: &'static [u8] = b"ACK";
    let state = Arc::new(AtomicUsize::new(0));
    let counter = state.clone();
    let mut cache = CorsCache::default();
    let handler =
        move |request: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            if request.method() == Method::OPTIONS &&
                state.clone().fetch_add(1, Ordering::SeqCst) == 0
            {
                assert!(
                    request
                        .headers()
                        .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
                );
                assert!(
                    !request
                        .headers()
                        .contains_key(header::ACCESS_CONTROL_REQUEST_HEADERS)
                );
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowOrigin::ANY);
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowCredentials);
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowMethods::from_iter(vec![Method::GET]));
                response
                    .headers_mut()
                    .typed_insert(AccessControlMaxAge::from(Duration::new(6000, 0)));
            } else {
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowOrigin::ANY);
                *response.body_mut() = make_body(ACK.to_vec());
            }
        };
    let (server, url) = make_server(handler);

    let mut request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url, Referrer::NoReferrer).build();
    request.use_cors_preflight = true;
    request.mode = RequestMode::CorsMode;
    let wrapped_request0 = request.clone();
    let wrapped_request1 = request.clone();
    let wrapped_request2 = request.clone();
    let wrapped_request3 = request;

    let fetch_response0 = fetch_with_cors_cache(wrapped_request0, &mut cache);
    let fetch_response1 = fetch_with_cors_cache(wrapped_request1, &mut cache);
    let _ = server.close();

    assert!(!fetch_response0.is_network_error() && !fetch_response1.is_network_error());

    // The response from the CORS-preflight cache was used
    assert_eq!(1, counter.load(Ordering::SeqCst));

    // The entry exists in the CORS-preflight cache
    assert_eq!(true, cache.match_method(&wrapped_request2, Method::GET));
    assert_eq!(true, cache.match_method(&wrapped_request3, Method::GET));

    match *fetch_response0.body.lock().unwrap() {
        ResponseBody::Done(ref body) => assert_eq!(&**body, ACK),
        _ => panic!(),
    };
    match *fetch_response1.body.lock().unwrap() {
        ResponseBody::Done(ref body) => assert_eq!(&**body, ACK),
        _ => panic!(),
    };
}

#[test]
fn test_cors_preflight_fetch_network_error() {
    static ACK: &'static [u8] = b"ACK";
    let state = Arc::new(AtomicUsize::new(0));
    let handler =
        move |request: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            if request.method() == Method::OPTIONS &&
                state.clone().fetch_add(1, Ordering::SeqCst) == 0
            {
                assert!(
                    request
                        .headers()
                        .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
                );
                assert!(
                    !request
                        .headers()
                        .contains_key(header::ACCESS_CONTROL_REQUEST_HEADERS)
                );
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowOrigin::ANY);
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowCredentials);
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowMethods::from_iter(vec![Method::GET]));
            } else {
                response
                    .headers_mut()
                    .typed_insert(AccessControlAllowOrigin::ANY);
                *response.body_mut() = make_body(ACK.to_vec());
            }
        };
    let (server, url) = make_server(handler);

    let mut request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url, Referrer::NoReferrer).build();
    request.method = Method::from_bytes(b"CHICKEN").unwrap();
    request.use_cors_preflight = true;
    request.mode = RequestMode::CorsMode;
    let fetch_response = fetch(request, None);
    let _ = server.close();

    assert!(fetch_response.is_network_error());
}

#[test]
fn test_fetch_response_is_basic_filtered() {
    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            response
                .headers_mut()
                .insert(header::SET_COOKIE, HeaderValue::from_static(""));
            // this header is obsoleted, so hyper doesn't implement it, but it's still covered by the spec
            response.headers_mut().insert(
                HeaderName::from_static("set-cookie2"),
                HeaderValue::from_bytes(&vec![]).unwrap(),
            );

            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let fetch_response = fetch(request, None);
    let _ = server.close();

    assert!(!fetch_response.is_network_error());
    assert_eq!(fetch_response.response_type, ResponseType::Basic);

    let headers = fetch_response.headers;
    assert!(!headers.contains_key(header::SET_COOKIE));
    assert!(
        headers
            .get(HeaderName::from_static("set-cookie2"))
            .is_none()
    );
}

#[test]
fn test_fetch_response_is_cors_filtered() {
    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            // this is mandatory for the Cors Check to pass
            // TODO test using different url encodings with this value ie. punycode
            response
                .headers_mut()
                .typed_insert(AccessControlAllowOrigin::ANY);

            // these are the headers that should be kept after filtering
            response.headers_mut().typed_insert(CacheControl::new());
            response.headers_mut().insert(
                header::CONTENT_LANGUAGE,
                HeaderValue::from_bytes(&vec![]).unwrap(),
            );
            response
                .headers_mut()
                .typed_insert(ContentType::from(mime::TEXT_HTML));
            response
                .headers_mut()
                .typed_insert(Expires::from(SystemTime::now() + Duration::new(86400, 0)));
            response
                .headers_mut()
                .typed_insert(LastModified::from(SystemTime::now()));
            response.headers_mut().typed_insert(Pragma::no_cache());

            // these headers should not be kept after filtering, even though they are given a pass
            response
                .headers_mut()
                .insert(header::SET_COOKIE, HeaderValue::from_static(""));
            response.headers_mut().insert(
                HeaderName::from_static("set-cookie2"),
                HeaderValue::from_bytes(&vec![]).unwrap(),
            );
            response
                .headers_mut()
                .typed_insert(AccessControlAllowHeaders::from_iter(vec![
                    HeaderName::from_static("set-cookie"),
                    HeaderName::from_static("set-cookie2"),
                ]));

            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    // an origin mis-match will stop it from defaulting to a basic filtered response
    let mut request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url, Referrer::NoReferrer).build();
    request.mode = RequestMode::CorsMode;
    let fetch_response = fetch(request, None);
    let _ = server.close();

    assert!(!fetch_response.is_network_error());
    assert_eq!(fetch_response.response_type, ResponseType::Cors);

    let headers = fetch_response.headers;
    assert!(headers.contains_key(header::CACHE_CONTROL));
    assert!(headers.contains_key(header::CONTENT_LANGUAGE));
    assert!(headers.contains_key(header::CONTENT_TYPE));
    assert!(headers.contains_key(header::EXPIRES));
    assert!(headers.contains_key(header::LAST_MODIFIED));
    assert!(headers.contains_key(header::PRAGMA));

    assert!(!headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    assert!(!headers.contains_key(header::SET_COOKIE));
    assert!(
        headers
            .get(HeaderName::from_static("set-cookie2"))
            .is_none()
    );
}

#[test]
fn test_fetch_response_is_opaque_filtered() {
    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    // an origin mis-match will fall through to an Opaque filtered response
    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url, Referrer::NoReferrer).build();
    let fetch_response = fetch(request, None);
    let _ = server.close();

    assert!(!fetch_response.is_network_error());
    assert_eq!(fetch_response.response_type, ResponseType::Opaque);

    assert!(fetch_response.url().is_none());
    assert!(fetch_response.url_list.is_empty());
    // this also asserts that status message is "the empty byte sequence"
    assert!(fetch_response.status.is_error());
    assert_eq!(fetch_response.headers, HeaderMap::new());
    match *fetch_response.body.lock().unwrap() {
        ResponseBody::Empty => {},
        _ => panic!(),
    }
    match fetch_response.cache_state {
        CacheState::None => {},
        _ => panic!(),
    }
}

#[test]
fn test_fetch_response_is_opaque_redirect_filtered() {
    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |request: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            let redirects = request
                .uri()
                .path()
                .split("/")
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);

            if redirects == 1 {
                *response.body_mut() = make_body(MESSAGE.to_vec());
            } else {
                *response.status_mut() = StatusCode::FOUND;
                response
                    .headers_mut()
                    .insert(header::LOCATION, HeaderValue::from_static("1"));
            }
        };

    let (server, url) = make_server(handler);

    let mut request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    request.redirect_mode = RedirectMode::Manual;
    let fetch_response = fetch(request, None);
    let _ = server.close();

    assert!(!fetch_response.is_network_error());
    assert_eq!(fetch_response.response_type, ResponseType::OpaqueRedirect);

    // this also asserts that status message is "the empty byte sequence"
    assert!(fetch_response.status.is_error());
    assert_eq!(fetch_response.headers, HeaderMap::new());
    match *fetch_response.body.lock().unwrap() {
        ResponseBody::Empty => {},
        _ => panic!(),
    }
    match fetch_response.cache_state {
        CacheState::None => {},
        _ => panic!(),
    }
}

#[test]
fn test_fetch_with_local_urls_only() {
    // If flag `local_urls_only` is set, fetching a non-local URL must result in network error.

    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, server_url) = make_server(handler);

    let do_fetch = |url: ServoUrl| {
        let mut request =
            RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
                .origin(url.origin())
                .build();

        // Set the flag.
        request.local_urls_only = true;

        fetch(request, None)
    };

    let local_url = ServoUrl::parse("about:blank").unwrap();
    let local_response = do_fetch(local_url);
    let server_response = do_fetch(server_url);

    let _ = server.close();

    assert!(!local_response.is_network_error());
    assert!(server_response.is_network_error());
}
// NOTE(emilio): If this test starts failing:
//
// openssl req -x509 -nodes -days 3650 -newkey rsa:2048 \
//   -keyout resources/privatekey_for_testing.key       \
//   -out resources/self_signed_certificate_for_testing.crt
//
// And make sure to specify `localhost` as the server name.
#[test]
fn test_fetch_with_hsts() {
    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };

    let (server, url) = make_ssl_server(handler);

    let embedder_proxy = create_embedder_proxy();

    let mut context = FetchContext {
        state: Arc::new(create_http_state(None)),
        user_agent: DEFAULT_USER_AGENT.into(),
        devtools_chan: None,
        filemanager: Arc::new(Mutex::new(FileManager::new(
            embedder_proxy.clone(),
            Weak::new(),
        ))),
        file_token: FileTokenCheck::NotRequired,
        request_interceptor: Arc::new(Mutex::new(RequestInterceptor::new(embedder_proxy))),
        cancellation_listener: Arc::new(Default::default()),
        timing: ServoArc::new(Mutex::new(ResourceFetchTiming::new(
            ResourceTimingType::Navigation,
        ))),
        protocols: Arc::new(ProtocolRegistry::default()),
    };

    // The server certificate is self-signed, so we need to add an override
    // so that the connection works properly.
    for certificate in server.certificates.as_ref().unwrap().iter() {
        context.state.override_manager.add_override(certificate);
    }

    {
        let mut list = context.state.hsts_list.write().unwrap();
        list.push(
            HstsEntry::new("localhost".to_owned(), IncludeSubdomains::NotIncluded, None).unwrap(),
        );
    }
    let mut request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    // Set the flag.
    request.local_urls_only = false;
    let response = fetch_with_context(request, &mut context);
    server.close();
    assert_eq!(
        response.internal_response.unwrap().url().unwrap().scheme(),
        "https"
    );
}

#[test]
fn test_load_adds_host_to_hsts_list_when_url_is_https() {
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            response
                .headers_mut()
                .typed_insert(StrictTransportSecurity::excluding_subdomains(
                    Duration::from_secs(31536000),
                ));
            *response.body_mut() = make_body(b"Yay!".to_vec());
        };

    let (server, mut url) = make_ssl_server(handler);
    url.as_mut_url().set_scheme("https").unwrap();

    let embedder_proxy = create_embedder_proxy();

    let mut context = FetchContext {
        state: Arc::new(create_http_state(None)),
        user_agent: DEFAULT_USER_AGENT.into(),
        devtools_chan: None,
        filemanager: Arc::new(Mutex::new(FileManager::new(
            embedder_proxy.clone(),
            Weak::new(),
        ))),
        file_token: FileTokenCheck::NotRequired,
        request_interceptor: Arc::new(Mutex::new(RequestInterceptor::new(embedder_proxy))),
        cancellation_listener: Arc::new(Default::default()),
        timing: ServoArc::new(Mutex::new(ResourceFetchTiming::new(
            ResourceTimingType::Navigation,
        ))),
        protocols: Arc::new(ProtocolRegistry::default()),
    };

    // The server certificate is self-signed, so we need to add an override
    // so that the connection works properly.
    for certificate in server.certificates.as_ref().unwrap().iter() {
        context.state.override_manager.add_override(certificate);
    }

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .method(Method::GET)
        .body(None)
        .destination(Destination::Document)
        .origin(url.clone().origin())
        .pipeline_id(Some(TEST_PIPELINE_ID))
        .build();

    let response = fetch_with_context(request, &mut context);

    let _ = server.close();

    assert!(
        response
            .internal_response
            .unwrap()
            .status
            .code()
            .is_success()
    );
    assert!(
        context
            .state
            .hsts_list
            .read()
            .unwrap()
            .is_host_secure(url.host_str().unwrap())
    );
}

#[test]
fn test_fetch_self_signed() {
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(b"Yay!".to_vec());
        };

    let (server, mut url) = make_ssl_server(handler);
    url.as_mut_url().set_scheme("https").unwrap();

    let embedder_proxy = create_embedder_proxy();

    let mut context = FetchContext {
        state: Arc::new(create_http_state(None)),
        user_agent: DEFAULT_USER_AGENT.into(),
        devtools_chan: None,
        filemanager: Arc::new(Mutex::new(FileManager::new(
            embedder_proxy.clone(),
            Weak::new(),
        ))),
        file_token: FileTokenCheck::NotRequired,
        request_interceptor: Arc::new(Mutex::new(RequestInterceptor::new(embedder_proxy))),
        cancellation_listener: Arc::new(Default::default()),
        timing: ServoArc::new(Mutex::new(ResourceFetchTiming::new(
            ResourceTimingType::Navigation,
        ))),
        protocols: Arc::new(ProtocolRegistry::default()),
    };

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .method(Method::GET)
        .body(None)
        .destination(Destination::Document)
        .origin(url.clone().origin())
        .pipeline_id(Some(TEST_PIPELINE_ID))
        .build();

    let response = fetch_with_context(request, &mut context);

    assert!(matches!(
        response.get_network_error(),
        Some(NetworkError::SslValidation(..))
    ));

    // The server certificate is self-signed, so we need to add an override
    // so that the connection works properly.
    for certificate in server.certificates.as_ref().unwrap().iter() {
        context.state.override_manager.add_override(certificate);
    }

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .method(Method::GET)
        .body(None)
        .destination(Destination::Document)
        .origin(url.clone().origin())
        .pipeline_id(Some(TEST_PIPELINE_ID))
        .build();

    let response = fetch_with_context(request, &mut context);

    assert!(response.status.code().is_success());

    let _ = server.close();
}

#[test]
fn test_fetch_with_sri_network_error() {
    static MESSAGE: &'static [u8] = b"alert('Hello, Network Error');";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    let mut request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    // To calulate hash use :
    // echo -n "alert('Hello, Network Error');" | openssl dgst -sha384 -binary | openssl base64 -A
    request.integrity_metadata =
        "sha384-H8BRh8j48O9oYatfu5AZzq6A9RINhZO5H16dQZngK7T62em8MUt1FLm52t+eX6xO".to_owned();
    // Set the flag.
    request.local_urls_only = false;

    let response = fetch(request, None);

    let _ = server.close();
    assert!(response.is_network_error());
}

#[test]
fn test_fetch_with_sri_sucess() {
    static MESSAGE: &'static [u8] = b"alert('Hello, world.');";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    let mut request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    // To calulate hash use :
    // echo -n "alert('Hello, Network Error');" | openssl dgst -sha384 -binary | openssl base64 -A
    request.integrity_metadata =
        "sha384-H8BRh8j48O9oYatfu5AZzq6A9RINhZO5H16dQZngK7T62em8MUt1FLm52t+eX6xO".to_owned();
    // Set the flag.
    request.local_urls_only = false;

    let response = fetch(request, None);

    let _ = server.close();
    assert_eq!(response_is_done(&response), true);
}

/// `fetch` should return a network error if there is a header `X-Content-Type-Options: nosniff`
#[test]
fn test_fetch_blocked_nosniff() {
    #[inline]
    fn test_nosniff_request(destination: Destination, mime: Mime, should_error: bool) {
        const MESSAGE: &'static [u8] = b"";
        const HEADER: &'static str = "x-content-type-options";
        const VALUE: &'static [u8] = b"nosniff";

        let handler =
            move |_: HyperRequest<Incoming>,
                  response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
                let mime_header = ContentType::from(mime.clone());
                response.headers_mut().typed_insert(mime_header);
                assert!(response.headers().contains_key(header::CONTENT_TYPE));
                // Add the nosniff header
                response.headers_mut().insert(
                    HeaderName::from_static(HEADER),
                    HeaderValue::from_bytes(VALUE).unwrap(),
                );

                *response.body_mut() = make_body(MESSAGE.to_vec());
            };

        let (server, url) = make_server(handler);

        let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
            .origin(url.origin())
            .destination(destination)
            .build();
        let fetch_response = fetch(request, None);
        let _ = server.close();

        assert_eq!(fetch_response.is_network_error(), should_error);
    }

    let tests = vec![
        (Destination::Script, mime::TEXT_JAVASCRIPT, false),
        (Destination::Script, mime::TEXT_CSS, true),
        (Destination::Style, mime::TEXT_CSS, false),
    ];

    for test in tests {
        let (destination, mime, should_error) = test;
        test_nosniff_request(destination, mime, should_error);
    }
}

fn setup_server_and_fetch(message: &'static [u8], redirect_cap: u32) -> Response {
    let handler =
        move |request: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            let redirects = request
                .uri()
                .path()
                .split("/")
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);

            if redirects >= redirect_cap {
                *response.body_mut() = make_body(message.to_vec());
            } else {
                *response.status_mut() = StatusCode::FOUND;
                let url = format!("{redirects}", redirects = redirects + 1);
                response
                    .headers_mut()
                    .insert(header::LOCATION, HeaderValue::from_str(&url).unwrap());
            }
        };

    let (server, url) = make_server(handler);

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let fetch_response = fetch(request, None);
    let _ = server.close();
    fetch_response
}

#[test]
fn test_fetch_redirect_count_ceiling() {
    static MESSAGE: &'static [u8] = b"no more redirects";
    // how many redirects to cause
    let redirect_cap = 20;

    let fetch_response = setup_server_and_fetch(MESSAGE, redirect_cap);

    assert!(!fetch_response.is_network_error());
    assert_eq!(fetch_response.response_type, ResponseType::Basic);

    match *fetch_response.body.lock().unwrap() {
        ResponseBody::Done(ref body) => {
            assert_eq!(&**body, MESSAGE);
        },
        _ => panic!(),
    };
}

#[test]
fn test_fetch_redirect_count_failure() {
    static MESSAGE: &'static [u8] = b"this message shouldn't be reachable";
    // how many redirects to cause
    let redirect_cap = 21;

    let fetch_response = setup_server_and_fetch(MESSAGE, redirect_cap);

    assert!(fetch_response.is_network_error());

    match *fetch_response.body.lock().unwrap() {
        ResponseBody::Done(_) | ResponseBody::Receiving(_) => panic!(),
        _ => {},
    };
}

fn test_fetch_redirect_updates_method_runner(
    tx: Sender<bool>,
    status_code: StatusCode,
    method: Method,
) {
    let handler_method = method.clone();
    let handler_tx = Arc::new(tx);

    let handler =
        move |request: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            let redirects = request
                .uri()
                .path()
                .split("/")
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);

            let mut test_pass = true;

            if redirects == 0 {
                *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
                response
                    .headers_mut()
                    .insert(header::LOCATION, HeaderValue::from_static("1"));
            } else if redirects == 1 {
                // this makes sure that the request method does't change from the wrong status code
                if handler_method != Method::GET && request.method() == Method::GET {
                    test_pass = false;
                }
                *response.status_mut() = status_code;
                response
                    .headers_mut()
                    .insert(header::LOCATION, HeaderValue::from_static("2"));
            } else if request.method() != Method::GET {
                test_pass = false;
            }

            // the first time this handler is reached, nothing is being tested, so don't send anything
            if redirects > 0 {
                handler_tx.send(test_pass).unwrap();
            }
        };

    let (server, url) = crate::make_server(handler);

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .method(method)
        .build();

    let _ = fetch(request, None);
    let _ = server.close();
}

#[test]
fn test_fetch_redirect_updates_method() {
    let (tx, rx) = unbounded();

    test_fetch_redirect_updates_method_runner(
        tx.clone(),
        StatusCode::MOVED_PERMANENTLY,
        Method::POST,
    );
    assert_eq!(rx.recv().unwrap(), true);
    assert_eq!(rx.recv().unwrap(), true);
    // make sure the test doesn't send more data than expected
    assert_eq!(rx.try_recv().is_err(), true);

    test_fetch_redirect_updates_method_runner(tx.clone(), StatusCode::FOUND, Method::POST);
    assert_eq!(rx.recv().unwrap(), true);
    assert_eq!(rx.recv().unwrap(), true);
    assert_eq!(rx.try_recv().is_err(), true);

    test_fetch_redirect_updates_method_runner(tx.clone(), StatusCode::SEE_OTHER, Method::GET);
    assert_eq!(rx.recv().unwrap(), true);
    assert_eq!(rx.recv().unwrap(), true);
    assert_eq!(rx.try_recv().is_err(), true);

    let extension = Method::from_bytes(b"FOO").unwrap();

    test_fetch_redirect_updates_method_runner(
        tx.clone(),
        StatusCode::MOVED_PERMANENTLY,
        extension.clone(),
    );
    assert_eq!(rx.recv().unwrap(), true);
    // for MovedPermanently and Found, Method should only be changed if it was Post
    assert_eq!(rx.recv().unwrap(), false);
    assert_eq!(rx.try_recv().is_err(), true);

    test_fetch_redirect_updates_method_runner(tx.clone(), StatusCode::FOUND, extension.clone());
    assert_eq!(rx.recv().unwrap(), true);
    assert_eq!(rx.recv().unwrap(), false);
    assert_eq!(rx.try_recv().is_err(), true);

    test_fetch_redirect_updates_method_runner(tx.clone(), StatusCode::SEE_OTHER, extension.clone());
    assert_eq!(rx.recv().unwrap(), true);
    // for SeeOther, Method should always be changed, so this should be true
    assert_eq!(rx.recv().unwrap(), true);
    assert_eq!(rx.try_recv().is_err(), true);
}

fn response_is_done(response: &Response) -> bool {
    let response_complete = match response.response_type {
        ResponseType::Default | ResponseType::Basic | ResponseType::Cors => {
            (*response.body.lock().unwrap()).is_done()
        },
        // if the internal response cannot have a body, it shouldn't block the "done" state
        ResponseType::Opaque | ResponseType::OpaqueRedirect | ResponseType::Error(..) => true,
    };

    let internal_complete = if let Some(ref res) = response.internal_response {
        res.body.lock().unwrap().is_done()
    } else {
        true
    };

    response_complete && internal_complete
}

#[test]
fn test_fetch_async_returns_complete_response() {
    static MESSAGE: &'static [u8] = b"this message should be retrieved in full";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let fetch_response = fetch(request, None);

    let _ = server.close();
    assert_eq!(response_is_done(&fetch_response), true);
}

#[test]
fn test_opaque_filtered_fetch_async_returns_complete_response() {
    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };
    let (server, url) = make_server(handler);

    // an origin mis-match will fall through to an Opaque filtered response
    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url, Referrer::NoReferrer).build();
    let fetch_response = fetch(request, None);

    let _ = server.close();

    assert_eq!(fetch_response.response_type, ResponseType::Opaque);
    assert_eq!(response_is_done(&fetch_response), true);
}

#[test]
fn test_opaque_redirect_filtered_fetch_async_returns_complete_response() {
    static MESSAGE: &'static [u8] = b"";
    let handler =
        move |request: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            let redirects = request
                .uri()
                .path()
                .split("/")
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);

            if redirects == 1 {
                *response.body_mut() = make_body(MESSAGE.to_vec());
            } else {
                *response.status_mut() = StatusCode::FOUND;
                response
                    .headers_mut()
                    .insert(header::LOCATION, HeaderValue::from_static("1"));
            }
        };

    let (server, url) = make_server(handler);
    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .redirect_mode(RedirectMode::Manual)
        .build();

    let fetch_response = fetch(request, None);

    let _ = server.close();

    assert_eq!(fetch_response.response_type, ResponseType::OpaqueRedirect);
    assert_eq!(response_is_done(&fetch_response), true);
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_fetch_with_devtools() {
    static MESSAGE: &'static [u8] = b"Yay!";
    let handler =
        move |_: HyperRequest<Incoming>,
              response: &mut HyperResponse<BoxBody<Bytes, hyper::Error>>| {
            *response.body_mut() = make_body(MESSAGE.to_vec());
        };

    let (server, url) = make_server(handler);

    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .redirect_mode(RedirectMode::Manual)
        .pipeline_id(Some(TEST_PIPELINE_ID))
        .build();

    let (devtools_chan, devtools_port) = unbounded();

    let _ = fetch(request, Some(devtools_chan));
    let _ = server.close();

    // notification received from devtools
    let devhttprequests = expect_devtools_http_request(&devtools_port);
    let mut devhttpresponse = devtools_response_with_body(&devtools_port);

    //Creating default headers for request
    let mut headers = HeaderMap::new();

    headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));

    headers.insert(
        header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("en-US,en;q=0.5"),
    );

    headers.typed_insert::<UserAgent>(DEFAULT_USER_AGENT.parse().unwrap());

    headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br"),
    );

    // Append fetch metadata headers
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("empty"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("no-cors"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin"),
    );

    let httprequest = DevtoolsHttpRequest {
        url: url,
        method: Method::GET,
        headers: headers,
        body: Some(vec![]),
        pipeline_id: TEST_PIPELINE_ID,
        started_date_time: devhttprequests.1.started_date_time,
        time_stamp: devhttprequests.1.time_stamp,
        connect_time: devhttprequests.1.connect_time,
        send_time: devhttprequests.1.send_time,
        destination: Destination::None,
        is_xhr: true,
        browsing_context_id: TEST_WEBVIEW_ID.0,
    };

    let content = "Yay!";
    let mut response_headers = HeaderMap::new();
    response_headers.typed_insert(ContentLength(content.len() as u64));
    devhttpresponse
        .headers
        .as_mut()
        .unwrap()
        .remove(header::DATE);

    let httpresponse = DevtoolsHttpResponse {
        headers: Some(response_headers),
        status: HttpStatus::default(),
        body: Some(content.as_bytes().to_vec()),
        pipeline_id: TEST_PIPELINE_ID,
        browsing_context_id: TEST_WEBVIEW_ID.0,
    };

    assert_eq!(devhttprequests.1, httprequest);
    assert_eq!(devhttpresponse, httpresponse);
}

#[test]
fn test_fetch_request_intercepted() {
    static BODY_PART1: &[u8] = b"Request is";
    static BODY_PART2: &[u8] = b" intercepted";
    static EXPECTED_BODY: &[u8] = b"Request is intercepted";
    static HEADERNAME: &str = "custom-header";
    static HEADERVALUE: &str = "custom-value";
    static STATUS_MESSAGE: &[u8] = b"custom status message";

    let (embedder_proxy, embedder_receiver) = create_embedder_proxy_and_receiver();

    std::thread::spawn(move || {
        let embedder_msg = embedder_receiver.recv().unwrap();
        match embedder_msg {
            embedder_traits::EmbedderMsg::WebResourceRequested(
                _,
                web_resource_request,
                response_sender,
            ) => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    HeaderName::from_static(HEADERNAME),
                    HeaderValue::from_static(HEADERVALUE),
                );
                let response =
                    embedder_traits::WebResourceResponse::new(web_resource_request.url.clone())
                        .headers(headers)
                        .status_code(StatusCode::FOUND)
                        .status_message(STATUS_MESSAGE.to_vec());
                let msg = embedder_traits::WebResourceResponseMsg::Start(response);
                let _ = response_sender.send(msg);
                let msg2 =
                    embedder_traits::WebResourceResponseMsg::SendBodyData(BODY_PART1.to_vec());
                let _ = response_sender.send(msg2);
                let msg3 =
                    embedder_traits::WebResourceResponseMsg::SendBodyData(BODY_PART2.to_vec());
                let _ = response_sender.send(msg3);
                let _ = response_sender.send(embedder_traits::WebResourceResponseMsg::FinishLoad);
            },
            _ => unreachable!(),
        }
    });

    let mut context = FetchContext {
        state: Arc::new(create_http_state(None)),
        user_agent: DEFAULT_USER_AGENT.into(),
        devtools_chan: None,
        filemanager: Arc::new(Mutex::new(FileManager::new(
            embedder_proxy.clone(),
            Weak::new(),
        ))),
        file_token: FileTokenCheck::NotRequired,
        request_interceptor: Arc::new(Mutex::new(RequestInterceptor::new(embedder_proxy))),
        cancellation_listener: Arc::new(Default::default()),
        timing: ServoArc::new(Mutex::new(ResourceFetchTiming::new(
            ResourceTimingType::Navigation,
        ))),
        protocols: Arc::new(ProtocolRegistry::default()),
    };

    let url = ServoUrl::parse("http://www.example.org").unwrap();
    let request = RequestBuilder::new(Some(TEST_WEBVIEW_ID), url.clone(), Referrer::NoReferrer)
        .origin(url.origin())
        .build();
    let response = fetch_with_context(request, &mut context);

    assert!(
        response
            .headers
            .get(HEADERNAME)
            .map(|v| v == HEADERVALUE)
            .unwrap_or(false),
        "The custom header does not exist or has an incorrect value!"
    );

    let body = response.body.lock().unwrap();
    match &*body {
        ResponseBody::Done(data) => {
            assert_eq!(data, &EXPECTED_BODY, "Body content does not match");
        },
        _ => panic!("Expected ResponseBody::Done, but got {:?}", *body),
    }

    assert_eq!(
        response.status.code(),
        StatusCode::FOUND,
        "Status code does not match!"
    );

    assert_eq!(
        response.status.message(),
        STATUS_MESSAGE,
        "The status_message was not set correctly!"
    );
}
