use shadow_rs::ShadowBuilder;

fn main() {
    println!("cargo:rerun-if-changed=../../web/dist");
    ShadowBuilder::builder().build().unwrap();
}
