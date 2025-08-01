/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Namespaces and ids shared by many crates in Servo.

#![allow(clippy::new_without_default)]

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};

use ipc_channel::ipc::{self, IpcReceiver, IpcSender};
use malloc_size_of::MallocSizeOfOps;
use malloc_size_of_derive::MallocSizeOf;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use webrender_api::{ExternalScrollId, PipelineId as WebRenderPipelineId};

/// Asserts the size of a type at compile time.
macro_rules! size_of_test {
    ($t: ty, $expected_size: expr) => {
        #[cfg(target_pointer_width = "64")]
        ::static_assertions::const_assert_eq!(std::mem::size_of::<$t>(), $expected_size);
    };
}

/// A type that implements this trait is expected to be used as part of
/// the [NamespaceIndex] type.
pub trait Indexable {
    /// The string prefix to display when debug printing an instance of
    /// this type.
    const DISPLAY_PREFIX: &'static str;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
/// A non-zero index, associated with a particular type.
pub struct Index<T>(pub NonZeroU32, pub PhantomData<T>);

#[derive(Debug)]
/// An attempt to create a new [Index] value failed because the index value
/// was zero.
pub struct ZeroIndex;

impl<T> Index<T> {
    /// Creates a new instance of [Index] with the given value.
    /// Returns an error if the value is zero.
    pub fn new(value: u32) -> Result<Index<T>, ZeroIndex> {
        Ok(Index(NonZeroU32::new(value).ok_or(ZeroIndex)?, PhantomData))
    }
}

impl<T> malloc_size_of::MallocSizeOf for Index<T> {
    fn size_of(&self, _ops: &mut MallocSizeOfOps) -> usize {
        0
    }
}

#[derive(
    Clone, Copy, Deserialize, Eq, Hash, MallocSizeOf, Ord, PartialEq, PartialOrd, Serialize,
)]
/// A pipeline-namespaced index associated with a particular type.
pub struct NamespaceIndex<T> {
    pub namespace_id: PipelineNamespaceId,
    pub index: Index<T>,
}

impl<T> fmt::Debug for NamespaceIndex<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let PipelineNamespaceId(namespace_id) = self.namespace_id;
        let Index(index, _) = self.index;
        write!(fmt, "({},{})", namespace_id, index.get())
    }
}

impl<T: Indexable> fmt::Display for NamespaceIndex<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}{:?}", T::DISPLAY_PREFIX, self)
    }
}

macro_rules! namespace_id {
    ($id_name:ident, $index_name:ident, $display_prefix:literal) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
            MallocSizeOf,
        )]
        pub struct $index_name;
        impl Indexable for $index_name {
            const DISPLAY_PREFIX: &'static str = $display_prefix;
        }
        pub type $id_name = NamespaceIndex<$index_name>;
        impl $id_name {
            pub fn new() -> $id_name {
                PIPELINE_NAMESPACE.with(|tls| {
                    let mut namespace = tls.get().expect("No namespace set for this thread!");
                    let next_id = namespace.next_namespace_index();
                    tls.set(Some(namespace));
                    next_id
                })
            }
        }
    };
}

#[derive(Debug, Deserialize, Serialize)]
/// Request a pipeline-namespace id from the constellation.
pub struct PipelineNamespaceRequest(pub IpcSender<PipelineNamespaceId>);

/// A per-process installer of pipeline-namespaces.
pub struct PipelineNamespaceInstaller {
    request_sender: Option<IpcSender<PipelineNamespaceRequest>>,
    namespace_sender: IpcSender<PipelineNamespaceId>,
    namespace_receiver: IpcReceiver<PipelineNamespaceId>,
}

impl Default for PipelineNamespaceInstaller {
    fn default() -> Self {
        let (namespace_sender, namespace_receiver) =
            ipc::channel().expect("PipelineNamespaceInstaller ipc channel failure");
        Self {
            request_sender: None,
            namespace_sender,
            namespace_receiver,
        }
    }
}

impl PipelineNamespaceInstaller {
    /// Provide a request sender to send requests to the constellation.
    pub fn set_sender(&mut self, sender: IpcSender<PipelineNamespaceRequest>) {
        self.request_sender = Some(sender);
    }

    /// Install a namespace, requesting a new Id from the constellation.
    pub fn install_namespace(&self) {
        match self.request_sender.as_ref() {
            Some(sender) => {
                let _ = sender.send(PipelineNamespaceRequest(self.namespace_sender.clone()));
                let namespace_id = self
                    .namespace_receiver
                    .recv()
                    .expect("The constellation to make a pipeline namespace id available");
                PipelineNamespace::install(namespace_id);
            },
            None => unreachable!("PipelineNamespaceInstaller should have a request_sender setup"),
        }
    }
}

/// A per-process unique pipeline-namespace-installer.
/// Accessible via PipelineNamespace.
///
/// Use PipelineNamespace::set_installer_sender to initiate with a sender to the constellation,
/// when a new process has been created.
///
/// Use PipelineNamespace::fetch_install to install a unique pipeline-namespace from the calling thread.
static PIPELINE_NAMESPACE_INSTALLER: LazyLock<Arc<Mutex<PipelineNamespaceInstaller>>> =
    LazyLock::new(|| Arc::new(Mutex::new(PipelineNamespaceInstaller::default())));

/// Each pipeline ID needs to be unique. However, it also needs to be possible to
/// generate the pipeline ID from an iframe element (this simplifies a lot of other
/// code that makes use of pipeline IDs).
///
/// To achieve this, each pipeline index belongs to a particular namespace. There is
/// a namespace for the constellation thread, and also one for every script thread.
///
/// A namespace can be installed for any other thread in a process
/// where an pipeline-installer has been initialized.
///
/// This allows pipeline IDs to be generated by any of those threads without conflicting
/// with pipeline IDs created by other script threads or the constellation. The
/// constellation is the only code that is responsible for creating new *namespaces*.
/// This ensures that namespaces are always unique, even when using multi-process mode.
///
/// It may help conceptually to think of the namespace ID as an identifier for the
/// thread that created this pipeline ID - however this is really an implementation
/// detail so shouldn't be relied upon in code logic. It's best to think of the
/// pipeline ID as a simple unique identifier that doesn't convey any more information.
#[derive(Clone, Copy)]
pub struct PipelineNamespace {
    id: PipelineNamespaceId,
    index: u32,
}

impl PipelineNamespace {
    /// Install a namespace for a given Id.
    pub fn install(namespace_id: PipelineNamespaceId) {
        PIPELINE_NAMESPACE.with(|tls| {
            assert!(tls.get().is_none());
            tls.set(Some(PipelineNamespace {
                id: namespace_id,
                index: 0,
            }));
        });
    }

    /// Setup the pipeline-namespace-installer, by providing it with a sender to the constellation.
    /// Idempotent in single-process mode.
    pub fn set_installer_sender(sender: IpcSender<PipelineNamespaceRequest>) {
        PIPELINE_NAMESPACE_INSTALLER.lock().set_sender(sender);
    }

    /// Install a namespace in the current thread, without requiring having a namespace Id ready.
    /// Panics if called more than once per thread.
    pub fn auto_install() {
        // Note that holding the lock for the duration of the call is irrelevant to performance,
        // since a thread would have to block on the ipc-response from the constellation,
        // with the constellation already acting as a global lock on namespace ids,
        // and only being able to handle one request at a time.
        //
        // Hence, any other thread attempting to concurrently install a namespace
        // would have to wait for the current call to finish, regardless of the lock held here.
        PIPELINE_NAMESPACE_INSTALLER.lock().install_namespace();
    }

    fn next_index(&mut self) -> NonZeroU32 {
        self.index += 1;
        NonZeroU32::new(self.index).expect("pipeline id index wrapped!")
    }

    fn next_namespace_index<T>(&mut self) -> NamespaceIndex<T> {
        NamespaceIndex {
            namespace_id: self.id,
            index: Index(self.next_index(), PhantomData),
        }
    }
}

thread_local!(pub static PIPELINE_NAMESPACE: Cell<Option<PipelineNamespace>> = const { Cell::new(None) });

#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, MallocSizeOf, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct PipelineNamespaceId(pub u32);

namespace_id! {PipelineId, PipelineIndex, "Pipeline"}

size_of_test!(PipelineId, 8);
size_of_test!(Option<PipelineId>, 8);

impl PipelineId {
    pub fn root_scroll_id(&self) -> webrender_api::ExternalScrollId {
        ExternalScrollId(0, self.into())
    }
}

impl From<WebRenderPipelineId> for PipelineId {
    #[allow(unsafe_code)]
    fn from(pipeline: WebRenderPipelineId) -> Self {
        let WebRenderPipelineId(namespace_id, index) = pipeline;
        unsafe {
            PipelineId {
                namespace_id: PipelineNamespaceId(namespace_id),
                index: Index(NonZeroU32::new_unchecked(index), PhantomData),
            }
        }
    }
}

impl From<PipelineId> for WebRenderPipelineId {
    fn from(value: PipelineId) -> Self {
        let PipelineNamespaceId(namespace_id) = value.namespace_id;
        let Index(index, _) = value.index;
        WebRenderPipelineId(namespace_id, index.get())
    }
}

impl From<&PipelineId> for WebRenderPipelineId {
    fn from(value: &PipelineId) -> Self {
        (*value).into()
    }
}

namespace_id! {BrowsingContextId, BrowsingContextIndex, "BrowsingContext"}

size_of_test!(BrowsingContextId, 8);
size_of_test!(Option<BrowsingContextId>, 8);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct BrowsingContextGroupId(pub u32);
impl fmt::Display for BrowsingContextGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BrowsingContextGroup{:?}", self)
    }
}

thread_local!(pub static WEBVIEW_ID: Cell<Option<WebViewId>> =
    const { Cell::new(None) });

#[derive(
    Clone, Copy, Deserialize, Eq, Hash, MallocSizeOf, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct WebViewId(pub BrowsingContextId);

size_of_test!(WebViewId, 8);
size_of_test!(Option<WebViewId>, 8);

impl fmt::Debug for WebViewId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TopLevel{:?}", self.0)
    }
}

impl fmt::Display for WebViewId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TopLevel{}", self.0)
    }
}

impl WebViewId {
    pub fn new() -> WebViewId {
        WebViewId(BrowsingContextId::new())
    }

    /// Each script and layout thread should have the top-level browsing context id installed,
    /// since it is used by crash reporting.
    pub fn install(id: WebViewId) {
        WEBVIEW_ID.with(|tls| tls.set(Some(id)))
    }

    pub fn installed() -> Option<WebViewId> {
        WEBVIEW_ID.with(|tls| tls.get())
    }
}

impl From<WebViewId> for BrowsingContextId {
    fn from(id: WebViewId) -> BrowsingContextId {
        id.0
    }
}

impl PartialEq<WebViewId> for BrowsingContextId {
    fn eq(&self, rhs: &WebViewId) -> bool {
        self.eq(&rhs.0)
    }
}

impl PartialEq<BrowsingContextId> for WebViewId {
    fn eq(&self, rhs: &BrowsingContextId) -> bool {
        self.0.eq(rhs)
    }
}

namespace_id! {MessagePortId, MessagePortIndex, "MessagePort"}

namespace_id! {MessagePortRouterId, MessagePortRouterIndex, "MessagePortRouter"}

namespace_id! {BroadcastChannelRouterId, BroadcastChannelRouterIndex, "BroadcastChannelRouter"}

namespace_id! {ServiceWorkerId, ServiceWorkerIndex, "ServiceWorker"}

namespace_id! {ServiceWorkerRegistrationId, ServiceWorkerRegistrationIndex, "ServiceWorkerRegistration"}

namespace_id! {BlobId, BlobIndex, "Blob"}

namespace_id! {DomPointId, DomPointIndex, "DomPoint"}

namespace_id! {DomExceptionId, DomExceptionIndex, "DomException"}

namespace_id! {HistoryStateId, HistoryStateIndex, "HistoryState"}

namespace_id! {ImageBitmapId, ImageBitmapIndex, "ImageBitmap"}

namespace_id! {OffscreenCanvasId, OffscreenCanvasIndex, "OffscreenCanvas"}

// We provide ids just for unit testing.
pub const TEST_NAMESPACE: PipelineNamespaceId = PipelineNamespaceId(1234);
#[allow(unsafe_code)]
pub const TEST_PIPELINE_INDEX: Index<PipelineIndex> =
    unsafe { Index(NonZeroU32::new_unchecked(5678), PhantomData) };
pub const TEST_PIPELINE_ID: PipelineId = PipelineId {
    namespace_id: TEST_NAMESPACE,
    index: TEST_PIPELINE_INDEX,
};
#[allow(unsafe_code)]
pub const TEST_BROWSING_CONTEXT_INDEX: Index<BrowsingContextIndex> =
    unsafe { Index(NonZeroU32::new_unchecked(8765), PhantomData) };
pub const TEST_BROWSING_CONTEXT_ID: BrowsingContextId = BrowsingContextId {
    namespace_id: TEST_NAMESPACE,
    index: TEST_BROWSING_CONTEXT_INDEX,
};

pub const TEST_WEBVIEW_ID: WebViewId = WebViewId(TEST_BROWSING_CONTEXT_ID);

/// An id for a ScrollTreeNode in the ScrollTree. This contains both the index
/// to the node in the tree's array of nodes as well as the corresponding SpatialId
/// for the SpatialNode in the WebRender display list.
#[derive(Clone, Copy, Debug, Default, Deserialize, MallocSizeOf, PartialEq, Serialize)]
pub struct ScrollTreeNodeId {
    /// The index of this scroll tree node in the tree's array of nodes.
    pub index: usize,
}
