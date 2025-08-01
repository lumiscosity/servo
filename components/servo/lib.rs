/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Servo, the mighty web browser engine from the future.
//!
//! This is a very simple library that wires all of Servo's components
//! together as type `Servo`, along with a generic client
//! implementing the `WindowMethods` trait, to create a working web
//! browser.
//!
//! The `Servo` type is responsible for configuring a
//! `Constellation`, which does the heavy lifting of coordinating all
//! of Servo's internal subsystems, including the `ScriptThread` and the
//! `LayoutThread`, as well maintains the navigation context.
//!
//! `Servo` is fed events from a generic type that implements the
//! `WindowMethods` trait.

mod clipboard_delegate;
mod javascript_evaluator;
mod proxies;
mod responders;
mod servo_delegate;
mod webview;
mod webview_delegate;

use std::cell::{Cell, RefCell};
use std::cmp::max;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::thread;

pub use base::id::WebViewId;
use base::id::{PipelineNamespace, PipelineNamespaceId};
#[cfg(feature = "bluetooth")]
use bluetooth::BluetoothThreadFactory;
#[cfg(feature = "bluetooth")]
use bluetooth_traits::BluetoothRequest;
use canvas_traits::webgl::{GlType, WebGLThreads};
use clipboard_delegate::StringRequest;
pub use compositing::WebRenderDebugOption;
use compositing::{IOCompositor, InitialCompositorState};
pub use compositing_traits::rendering_context::{
    OffscreenRenderingContext, RenderingContext, SoftwareRenderingContext, WindowRenderingContext,
};
use compositing_traits::{
    CompositorMsg, CompositorProxy, CrossProcessCompositorApi, WebrenderExternalImageHandlers,
    WebrenderExternalImageRegistry, WebrenderImageHandlerType,
};
#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "ios"),
    not(target_os = "android"),
    not(target_arch = "arm"),
    not(target_arch = "aarch64"),
    not(target_env = "ohos"),
))]
use constellation::content_process_sandbox_profile;
use constellation::{
    Constellation, FromEmbedderLogger, FromScriptLogger, InitialConstellationState,
    UnprivilegedContent,
};
use constellation_traits::{EmbedderToConstellationMessage, ScriptToConstellationChan};
use crossbeam_channel::{Receiver, Sender, unbounded};
use embedder_traits::FormControl as EmbedderFormControl;
use embedder_traits::user_content_manager::UserContentManager;
pub use embedder_traits::*;
use env_logger::Builder as EnvLoggerBuilder;
use fonts::SystemFontService;
#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "ios"),
    not(target_os = "android"),
    not(target_arch = "arm"),
    not(target_arch = "aarch64"),
    not(target_env = "ohos"),
))]
use gaol::sandbox::{ChildSandbox, ChildSandboxMethods};
pub use gleam::gl;
use gleam::gl::RENDERER;
use ipc_channel::ipc::{self, IpcSender};
use ipc_channel::router::ROUTER;
use javascript_evaluator::JavaScriptEvaluator;
pub use keyboard_types::{
    Code, CompositionEvent, CompositionState, Key, KeyState, Location, Modifiers, NamedKey,
};
use layout::LayoutFactoryImpl;
use log::{Log, Metadata, Record, debug, error, warn};
use media::{GlApi, NativeDisplay, WindowGLContext};
use net::protocols::ProtocolRegistry;
use net::resource_thread::new_resource_threads;
use profile::{mem as profile_mem, time as profile_time};
use profile_traits::mem::MemoryReportResult;
use profile_traits::{mem, time};
use script::{JSEngineSetup, ServiceWorkerManager};
use servo_config::opts::Opts;
use servo_config::prefs::Preferences;
use servo_config::{opts, pref, prefs};
use servo_delegate::DefaultServoDelegate;
use servo_geometry::{
    DeviceIndependentIntRect, convert_rect_to_css_pixel, convert_size_to_css_pixel,
};
use servo_media::ServoMedia;
use servo_media::player::context::GlContext;
use servo_url::ServoUrl;
use webgl::WebGLComm;
#[cfg(feature = "webgpu")]
pub use webgpu;
#[cfg(feature = "webgpu")]
use webgpu::swapchain::WGPUImageMap;
use webrender::{ONE_TIME_USAGE_HINT, RenderApiSender, ShaderPrecacheFlags, UploadMethod};
use webrender_api::{ColorF, DocumentId, FramePublishId};
use webview::WebViewInner;
#[cfg(feature = "webxr")]
pub use webxr;
pub use {
    background_hang_monitor, base, canvas_traits, devtools, devtools_traits, euclid, fonts,
    ipc_channel, layout_api, media, net, net_traits, profile, profile_traits, script,
    script_traits, servo_config as config, servo_config, servo_geometry, servo_url, style,
    style_traits, webrender_api,
};
#[cfg(feature = "bluetooth")]
pub use {bluetooth, bluetooth_traits};

use crate::proxies::ConstellationProxy;
use crate::responders::ServoErrorChannel;
pub use crate::servo_delegate::{ServoDelegate, ServoError};
use crate::webrender_api::FrameReadyParams;
pub use crate::webview::{WebView, WebViewBuilder};
pub use crate::webview_delegate::{
    AllowOrDenyRequest, AuthenticationRequest, ColorPicker, FormControl, NavigationRequest,
    PermissionRequest, SelectElement, WebResourceLoad, WebViewDelegate,
};

#[cfg(feature = "media-gstreamer")]
mod media_platform {
    #[cfg(any(windows, target_os = "macos"))]
    mod gstreamer_plugins {
        include!(concat!(env!("OUT_DIR"), "/gstreamer_plugins.rs"));
    }

    use servo_media_gstreamer::GStreamerBackend;

    use super::ServoMedia;

    #[cfg(any(windows, target_os = "macos"))]
    pub fn init() {
        ServoMedia::init_with_backend(|| {
            let mut plugin_dir = std::env::current_exe().unwrap();
            plugin_dir.pop();

            if cfg!(target_os = "macos") {
                plugin_dir.push("lib");
            }

            match GStreamerBackend::init_with_plugins(
                plugin_dir,
                gstreamer_plugins::GSTREAMER_PLUGINS,
            ) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error initializing GStreamer: {:?}", e);
                    std::process::exit(1);
                },
            }
        });
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn init() {
        ServoMedia::init::<GStreamerBackend>();
    }
}

#[cfg(not(feature = "media-gstreamer"))]
mod media_platform {
    use super::ServoMedia;
    pub fn init() {
        ServoMedia::init::<servo_media_dummy::DummyBackend>();
    }
}

/// The in-process interface to Servo.
///
/// It does everything necessary to render the web, primarily
/// orchestrating the interaction between JavaScript, CSS layout,
/// rendering, and the client window.
///
// Clients create an event loop to pump messages between the embedding
// application and various browser components.
pub struct Servo {
    delegate: RefCell<Rc<dyn ServoDelegate>>,
    compositor: Rc<RefCell<IOCompositor>>,
    constellation_proxy: ConstellationProxy,
    embedder_receiver: Receiver<EmbedderMsg>,
    /// A struct that tracks ongoing JavaScript evaluations and is responsible for
    /// calling the callback when the evaluation is complete.
    javascript_evaluator: Rc<RefCell<JavaScriptEvaluator>>,
    /// Tracks whether we are in the process of shutting down, or have shut down.
    /// This is shared with `WebView`s and the `ServoRenderer`.
    shutdown_state: Rc<Cell<ShutdownState>>,
    /// A map  [`WebView`]s that are managed by this [`Servo`] instance. These are stored
    /// as `Weak` references so that the embedding application can control their lifetime.
    /// When accessed, `Servo` will be reponsible for cleaning up the invalid `Weak`
    /// references.
    webviews: RefCell<HashMap<WebViewId, Weak<RefCell<WebViewInner>>>>,
    servo_errors: ServoErrorChannel,
    /// For single-process Servo instances, this field controls the initialization
    /// and deinitialization of the JS Engine. Multiprocess Servo instances have their
    /// own instance that exists in the content process instead.
    _js_engine_setup: Option<JSEngineSetup>,
    /// Whether or not any WebView in this instance is animating or WebXR is enabled.
    animating: Cell<bool>,
}

#[derive(Clone)]
struct RenderNotifier {
    compositor_proxy: CompositorProxy,
}

impl RenderNotifier {
    pub fn new(compositor_proxy: CompositorProxy) -> RenderNotifier {
        RenderNotifier { compositor_proxy }
    }
}

impl webrender_api::RenderNotifier for RenderNotifier {
    fn clone(&self) -> Box<dyn webrender_api::RenderNotifier> {
        Box::new(RenderNotifier::new(self.compositor_proxy.clone()))
    }

    fn wake_up(&self, _composite_needed: bool) {}

    fn new_frame_ready(
        &self,
        document_id: DocumentId,
        _: FramePublishId,
        frame_ready_params: &FrameReadyParams,
    ) {
        self.compositor_proxy
            .send(CompositorMsg::NewWebRenderFrameReady(
                document_id,
                frame_ready_params.render,
            ));
    }
}

impl Servo {
    #[servo_tracing::instrument(skip(builder))]
    fn new(builder: ServoBuilder) -> Self {
        // Global configuration options, parsed from the command line.
        let opts = builder.opts.map(|opts| *opts);
        opts::set_options(opts.unwrap_or_default());
        let opts = opts::get();

        // Set the preferences globally.
        // TODO: It would be better to make these private to a particular Servo instance.
        let preferences = builder.preferences.map(|opts| *opts);
        servo_config::prefs::set(preferences.unwrap_or_default());

        use std::sync::atomic::Ordering;

        style::context::DEFAULT_DISABLE_STYLE_SHARING_CACHE
            .store(opts.debug.disable_share_style_cache, Ordering::Relaxed);
        style::context::DEFAULT_DUMP_STYLE_STATISTICS
            .store(opts.debug.dump_style_statistics, Ordering::Relaxed);
        style::traversal::IS_SERVO_NONINCREMENTAL_LAYOUT
            .store(opts.nonincremental_layout, Ordering::Relaxed);

        if !opts.multiprocess {
            media_platform::init();
        }

        // Get GL bindings
        let rendering_context = builder.rendering_context;
        let webrender_gl = rendering_context.gleam_gl_api();

        // Make sure the gl context is made current.
        if let Err(err) = rendering_context.make_current() {
            warn!("Failed to make the rendering context current: {:?}", err);
        }
        debug_assert_eq!(webrender_gl.get_error(), gleam::gl::NO_ERROR,);

        // Reserving a namespace to create WebViewId.
        PipelineNamespace::install(PipelineNamespaceId(0));

        // Get both endpoints of a special channel for communication between
        // the client window and the compositor. This channel is unique because
        // messages to client may need to pump a platform-specific event loop
        // to deliver the message.
        let event_loop_waker = builder.event_loop_waker;
        let (compositor_proxy, compositor_receiver) =
            create_compositor_channel(event_loop_waker.clone());
        let (embedder_proxy, embedder_receiver) = create_embedder_channel(event_loop_waker.clone());
        let time_profiler_chan = profile_time::Profiler::create(
            &opts.time_profiling,
            opts.time_profiler_trace_path.clone(),
        );
        let mem_profiler_chan = profile_mem::Profiler::create();

        let devtools_sender = if pref!(devtools_server_enabled) {
            Some(devtools::start_server(
                pref!(devtools_server_port) as u16,
                embedder_proxy.clone(),
            ))
        } else {
            None
        };

        let (mut webrender, webrender_api_sender) = {
            let mut debug_flags = webrender::DebugFlags::empty();
            debug_flags.set(
                webrender::DebugFlags::PROFILER_DBG,
                opts.debug.webrender_stats,
            );

            rendering_context.prepare_for_rendering();
            let render_notifier = Box::new(RenderNotifier::new(compositor_proxy.clone()));
            let clear_color = servo_config::pref!(shell_background_color_rgba);
            let clear_color = ColorF::new(
                clear_color[0] as f32,
                clear_color[1] as f32,
                clear_color[2] as f32,
                clear_color[3] as f32,
            );

            // Use same texture upload method as Gecko with ANGLE:
            // https://searchfox.org/mozilla-central/source/gfx/webrender_bindings/src/bindings.rs#1215-1219
            let upload_method = if webrender_gl.get_string(RENDERER).starts_with("ANGLE") {
                UploadMethod::Immediate
            } else {
                UploadMethod::PixelBuffer(ONE_TIME_USAGE_HINT)
            };
            let worker_threads = thread::available_parallelism()
                .map(|i| i.get())
                .unwrap_or(pref!(threadpools_fallback_worker_num) as usize)
                .min(pref!(threadpools_webrender_workers_max).max(1) as usize);
            let workers = Some(Arc::new(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(worker_threads)
                    .thread_name(|idx| format!("WRWorker#{}", idx))
                    .build()
                    .unwrap(),
            ));
            webrender::create_webrender_instance(
                webrender_gl.clone(),
                render_notifier,
                webrender::WebRenderOptions {
                    // We force the use of optimized shaders here because rendering is broken
                    // on Android emulators with unoptimized shaders. This is due to a known
                    // issue in the emulator's OpenGL emulation layer.
                    // See: https://github.com/servo/servo/issues/31726
                    use_optimized_shaders: true,
                    resource_override_path: opts.shaders_dir.clone(),
                    debug_flags,
                    precache_flags: if pref!(gfx_precache_shaders) {
                        ShaderPrecacheFlags::FULL_COMPILE
                    } else {
                        ShaderPrecacheFlags::empty()
                    },
                    enable_aa: pref!(gfx_text_antialiasing_enabled),
                    enable_subpixel_aa: pref!(gfx_subpixel_text_antialiasing_enabled),
                    allow_texture_swizzling: pref!(gfx_texture_swizzling_enabled),
                    clear_color,
                    upload_method,
                    workers,
                    size_of_op: Some(servo_allocator::usable_size),
                    ..Default::default()
                },
                None,
            )
            .expect("Unable to initialize webrender!")
        };

        let webrender_api = webrender_api_sender.create_api();
        let webrender_document = webrender_api.add_document(rendering_context.size2d().to_i32());

        // Important that this call is done in a single-threaded fashion, we
        // can't defer it after `create_constellation` has started.
        let js_engine_setup = if !opts.multiprocess {
            Some(script::init())
        } else {
            None
        };

        // Create the webgl thread
        let gl_type = match webrender_gl.get_type() {
            gleam::gl::GlType::Gl => GlType::Gl,
            gleam::gl::GlType::Gles => GlType::Gles,
        };

        let (external_image_handlers, external_images) = WebrenderExternalImageHandlers::new();
        let mut external_image_handlers = Box::new(external_image_handlers);

        let WebGLComm {
            webgl_threads,
            #[cfg(feature = "webxr")]
            webxr_layer_grand_manager,
            image_handler,
        } = WebGLComm::new(
            rendering_context.clone(),
            compositor_proxy.cross_process_compositor_api.clone(),
            webrender_api.create_sender(),
            external_images.clone(),
            gl_type,
        );

        // Set webrender external image handler for WebGL textures
        external_image_handlers.set_handler(image_handler, WebrenderImageHandlerType::WebGL);

        // Create the WebXR main thread
        #[cfg(feature = "webxr")]
        let mut webxr_main_thread =
            webxr::MainThreadRegistry::new(event_loop_waker.clone(), webxr_layer_grand_manager)
                .expect("Failed to create WebXR device registry");
        #[cfg(feature = "webxr")]
        if pref!(dom_webxr_enabled) {
            builder.webxr_registry.register(&mut webxr_main_thread);
        }

        #[cfg(feature = "webgpu")]
        let wgpu_image_handler = webgpu::WGPUExternalImages::default();
        #[cfg(feature = "webgpu")]
        let wgpu_image_map = wgpu_image_handler.images.clone();
        #[cfg(feature = "webgpu")]
        external_image_handlers.set_handler(
            Box::new(wgpu_image_handler),
            WebrenderImageHandlerType::WebGPU,
        );

        WindowGLContext::initialize_image_handler(
            &mut external_image_handlers,
            external_images.clone(),
        );

        webrender.set_external_image_handler(external_image_handlers);

        // Create the constellation, which maintains the engine pipelines, including script and
        // layout, as well as the navigation context.
        let mut protocols = ProtocolRegistry::with_internal_protocols();
        protocols.merge(builder.protocol_registry);

        let constellation_chan = create_constellation(
            opts.config_dir.clone(),
            embedder_proxy,
            compositor_proxy.clone(),
            time_profiler_chan.clone(),
            mem_profiler_chan.clone(),
            devtools_sender,
            webrender_document,
            webrender_api_sender,
            #[cfg(feature = "webxr")]
            webxr_main_thread.registry(),
            Some(webgl_threads),
            external_images,
            #[cfg(feature = "webgpu")]
            wgpu_image_map,
            protocols,
            builder.user_content_manager,
        );

        // The compositor coordinates with the client window to create the final
        // rendered page and display it somewhere.
        let shutdown_state = Rc::new(Cell::new(ShutdownState::NotShuttingDown));
        let compositor = IOCompositor::new(
            InitialCompositorState {
                sender: compositor_proxy,
                receiver: compositor_receiver,
                constellation_chan: constellation_chan.clone(),
                time_profiler_chan,
                mem_profiler_chan,
                webrender,
                webrender_document,
                webrender_api,
                rendering_context,
                webrender_gl,
                #[cfg(feature = "webxr")]
                webxr_main_thread,
                shutdown_state: shutdown_state.clone(),
                event_loop_waker,
            },
            opts.debug.convert_mouse_to_touch,
        );

        let constellation_proxy = ConstellationProxy::new(constellation_chan);
        Self {
            delegate: RefCell::new(Rc::new(DefaultServoDelegate)),
            compositor: Rc::new(RefCell::new(compositor)),
            javascript_evaluator: Rc::new(RefCell::new(JavaScriptEvaluator::new(
                constellation_proxy.clone(),
            ))),
            constellation_proxy,
            embedder_receiver,
            shutdown_state,
            webviews: Default::default(),
            servo_errors: ServoErrorChannel::default(),
            _js_engine_setup: js_engine_setup,
            animating: Cell::new(false),
        }
    }

    pub fn delegate(&self) -> Rc<dyn ServoDelegate> {
        self.delegate.borrow().clone()
    }

    pub fn set_delegate(&self, delegate: Rc<dyn ServoDelegate>) {
        *self.delegate.borrow_mut() = delegate;
    }

    /// Whether or not any [`WebView`] of this Servo instance has animating content, such as a CSS
    /// animation or transition or is running `requestAnimationFrame` callbacks. In addition, this
    /// returns true if WebXR content is running. This indicates that the embedding application
    /// should be spinning the Servo event loop on regular intervals in order to trigger animation
    /// updates.
    pub fn animating(&self) -> bool {
        self.animating.get()
    }

    /// **EXPERIMENTAL:** Intialize GL accelerated media playback. This currently only works on a limited number
    /// of platforms. This should be run *before* calling [`Servo::new`] and creating the first [`WebView`].
    pub fn initialize_gl_accelerated_media(display: NativeDisplay, api: GlApi, context: GlContext) {
        WindowGLContext::initialize(display, api, context)
    }

    /// Spin the Servo event loop, which:
    ///
    ///   - Performs updates in the compositor, such as queued pinch zoom events
    ///   - Runs delebgate methods on all `WebView`s and `Servo` itself
    ///   - Maybe update the rendered compositor output, but *without* swapping buffers.
    ///
    /// The return value of this method indicates whether or not Servo, false indicates that Servo
    /// has finished shutting down and you should not spin the event loop any longer.
    pub fn spin_event_loop(&self) -> bool {
        if self.shutdown_state.get() == ShutdownState::FinishedShuttingDown {
            return false;
        }

        {
            let mut compositor = self.compositor.borrow_mut();
            let mut messages = Vec::new();
            while let Ok(message) = compositor.receiver().try_recv() {
                messages.push(message);
            }
            compositor.handle_messages(messages);
        }

        // Only handle incoming embedder messages if the compositor hasn't already started shutting down.
        while let Ok(message) = self.embedder_receiver.try_recv() {
            self.handle_embedder_message(message);

            if self.shutdown_state.get() == ShutdownState::FinishedShuttingDown {
                break;
            }
        }

        if self.constellation_proxy.disconnected() {
            self.delegate()
                .notify_error(self, ServoError::LostConnectionWithBackend);
        }

        self.compositor.borrow_mut().perform_updates();
        self.send_new_frame_ready_messages();
        self.send_animating_changed_messages();
        self.handle_delegate_errors();
        self.clean_up_destroyed_webview_handles();

        if self.shutdown_state.get() == ShutdownState::FinishedShuttingDown {
            return false;
        }

        true
    }

    fn send_new_frame_ready_messages(&self) {
        if !self.compositor.borrow().needs_repaint() {
            return;
        }

        for webview in self
            .webviews
            .borrow()
            .values()
            .filter_map(WebView::from_weak_handle)
        {
            webview.delegate().notify_new_frame_ready(webview);
        }
    }

    fn send_animating_changed_messages(&self) {
        let animating = self.compositor.borrow().webxr_running() ||
            self.webviews
                .borrow()
                .values()
                .filter_map(WebView::from_weak_handle)
                .any(|webview| webview.animating());
        if animating != self.animating.get() {
            self.animating.set(animating);
            self.delegate().notify_animating_changed(animating);
        }
    }

    fn handle_delegate_errors(&self) {
        while let Some(error) = self.servo_errors.try_recv() {
            self.delegate().notify_error(self, error);
        }
    }

    fn clean_up_destroyed_webview_handles(&self) {
        // Remove any webview handles that have been destroyed and would not be upgradable.
        // Note that `retain` is O(capacity) because it visits empty buckets, so it may be worth
        // calling `shrink_to_fit` at some point to deal with cases where a long-running Servo
        // instance goes from many open webviews to only a few.
        self.webviews
            .borrow_mut()
            .retain(|_webview_id, webview| webview.strong_count() > 0);
    }

    pub fn setup_logging(&self) {
        let constellation_chan = self.constellation_proxy.sender();
        let env = env_logger::Env::default();
        let env_logger = EnvLoggerBuilder::from_env(env).build();
        let con_logger = FromEmbedderLogger::new(constellation_chan);

        let filter = max(env_logger.filter(), con_logger.filter());
        let logger = BothLogger(env_logger, con_logger);

        log::set_boxed_logger(Box::new(logger)).expect("Failed to set logger.");
        log::set_max_level(filter);
    }

    pub fn create_memory_report(&self, snd: IpcSender<MemoryReportResult>) {
        self.constellation_proxy
            .send(EmbedderToConstellationMessage::CreateMemoryReport(snd));
    }

    pub fn start_shutting_down(&self) {
        if self.shutdown_state.get() != ShutdownState::NotShuttingDown {
            warn!("Requested shutdown while already shutting down");
            return;
        }

        debug!("Sending Exit message to Constellation");
        self.constellation_proxy
            .send(EmbedderToConstellationMessage::Exit);
        self.shutdown_state.set(ShutdownState::ShuttingDown);
    }

    fn finish_shutting_down(&self) {
        debug!("Servo received message that Constellation shutdown is complete");
        self.shutdown_state.set(ShutdownState::FinishedShuttingDown);
        self.compositor.borrow_mut().finish_shutting_down();
    }

    pub fn deinit(&self) {
        self.compositor.borrow_mut().deinit();
    }

    fn get_webview_handle(&self, id: WebViewId) -> Option<WebView> {
        self.webviews
            .borrow()
            .get(&id)
            .and_then(WebView::from_weak_handle)
    }

    fn handle_embedder_message(&self, message: EmbedderMsg) {
        match message {
            EmbedderMsg::ShutdownComplete => self.finish_shutting_down(),
            EmbedderMsg::Status(webview_id, status_text) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.set_status_text(status_text);
                }
            },
            EmbedderMsg::ChangePageTitle(webview_id, title) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.set_page_title(title);
                }
            },
            EmbedderMsg::MoveTo(webview_id, position) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().request_move_to(webview, position);
                }
            },
            EmbedderMsg::ResizeTo(webview_id, size) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().request_resize_to(webview, size);
                }
            },
            EmbedderMsg::ShowSimpleDialog(webview_id, prompt_definition) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview
                        .delegate()
                        .show_simple_dialog(webview, prompt_definition);
                }
            },
            EmbedderMsg::ShowContextMenu(webview_id, ipc_sender, title, items) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview
                        .delegate()
                        .show_context_menu(webview, ipc_sender, title, items);
                }
            },
            EmbedderMsg::AllowNavigationRequest(webview_id, pipeline_id, servo_url) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    let request = NavigationRequest {
                        url: servo_url.into_url(),
                        pipeline_id,
                        constellation_proxy: self.constellation_proxy.clone(),
                        response_sent: false,
                    };
                    webview.delegate().request_navigation(webview, request);
                }
            },
            EmbedderMsg::AllowOpeningWebView(webview_id, response_sender) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    let webview_id_and_viewport_details = webview
                        .delegate()
                        .request_open_auxiliary_webview(webview)
                        .map(|webview| (webview.id(), webview.viewport_details()));
                    let _ = response_sender.send(webview_id_and_viewport_details);
                }
            },
            EmbedderMsg::WebViewClosed(webview_id) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().notify_closed(webview);
                }
            },
            EmbedderMsg::WebViewFocused(webview_id, focus_id, focus_result) => {
                if focus_result {
                    for id in self.webviews.borrow().keys() {
                        if let Some(webview) = self.get_webview_handle(*id) {
                            let focused = webview.id() == webview_id;
                            webview.set_focused(focused);
                        }
                    }
                }
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.complete_focus(focus_id);
                }
            },
            EmbedderMsg::WebViewBlurred => {
                for id in self.webviews.borrow().keys() {
                    if let Some(webview) = self.get_webview_handle(*id) {
                        webview.set_focused(false);
                    }
                }
            },
            EmbedderMsg::AllowUnload(webview_id, response_sender) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    let request = AllowOrDenyRequest::new(
                        response_sender,
                        AllowOrDeny::Allow,
                        self.servo_errors.sender(),
                    );
                    webview.delegate().request_unload(webview, request);
                }
            },
            EmbedderMsg::FinishJavaScriptEvaluation(evaluation_id, result) => {
                self.javascript_evaluator
                    .borrow_mut()
                    .finish_evaluation(evaluation_id, result);
            },
            EmbedderMsg::Keyboard(webview_id, keyboard_event) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview
                        .delegate()
                        .notify_keyboard_event(webview, keyboard_event);
                }
            },
            EmbedderMsg::ClearClipboard(webview_id) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.clipboard_delegate().clear(webview);
                }
            },
            EmbedderMsg::GetClipboardText(webview_id, result_sender) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview
                        .clipboard_delegate()
                        .get_text(webview, StringRequest::from(result_sender));
                }
            },
            EmbedderMsg::SetClipboardText(webview_id, string) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.clipboard_delegate().set_text(webview, string);
                }
            },
            EmbedderMsg::SetCursor(webview_id, cursor) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.set_cursor(cursor);
                }
            },
            EmbedderMsg::NewFavicon(webview_id, url) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.set_favicon_url(url.into_url());
                }
            },
            EmbedderMsg::NotifyLoadStatusChanged(webview_id, load_status) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.set_load_status(load_status);
                }
            },
            EmbedderMsg::HistoryTraversalComplete(webview_id, traversal_id) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview
                        .delegate()
                        .notify_traversal_complete(webview.clone(), traversal_id);
                }
            },
            EmbedderMsg::HistoryChanged(webview_id, urls, current_index) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    let urls: Vec<_> = urls.into_iter().map(ServoUrl::into_url).collect();
                    let current_url = urls[current_index].clone();

                    webview
                        .delegate()
                        .notify_history_changed(webview.clone(), urls, current_index);
                    webview.set_url(current_url);
                }
            },
            EmbedderMsg::NotifyFullscreenStateChanged(webview_id, fullscreen) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview
                        .delegate()
                        .notify_fullscreen_state_changed(webview, fullscreen);
                }
            },
            EmbedderMsg::WebResourceRequested(
                webview_id,
                web_resource_request,
                response_sender,
            ) => {
                if let Some(webview) =
                    webview_id.and_then(|webview_id| self.get_webview_handle(webview_id))
                {
                    let web_resource_load = WebResourceLoad::new(
                        web_resource_request,
                        response_sender,
                        self.servo_errors.sender(),
                    );
                    webview
                        .delegate()
                        .load_web_resource(webview, web_resource_load);
                } else {
                    let web_resource_load = WebResourceLoad::new(
                        web_resource_request,
                        response_sender,
                        self.servo_errors.sender(),
                    );
                    self.delegate().load_web_resource(web_resource_load);
                }
            },
            EmbedderMsg::Panic(webview_id, reason, backtrace) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview
                        .delegate()
                        .notify_crashed(webview, reason, backtrace);
                }
            },
            EmbedderMsg::GetSelectedBluetoothDevice(webview_id, items, response_sender) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().show_bluetooth_device_dialog(
                        webview,
                        items,
                        response_sender,
                    );
                }
            },
            EmbedderMsg::SelectFiles(
                webview_id,
                filter_patterns,
                allow_select_multiple,
                response_sender,
            ) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().show_file_selection_dialog(
                        webview,
                        filter_patterns,
                        allow_select_multiple,
                        response_sender,
                    );
                }
            },
            EmbedderMsg::RequestAuthentication(webview_id, url, for_proxy, response_sender) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    let authentication_request = AuthenticationRequest::new(
                        url.into_url(),
                        for_proxy,
                        response_sender,
                        self.servo_errors.sender(),
                    );
                    webview
                        .delegate()
                        .request_authentication(webview, authentication_request);
                }
            },
            EmbedderMsg::PromptPermission(webview_id, requested_feature, response_sender) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    let permission_request = PermissionRequest {
                        requested_feature,
                        allow_deny_request: AllowOrDenyRequest::new(
                            response_sender,
                            AllowOrDeny::Deny,
                            self.servo_errors.sender(),
                        ),
                    };
                    webview
                        .delegate()
                        .request_permission(webview, permission_request);
                }
            },
            EmbedderMsg::ShowIME(webview_id, input_method_type, text, multiline, position) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().show_ime(
                        webview,
                        input_method_type,
                        text,
                        multiline,
                        position,
                    );
                }
            },
            EmbedderMsg::HideIME(webview_id) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().hide_ime(webview);
                }
            },
            EmbedderMsg::ReportProfile(_items) => {},
            EmbedderMsg::MediaSessionEvent(webview_id, media_session_event) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview
                        .delegate()
                        .notify_media_session_event(webview, media_session_event);
                }
            },
            EmbedderMsg::OnDevtoolsStarted(port, token) => match port {
                Ok(port) => self
                    .delegate()
                    .notify_devtools_server_started(self, port, token),
                Err(()) => self
                    .delegate()
                    .notify_error(self, ServoError::DevtoolsFailedToStart),
            },
            EmbedderMsg::RequestDevtoolsConnection(response_sender) => {
                self.delegate().request_devtools_connection(
                    self,
                    AllowOrDenyRequest::new(
                        response_sender,
                        AllowOrDeny::Deny,
                        self.servo_errors.sender(),
                    ),
                );
            },
            EmbedderMsg::PlayGamepadHapticEffect(
                webview_id,
                gamepad_index,
                gamepad_haptic_effect_type,
                ipc_sender,
            ) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().play_gamepad_haptic_effect(
                        webview,
                        gamepad_index,
                        gamepad_haptic_effect_type,
                        ipc_sender,
                    );
                }
            },
            EmbedderMsg::StopGamepadHapticEffect(webview_id, gamepad_index, ipc_sender) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    webview.delegate().stop_gamepad_haptic_effect(
                        webview,
                        gamepad_index,
                        ipc_sender,
                    );
                }
            },
            EmbedderMsg::ShowNotification(webview_id, notification) => {
                match webview_id.and_then(|webview_id| self.get_webview_handle(webview_id)) {
                    Some(webview) => webview.delegate().show_notification(webview, notification),
                    None => self.delegate().show_notification(notification),
                }
            },
            EmbedderMsg::ShowFormControl(webview_id, position, form_control) => {
                if let Some(webview) = self.get_webview_handle(webview_id) {
                    let form_control = match form_control {
                        EmbedderFormControl::SelectElement(
                            options,
                            selected_option,
                            ipc_sender,
                        ) => FormControl::SelectElement(SelectElement::new(
                            options,
                            selected_option,
                            position,
                            ipc_sender,
                        )),
                        EmbedderFormControl::ColorPicker(current_color, ipc_sender) => {
                            FormControl::ColorPicker(ColorPicker::new(
                                current_color,
                                position,
                                ipc_sender,
                                self.servo_errors.sender(),
                            ))
                        },
                    };

                    webview.delegate().show_form_control(webview, form_control);
                }
            },
            EmbedderMsg::GetWindowRect(webview_id, response_sender) => {
                let window_rect = || {
                    let Some(webview) = self.get_webview_handle(webview_id) else {
                        return DeviceIndependentIntRect::default();
                    };
                    let hidpi_scale_factor = webview.hidpi_scale_factor();
                    let Some(screen_geometry) = webview.delegate().screen_geometry(webview) else {
                        return DeviceIndependentIntRect::default();
                    };

                    convert_rect_to_css_pixel(screen_geometry.window_rect, hidpi_scale_factor)
                };

                if let Err(error) = response_sender.send(window_rect()) {
                    warn!("Failed to respond to GetWindowRect: {error}");
                }
            },
            EmbedderMsg::GetScreenMetrics(webview_id, response_sender) => {
                let screen_metrics = || {
                    let Some(webview) = self.get_webview_handle(webview_id) else {
                        return ScreenMetrics::default();
                    };
                    let hidpi_scale_factor = webview.hidpi_scale_factor();
                    let Some(screen_geometry) = webview.delegate().screen_geometry(webview) else {
                        return ScreenMetrics::default();
                    };

                    ScreenMetrics {
                        screen_size: convert_size_to_css_pixel(
                            screen_geometry.size,
                            hidpi_scale_factor,
                        ),
                        available_size: convert_size_to_css_pixel(
                            screen_geometry.available_size,
                            hidpi_scale_factor,
                        ),
                    }
                };
                if let Err(error) = response_sender.send(screen_metrics()) {
                    warn!("Failed to respond to GetScreenMetrics: {error}");
                }
            },
        }
    }

    pub fn constellation_sender(&self) -> Sender<EmbedderToConstellationMessage> {
        self.constellation_proxy.sender()
    }

    pub fn execute_webdriver_command(&self, command: WebDriverCommandMsg) {
        if let WebDriverCommandMsg::TakeScreenshot(webview_id, page_rect, response_sender) = command
        {
            let res = self
                .compositor
                .borrow_mut()
                .render_to_shared_memory(webview_id, page_rect);
            if let Err(ref e) = res {
                error!("Error retrieving PNG: {:?}", e);
            }
            let img = res.unwrap_or(None);
            if let Err(e) = response_sender.send(img) {
                error!("Sending reply to create png failed ({:?}).", e);
            }
        } else {
            self.constellation_proxy
                .send(EmbedderToConstellationMessage::WebDriverCommand(command));
        }
    }
}

fn create_embedder_channel(
    event_loop_waker: Box<dyn EventLoopWaker>,
) -> (EmbedderProxy, Receiver<EmbedderMsg>) {
    let (sender, receiver) = unbounded();
    (
        EmbedderProxy {
            sender,
            event_loop_waker,
        },
        receiver,
    )
}

fn create_compositor_channel(
    event_loop_waker: Box<dyn EventLoopWaker>,
) -> (CompositorProxy, Receiver<CompositorMsg>) {
    let (sender, receiver) = unbounded();

    let (compositor_ipc_sender, compositor_ipc_receiver) =
        ipc::channel().expect("ipc channel failure");

    let cross_process_compositor_api = CrossProcessCompositorApi(compositor_ipc_sender);
    let compositor_proxy = CompositorProxy {
        sender,
        cross_process_compositor_api,
        event_loop_waker,
    };

    let compositor_proxy_clone = compositor_proxy.clone();
    ROUTER.add_typed_route(
        compositor_ipc_receiver,
        Box::new(move |message| {
            compositor_proxy_clone.send(message.expect("Could not convert Compositor message"));
        }),
    );

    (compositor_proxy, receiver)
}

#[allow(clippy::too_many_arguments)]
fn create_constellation(
    config_dir: Option<PathBuf>,
    embedder_proxy: EmbedderProxy,
    compositor_proxy: CompositorProxy,
    time_profiler_chan: time::ProfilerChan,
    mem_profiler_chan: mem::ProfilerChan,
    devtools_sender: Option<Sender<devtools_traits::DevtoolsControlMsg>>,
    webrender_document: DocumentId,
    webrender_api_sender: RenderApiSender,
    #[cfg(feature = "webxr")] webxr_registry: webxr_api::Registry,
    webgl_threads: Option<WebGLThreads>,
    external_images: Arc<Mutex<WebrenderExternalImageRegistry>>,
    #[cfg(feature = "webgpu")] wgpu_image_map: WGPUImageMap,
    protocols: ProtocolRegistry,
    user_content_manager: UserContentManager,
) -> Sender<EmbedderToConstellationMessage> {
    // Global configuration options, parsed from the command line.
    let opts = opts::get();

    #[cfg(feature = "bluetooth")]
    let bluetooth_thread: IpcSender<BluetoothRequest> =
        BluetoothThreadFactory::new(embedder_proxy.clone());

    let (public_resource_threads, private_resource_threads) = new_resource_threads(
        devtools_sender.clone(),
        time_profiler_chan.clone(),
        mem_profiler_chan.clone(),
        embedder_proxy.clone(),
        config_dir,
        opts.certificate_path.clone(),
        opts.ignore_certificate_errors,
        Arc::new(protocols),
    );

    let system_font_service = Arc::new(
        SystemFontService::spawn(
            compositor_proxy.cross_process_compositor_api.clone(),
            mem_profiler_chan.clone(),
        )
        .to_proxy(),
    );

    let initial_state = InitialConstellationState {
        compositor_proxy,
        embedder_proxy,
        devtools_sender,
        #[cfg(feature = "bluetooth")]
        bluetooth_thread,
        system_font_service,
        public_resource_threads,
        private_resource_threads,
        time_profiler_chan,
        mem_profiler_chan,
        webrender_document,
        webrender_api_sender,
        #[cfg(feature = "webxr")]
        webxr_registry: Some(webxr_registry),
        #[cfg(not(feature = "webxr"))]
        webxr_registry: None,
        webgl_threads,
        webrender_external_images: external_images,
        #[cfg(feature = "webgpu")]
        wgpu_image_map,
        user_content_manager,
    };

    let layout_factory = Arc::new(LayoutFactoryImpl());

    Constellation::<script::ScriptThread, script::ServiceWorkerManager>::start(
        initial_state,
        layout_factory,
        opts.random_pipeline_closure_probability,
        opts.random_pipeline_closure_seed,
        opts.hard_fail,
    )
}

// A logger that logs to two downstream loggers.
// This should probably be in the log crate.
struct BothLogger<Log1, Log2>(Log1, Log2);

impl<Log1, Log2> Log for BothLogger<Log1, Log2>
where
    Log1: Log,
    Log2: Log,
{
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.0.enabled(metadata) || self.1.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        self.0.log(record);
        self.1.log(record);
    }

    fn flush(&self) {
        self.0.flush();
        self.1.flush();
    }
}

pub fn set_logger(script_to_constellation_chan: ScriptToConstellationChan) {
    let con_logger = FromScriptLogger::new(script_to_constellation_chan);
    let env = env_logger::Env::default();
    let env_logger = EnvLoggerBuilder::from_env(env).build();

    let filter = max(env_logger.filter(), con_logger.filter());
    let logger = BothLogger(env_logger, con_logger);

    log::set_boxed_logger(Box::new(logger)).expect("Failed to set logger.");
    log::set_max_level(filter);
}

/// Content process entry point.
pub fn run_content_process(token: String) {
    let (unprivileged_content_sender, unprivileged_content_receiver) =
        ipc::channel::<UnprivilegedContent>().unwrap();
    let connection_bootstrap: IpcSender<IpcSender<UnprivilegedContent>> =
        IpcSender::connect(token).unwrap();
    connection_bootstrap
        .send(unprivileged_content_sender)
        .unwrap();

    let unprivileged_content = unprivileged_content_receiver.recv().unwrap();
    opts::set_options(unprivileged_content.opts());
    prefs::set(unprivileged_content.prefs().clone());

    // Enter the sandbox if necessary.
    if opts::get().sandbox {
        create_sandbox();
    }

    let _js_engine_setup = script::init();

    match unprivileged_content {
        UnprivilegedContent::Pipeline(mut content) => {
            media_platform::init();

            set_logger(content.script_to_constellation_chan().clone());

            let (background_hang_monitor_register, join_handle) =
                content.register_with_background_hang_monitor();
            let layout_factory = Arc::new(LayoutFactoryImpl());

            content.register_system_memory_reporter();

            content.start_all::<script::ScriptThread>(
                true,
                layout_factory,
                background_hang_monitor_register,
            );

            // Since wait_for_completion is true,
            // here we know that the script-thread
            // will exit(or already has),
            // and so we can join on the BHM worker thread.
            join_handle
                .join()
                .expect("Failed to join on the BHM background thread.");
        },
        UnprivilegedContent::ServiceWorker(content) => {
            content.start::<ServiceWorkerManager>();
        },
    }
}

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "ios"),
    not(target_os = "android"),
    not(target_arch = "arm"),
    not(target_arch = "aarch64"),
    not(target_env = "ohos"),
))]
fn create_sandbox() {
    ChildSandbox::new(content_process_sandbox_profile())
        .activate()
        .expect("Failed to activate sandbox!");
}

#[cfg(any(
    target_os = "windows",
    target_os = "ios",
    target_os = "android",
    target_arch = "arm",
    target_arch = "aarch64",
    target_env = "ohos",
))]
fn create_sandbox() {
    panic!("Sandboxing is not supported on Windows, iOS, ARM targets and android.");
}

struct DefaultEventLoopWaker;

impl EventLoopWaker for DefaultEventLoopWaker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(DefaultEventLoopWaker)
    }
}

#[cfg(feature = "webxr")]
struct DefaultWebXrRegistry;
#[cfg(feature = "webxr")]
impl webxr::WebXrRegistry for DefaultWebXrRegistry {}

pub struct ServoBuilder {
    rendering_context: Rc<dyn RenderingContext>,
    opts: Option<Box<Opts>>,
    preferences: Option<Box<Preferences>>,
    event_loop_waker: Box<dyn EventLoopWaker>,
    user_content_manager: UserContentManager,
    protocol_registry: ProtocolRegistry,
    #[cfg(feature = "webxr")]
    webxr_registry: Box<dyn webxr::WebXrRegistry>,
}

impl ServoBuilder {
    pub fn new(rendering_context: Rc<dyn RenderingContext>) -> Self {
        Self {
            rendering_context,
            opts: None,
            preferences: None,
            event_loop_waker: Box::new(DefaultEventLoopWaker),
            user_content_manager: UserContentManager::default(),
            protocol_registry: ProtocolRegistry::default(),
            #[cfg(feature = "webxr")]
            webxr_registry: Box::new(DefaultWebXrRegistry),
        }
    }

    pub fn build(self) -> Servo {
        Servo::new(self)
    }

    pub fn opts(mut self, opts: Opts) -> Self {
        self.opts = Some(Box::new(opts));
        self
    }

    pub fn preferences(mut self, preferences: Preferences) -> Self {
        self.preferences = Some(Box::new(preferences));
        self
    }

    pub fn event_loop_waker(mut self, event_loop_waker: Box<dyn EventLoopWaker>) -> Self {
        self.event_loop_waker = event_loop_waker;
        self
    }

    pub fn user_content_manager(mut self, user_content_manager: UserContentManager) -> Self {
        self.user_content_manager = user_content_manager;
        self
    }

    pub fn protocol_registry(mut self, protocol_registry: ProtocolRegistry) -> Self {
        self.protocol_registry = protocol_registry;
        self
    }

    #[cfg(feature = "webxr")]
    pub fn webxr_registry(mut self, webxr_registry: Box<dyn webxr::WebXrRegistry>) -> Self {
        self.webxr_registry = webxr_registry;
        self
    }
}
