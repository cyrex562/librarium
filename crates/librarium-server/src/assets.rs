use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../target/frontend/"]
pub struct Assets;
