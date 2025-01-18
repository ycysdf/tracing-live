fn main() {
    #[cfg(feature = "build-proto")]
    {
        use std::path::Path;
        use tonic_build::Config;
        println!("cargo:rerun-if-changed=proto/tracing.proto");
        let mut config = Config::new();
        let workspace_dir: &Path = Path::new(env!("CARGO_WORKSPACE_DIR"));
        config.bytes(["tracing"]);
        #[cfg(feature = "build-proto-dev")]
        config.out_dir(workspace_dir.join("crates/core/src"));
        tonic_build::configure()
            .type_attribute("tracing.SpanInfo", "#[derive(Hash,Eq)]")
            .server_mod_attribute("tracing", "#[cfg(feature = \"server\")]")
            .server_attribute("tracing", "#[derive(PartialEq)]")
            .client_mod_attribute("tracing", "#[cfg(feature = \"client\")]")
            .client_attribute("tracing", "#[derive(PartialEq)]")
            .compile_protos_with_config(config, &["proto/tracing.proto"], &["proto"])
            .unwrap();
    }
}
