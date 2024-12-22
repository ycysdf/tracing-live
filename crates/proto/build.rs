fn main() {
    // println!("cargo:rerun-if-changed=proto/tracing.proto");
    // tonic_build::configure()
    //     .type_attribute("tracing.SpanInfo", "#[derive(Hash,Eq)]")
    //     .server_mod_attribute("tracing", "#[cfg(feature = \"server\")]")
    //     .server_attribute("tracing", "#[derive(PartialEq)]")
    //     .client_mod_attribute("tracing", "#[cfg(feature = \"client\")]")
    //     .client_attribute("tracing", "#[derive(PartialEq)]")
    //     .compile_protos(&["proto/tracing.proto"], &["proto"])
    //     .unwrap();
}
