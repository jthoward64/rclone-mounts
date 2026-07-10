// SPDX-License-Identifier: GPL-2.0-or-later

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("dev.jthoward.RcloneMounts").qml_files([
            "qml/Main.qml",
            "qml/Helpers.qml",
            "qml/MainListPage.qml",
            "qml/SourceSectionHeader.qml",
            "qml/MountListItem.qml",
            "qml/SourceEditorForm.qml",
            "qml/SourceKindFieldsRepeater.qml",
            "qml/SourceEditorPage.qml",
            "qml/SourceAddWizardDialog.qml",
            "qml/MountEditorForm.qml",
            "qml/MountEditorPage.qml",
            "qml/CredentialsPage.qml",
            "qml/CredentialEditDialog.qml",
            "qml/SourceWizard.qml",
        ]),
    )
    .file("src/backend_controller.rs")
    .build();
}
