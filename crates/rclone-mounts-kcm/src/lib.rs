// SPDX-License-Identifier: GPL-2.0-or-later

// Force linkage of all cxx-qt-built crates whose initializers we link directly
// via .o files in CMake. Without these `extern crate` lines, the staticlib drops
// crates that have no direct symbol usage, leaving undefined references at link
// time for cxx_qt_init_crate_<name>.
extern crate cxx_qt;
extern crate cxx_qt_lib;
extern crate cxx_qt_lib_extras;
extern crate cxx_kde_frameworks;

pub mod backend_controller;
pub mod mount_model;
pub mod remote_model;
