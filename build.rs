fn main() {
    embed_resource::compile("stock_icon.rc", embed_resource::NONE).manifest_optional().unwrap();
}
