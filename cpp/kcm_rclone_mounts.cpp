// SPDX-License-Identifier: GPL-2.0-or-later

#include "kcm_rclone_mounts.h"

#include <KPluginFactory>

K_PLUGIN_CLASS_WITH_JSON(KCMRcloneMounts, "metadata.json")

KCMRcloneMounts::KCMRcloneMounts(QObject *parent, const KPluginMetaData &data)
    : KQuickConfigModule(parent, data)
{
    // The QML module is registered by cxx-qt-build at compile time under URI
    // "dev.jthoward.RcloneMounts"; the QML root is loaded by KQuickConfigModule
    // via its mainUi() resolution against the QML import path.
}

void KCMRcloneMounts::load()
{
    KQuickConfigModule::load();
    // TODO: forward to BackendController::load() once we have a handle on the root
    // Rust QObject. cxx-qt registers it as a QML element; we'll either fetch it from
    // the QML root or hold our own instance and expose it as a context property.
}

void KCMRcloneMounts::save()
{
    KQuickConfigModule::save();
    // TODO: forward to BackendController::commit()
}

void KCMRcloneMounts::defaults()
{
    KQuickConfigModule::defaults();
    // TODO: forward to BackendController::reset()
}

#include "kcm_rclone_mounts.moc"
