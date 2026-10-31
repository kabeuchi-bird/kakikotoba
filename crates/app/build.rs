use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    println!("cargo::rerun-if-changed=cpp/shim.h");
    CxxQtBuilder::new_qml_module(QmlModule::new("kakikotoba").qml_files(["qml/main.qml"]))
        .qt_module("Quick")
        .include_dir("cpp")
        .file("src/item.rs")
        .build();
}
