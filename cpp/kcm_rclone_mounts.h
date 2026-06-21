// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <KQuickConfigModule>

class KCMRcloneMounts : public KQuickConfigModule {
    Q_OBJECT

public:
    KCMRcloneMounts(QObject *parent, const KPluginMetaData &data);
    ~KCMRcloneMounts() override = default;

    void load() override;
    void save() override;
    void defaults() override;
};
