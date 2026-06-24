// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <KQuickConfigModule>

// Deliberately thin. The KCM framework drives this object's load/save/defaults;
// we forward each as a Qt signal and let the QML/Rust side (BackendController)
// do the actual work. This keeps all mount/source logic out of C++ — the C++
// here knows nothing beyond "the user pressed Apply/Reset". QML connects to
// these via the `kcm` context object, and binds kcm.needsSave to the
// controller's dirty property.
class KCMRcloneMounts : public KQuickConfigModule {
    Q_OBJECT

public:
    KCMRcloneMounts(QObject *parent, const KPluginMetaData &data);
    ~KCMRcloneMounts() override = default;

    void load() override;
    void save() override;
    void defaults() override;

Q_SIGNALS:
    void loadRequested();
    void saveRequested();
    void defaultsRequested();
};
