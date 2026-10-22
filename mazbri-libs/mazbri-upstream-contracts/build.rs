fn main() {
    let protoc_path =
        protoc_bin_vendored::protoc_bin_path().expect("failed to locate vendored protoc binary");
    let mut prost_config = tonic_prost_build::Config::new();
    prost_config.protoc_executable(protoc_path);

    let proto_dir = "protos";
    let protos = [
        format!("{proto_dir}/shared/v1/common.proto"),
        format!("{proto_dir}/bypass_http_proxy/v1/service.proto"),
    ];

    println!("cargo:rerun-if-changed={proto_dir}");
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_with_config(prost_config, &protos, &[proto_dir.to_string()])
        .expect("failed to compile upstream proto definitions");
}
