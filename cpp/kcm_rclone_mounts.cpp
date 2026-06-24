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
    Q_EMIT loadRequested();
}

void KCMRcloneMounts::save()
{
    KQuickConfigModule::save();
    Q_EMIT saveRequested();
}

void KCMRcloneMounts::defaults()
{
    KQuickConfigModule::defaults();
    Q_EMIT defaultsRequested();
}

#include "kcm_rclone_mounts.moc"
