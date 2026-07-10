# SPDX-License-Identifier: GPL-2.0-or-later

cmake_minimum_required(VERSION 3.25)

project(rclone-mounts VERSION 0.1.0 LANGUAGES CXX)

set(KF_MIN_VERSION "6.0.0")
set(QT_MIN_VERSION "6.6.0")

find_package(ECM ${KF_MIN_VERSION} REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH})

include(KDEInstallDirs)
include(KDECMakeSettings)
include(KDECompilerSettings NO_POLICY_SCOPE)
include(ECMQmlModule)
include(FetchContent)

find_package(Qt6 ${QT_MIN_VERSION} REQUIRED COMPONENTS
    Core
    Quick
    Qml
    QuickControls2
    Widgets
    Gui
    Network
    DBus
    QmlMeta
    QmlModels
    QmlWorkerScript
)
find_package(KF6 ${KF_MIN_VERSION} REQUIRED COMPONENTS
    CoreAddons
    I18n
    KCMUtils
)

# Use system corrosion (Arch package). Falls back to FetchContent if not present.
find_package(Corrosion QUIET)
if(NOT Corrosion_FOUND)
    FetchContent_Declare(
        Corrosion
        GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
        GIT_TAG v0.5
    )
    FetchContent_MakeAvailable(Corrosion)
endif()

corrosion_import_crate(
    MANIFEST_PATH Cargo.toml
    CRATES rclone-mounts-kcm rclone-mounts-helper
)

# Tell cxx-qt-build to export object files (notably crate initializers from
# upstream crates like cxx-kde-frameworks) so the C++ shim can link them.
set(CXX_QT_EXPORT_DIR "${CMAKE_BINARY_DIR}/cxxqt-export")
file(MAKE_DIRECTORY "${CXX_QT_EXPORT_DIR}")
corrosion_set_env_vars(rclone_mounts_kcm
    "CXX_QT_EXPORT_DIR=${CXX_QT_EXPORT_DIR}"
    "CXX_QT_EXPORT_CRATE_rclone-mounts-kcm=1"
    "CXX_QT_EXPORT_CRATE_cxx-kde-frameworks=1"
    "CXX_QT_EXPORT_CRATE_cxx-qt=1"
    "CXX_QT_EXPORT_CRATE_cxx-qt-lib=1"
    "CXX_QT_EXPORT_CRATE_cxx-qt-lib-extras=1"
)

# The KCM plugin: C++ shim links against the Rust staticlib produced by corrosion.
kcoreaddons_add_plugin(kcm_rclone_mounts
    SOURCES cpp/kcm_rclone_mounts.cpp cpp/kcm_rclone_mounts.qrc
    INSTALL_NAMESPACE "plasma/kcms"
)

# The Rust staticlib's own Qt usage (cxx-qt-lib's bindings, compiled by
# cxx-qt-build/corrosion, not visible to CMake) is invisible to the linker's
# dependency graph until the archive is scanned, which happens last. Under
# --as-needed (the modern default nearly everywhere, including Arch's
# packaging LDFLAGS), the linker drops each Qt/KF6 .so's exports before
# reaching the archive that needs them, since nothing appeared to need them
# yet. --no-as-needed keeps every library's exports available regardless of
# scan order.
target_link_options(kcm_rclone_mounts PRIVATE "LINKER:--no-as-needed")

target_link_libraries(kcm_rclone_mounts PRIVATE
    Qt6::Core
    Qt6::Quick
    Qt6::Qml
    Qt6::QuickControls2
    Qt6::Widgets
    Qt6::Gui
    Qt6::Network
    Qt6::DBus
    Qt6::QmlMeta
    Qt6::QmlModels
    Qt6::QmlWorkerScript
    KF6::CoreAddons
    KF6::I18n
    KF6::KCMUtils
    # The Rust staticlib provides the actual cxx_qt_init_crate_* function
    # bodies; the initializers.o files added via target_sources are the
    # call sites that fire at library load.
    rclone_mounts_kcm
)

# cxx-qt-build emits per-crate initializers.o files that register Qt types, QML
# modules, and chained call_once dispatchers. They're produced when CXX_QT_EXPORT
# is enabled (see corrosion_set_env_vars above). The .o files have no external
# references, so we link them as plain objects rather than going through the
# staticlib (which would drop them under default linker semantics).
set(_cxxqt_initializers
    "${CXX_QT_EXPORT_DIR}/crates/cxx-qt/initializers.o"
    "${CXX_QT_EXPORT_DIR}/crates/cxx-qt-lib/initializers.o"
    "${CXX_QT_EXPORT_DIR}/crates/cxx-qt-lib-extras/initializers.o"
    "${CXX_QT_EXPORT_DIR}/crates/cxx-kde-frameworks/initializers.o"
    "${CXX_QT_EXPORT_DIR}/crates/rclone-mounts-kcm/initializers.o"
    "${CXX_QT_EXPORT_DIR}/qml_modules/dev/jthoward/RcloneMounts/plugin_init.o"
)
target_sources(kcm_rclone_mounts PRIVATE ${_cxxqt_initializers})
set_source_files_properties(${_cxxqt_initializers} PROPERTIES
    EXTERNAL_OBJECT TRUE
    GENERATED TRUE
)
add_dependencies(kcm_rclone_mounts cargo-build_rclone_mounts_kcm)

# Install non-binary artifacts.
install(FILES kcm_rclone_mounts.desktop
    DESTINATION ${KDE_INSTALL_APPDIR})

# Service/unit files reference the helper's install path, which varies by
# platform (e.g. lib/libexec vs libexec); generate them rather than hardcoding it.
set(RCLONE_MOUNTS_HELPER_PATH "${KDE_INSTALL_FULL_LIBEXECDIR}/rclone-mounts-helper")

configure_file(data/dev.jthoward.RcloneMounts.Helper.service.in
    ${CMAKE_CURRENT_BINARY_DIR}/dev.jthoward.RcloneMounts.Helper.service
    @ONLY)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/dev.jthoward.RcloneMounts.Helper.service
    DESTINATION ${KDE_INSTALL_DATAROOTDIR}/dbus-1/system-services)

install(FILES data/dev.jthoward.RcloneMounts.Helper.conf
    DESTINATION ${KDE_INSTALL_DATAROOTDIR}/dbus-1/system.d)

install(FILES data/dev.jthoward.rclone-mounts.policy
    DESTINATION ${KDE_INSTALL_DATAROOTDIR}/polkit-1/actions)

configure_file(systemd/rclone-mounts-helper.service.in
    ${CMAKE_CURRENT_BINARY_DIR}/rclone-mounts-helper.service
    @ONLY)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/rclone-mounts-helper.service
    DESTINATION ${KDE_INSTALL_SYSTEMDUNITDIR}/system)

# Helper binary path matches what dev.jthoward.RcloneMounts.Helper.service expects.
install(PROGRAMS $<TARGET_FILE:rclone-mounts-helper>
    DESTINATION ${KDE_INSTALL_LIBEXECDIR}
    RENAME rclone-mounts-helper)
