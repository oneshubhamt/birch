mod attachment_protocol;

use dioxus::prelude::{Asset, Element, asset, manganis, use_context};
use rustnote::ui::App as RustnoteApp;

static APP_STYLES: Asset = asset!("/assets/main.css");
static KATEX_STYLES: Asset = asset!("/assets/katex-embedded.css");
static EDITOR_RUNTIME: Asset = asset!("/assets/prosemirror-editor.js");

fn app() -> Element {
    let libraries = use_context::<rustnote::library::LibraryRuntime>();
    let attachment_libraries = libraries.clone();
    let pdf_libraries = libraries.clone();
    // Images and PDF resources stay on Dioxus's app protocol. Audio/video
    // descriptors prefer Rustnote's loopback HTTP range server so WebKit gets
    // normal media transport semantics; this handler remains a safe fallback.
    dioxus_desktop::use_asset_handler("rustnoteattachment", move |request, responder| {
        let library = attachment_libraries.current();
        responder.respond(attachment_protocol::attachment_asset_response(
            request,
            library.as_deref(),
        ));
    });
    dioxus_desktop::use_asset_handler("rustnotepdf", move |request, responder| {
        let library = pdf_libraries.current();
        responder.respond(attachment_protocol::pdf_asset_response(
            request,
            library.as_deref(),
        ));
    });
    RustnoteApp()
}

fn main() {
    eprintln!(
        "Rustnote {} starting",
        rustnote::build_info::FULL_BUILD_IDENTIFIER
    );
    // Sensitive browser/native staging is part of the Vault plaintext boundary.
    // Do not start serving libraries if abandoned plaintext cannot be inspected
    // and removed deterministically after a previous crash.
    if let Err(error) = rustnote::native::output::recover_stale_output_transactions() {
        eprintln!("Rustnote cannot start because external output recovery failed: {error}");
        return;
    }
    if let Err(error) = rustnote::native::staging::cleanup_stale_pdf_exports() {
        eprintln!("Rustnote cannot start because PDF staging privacy cleanup failed: {error}");
        return;
    }
    if let Err(error) = rustnote::native::staging::cleanup_stale_image_stages() {
        eprintln!("Rustnote cannot start because image staging privacy cleanup failed: {error}");
        return;
    }
    if let Err(error) = rustnote::native::staging::cleanup_stale_archive_restore_stages() {
        eprintln!(
            "Rustnote cannot start because archive restore staging privacy cleanup failed: {error}"
        );
        return;
    }
    let library_runtime = rustnote::library::LibraryRuntime::new();
    if let Err(error) = rustnote::media_server::start(library_runtime.clone()) {
        eprintln!("Rustnote inline media server unavailable: {error}");
    }
    let custom_head = format!(
        r#"<link rel="stylesheet" href="{}"><link rel="stylesheet" href="{}"><script src="{}"></script>"#,
        APP_STYLES, KATEX_STYLES, EDITOR_RUNTIME,
    );
    let config = dioxus_desktop::Config::new()
        .with_custom_head(custom_head)
        .with_disable_drag_drop_handler(true)
        .with_background_color((246, 247, 244, 255))
        .with_window(dioxus_desktop::WindowBuilder::new().with_title("Rustnote"));

    dioxus::LaunchBuilder::new()
        .with_context(library_runtime)
        .with_cfg(config)
        .launch(app);
}
