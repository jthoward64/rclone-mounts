// SPDX-License-Identifier: GPL-2.0-or-later

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("dev.jthoward.RcloneMounts").qml_files(["qml/Main.qml"]),
    )
    .file("src/backend_controller.rs")
    .build();
}
