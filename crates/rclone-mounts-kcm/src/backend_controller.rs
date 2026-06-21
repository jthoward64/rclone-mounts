// SPDX-License-Identifier: GPL-2.0-or-later

// BackendController is the root QObject exposed to QML and held by the C++ KCM shim.
// The C++ shim owns Apply/Cancel/Defaults lifecycle; on those events it invokes load,
// commit, or reset on this object. Rust tracks pending vs applied state and emits
// dirty_changed when they diverge — the shim forwards to setNeedsSave().

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, dirty, READ, NOTIFY = dirty_changed)]
        type BackendController = super::BackendControllerRust;

        #[qinvokable]
        fn load(self: Pin<&mut BackendController>);

        #[qinvokable]
        fn commit(self: Pin<&mut BackendController>);

        #[qinvokable]
        fn reset(self: Pin<&mut BackendController>);

        #[qsignal]
        fn dirty_changed(self: Pin<&mut BackendController>);
    }
}

use core::pin::Pin;

#[derive(Default)]
pub struct BackendControllerRust {
    dirty: bool,
}

impl ffi::BackendController {
    fn load(self: Pin<&mut Self>) {
        tracing::info!("BackendController::load (stub)");
    }

    fn commit(self: Pin<&mut Self>) {
        tracing::info!("BackendController::commit (stub)");
    }

    fn reset(self: Pin<&mut Self>) {
        tracing::info!("BackendController::reset (stub)");
    }
}
